use std::{
    collections::HashMap,
    sync::{atomic::AtomicU8, Arc},
};

use collect_slice::CollectSlice;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Sample, SampleRate,
};
use dasp_interpolate::sinc::Sinc;
use dasp_sample::{FromSample, ToSample};
use dasp_signal::{interpolate::Converter, Signal};
use itertools::Itertools;
use once_cell::sync::Lazy;
use rivulet::{
    circular_buffer::{Sink, Source},
    splittable, SplittableView, View, ViewMut,
};

use crate::ids::DeviceId;

type DeviceCmdChan = std::sync::mpsc::SyncSender<(DeviceCommand, oneshot::Sender<DeviceResponse>)>;

static DEVICE_CMD_CHAN: Lazy<DeviceCmdChan> = Lazy::new(|| {
    let (sender, receiver): (
        _,
        std::sync::mpsc::Receiver<(DeviceCommand, oneshot::Sender<DeviceResponse>)>,
    ) = std::sync::mpsc::sync_channel(1);

    std::thread::spawn(move || {
        let mut devices: HashMap<DeviceId, cpal::Stream> = HashMap::new();
        let mut resync_counters: HashMap<DeviceId, Arc<AtomicU8>> = HashMap::new();

        for (cmd, resp_chan) in receiver {
            match cmd {
                DeviceCommand::ListHosts => {
                    resp_chan
                        .send(DeviceResponse::Hosts(cpal::available_hosts()))
                        .unwrap();
                }
                DeviceCommand::ListInputs(host) => {
                    let host = cpal::host_from_id(
                        cpal::available_hosts()
                            .into_iter()
                            .find(|id| *id == host)
                            .unwrap(),
                    )
                    .unwrap();

                    let devices = host
                        .input_devices()
                        .unwrap()
                        .filter_map(|d| d.name().ok())
                        .collect::<Vec<_>>();

                    resp_chan.send(DeviceResponse::Devices(devices)).unwrap();
                }
                DeviceCommand::ListOutputs(host) => {
                    let host = cpal::host_from_id(
                        cpal::available_hosts()
                            .into_iter()
                            .find(|id| *id == host)
                            .unwrap(),
                    )
                    .unwrap();

                    let devices = host
                        .output_devices()
                        .unwrap()
                        .filter_map(|d| d.name().ok())
                        .collect::<Vec<_>>();

                    resp_chan.send(DeviceResponse::Devices(devices)).unwrap();
                }
                DeviceCommand::OpenInput(host, dev) => {
                    tracing::info!("Opening input device {dev:?}");
                    let host = cpal::host_from_id(
                        cpal::available_hosts()
                            .into_iter()
                            .find(|id| *id == host)
                            .unwrap(),
                    )
                    .unwrap();

                    let device = host
                        .input_devices()
                        .unwrap()
                        .find(|d| d.name().ok().as_ref() == Some(&dev))
                        .unwrap();

                    let r = match input_stream(device) {
                        Ok((stream, source)) => {
                            stream.play().unwrap();
                            let id = DeviceId::generate();
                            devices.insert(id, stream);

                            Some((id, source))
                        }
                        Err(e) => {
                            tracing::error!("Opening input failed: {:#}", e);
                            None
                        }
                    };

                    resp_chan.send(DeviceResponse::InputOpened(r)).unwrap();
                }
                DeviceCommand::OpenOutput(host, dev) => {
                    tracing::info!("Opening output device {dev:?}");
                    let host = cpal::host_from_id(
                        cpal::available_hosts()
                            .into_iter()
                            .find(|id| *id == host)
                            .unwrap(),
                    )
                    .unwrap();

                    let device = host
                        .output_devices()
                        .unwrap()
                        .find(|d| d.name().ok().as_ref() == Some(&dev))
                        .unwrap();

                    let r = match output_stream(device) {
                        Ok((stream, sink, resync)) => {
                            stream.play().unwrap();
                            let id = DeviceId::generate();
                            devices.insert(id, stream);
                            resync_counters.insert(id, resync);

                            Some((id, sink))
                        }
                        Err(e) => {
                            tracing::error!("Opening output failed: {:#}", e);
                            None
                        }
                    };

                    resp_chan.send(DeviceResponse::OutputOpened(r)).unwrap();
                }
                DeviceCommand::CloseDevice(dev) => {
                    tracing::info!("Closing device {dev:?}");

                    if let Some(dev) = devices.remove(&dev) {
                        let _ = dev.pause();
                    }

                    resp_chan.send(DeviceResponse::DeviceClosed).unwrap();
                }
                DeviceCommand::TriggerResync => {
                    for counter in resync_counters.values() {
                        counter.fetch_add(5, std::sync::atomic::Ordering::Relaxed);
                    }

                    resp_chan.send(DeviceResponse::Resynced).unwrap();
                }
            }
        }
    });

    sender
});

pub fn invoke(cmd: DeviceCommand) -> DeviceResponse {
    let (resp_in, resp_out) = oneshot::channel();
    DEVICE_CMD_CHAN.send((cmd, resp_in)).unwrap();
    resp_out.recv().unwrap()
}

pub enum DeviceCommand {
    ListHosts,
    ListInputs(cpal::HostId),
    ListOutputs(cpal::HostId),
    OpenInput(cpal::HostId, String),
    OpenOutput(cpal::HostId, String),
    CloseDevice(DeviceId),
    TriggerResync,
}

pub enum DeviceResponse {
    Hosts(Vec<cpal::HostId>),
    Devices(Vec<String>),
    InputOpened(Option<(DeviceId, splittable::View<Source<f32>>)>),
    OutputOpened(Option<(DeviceId, Sink<f32>)>),
    DeviceClosed,
    Resynced,
}

impl DeviceResponse {
    pub fn hosts(self) -> Option<Vec<cpal::HostId>> {
        match self {
            Self::Hosts(x) => Some(x),
            _ => None,
        }
    }

    pub fn devices(self) -> Option<Vec<String>> {
        match self {
            Self::Devices(x) => Some(x),
            _ => None,
        }
    }

    pub fn input_opened(self) -> Option<Option<(DeviceId, splittable::View<Source<f32>>)>> {
        match self {
            Self::InputOpened(v) => Some(v),
            _ => None,
        }
    }

    pub fn output_opened(self) -> Option<Option<(DeviceId, Sink<f32>)>> {
        match self {
            Self::OutputOpened(v) => Some(v),
            _ => None,
        }
    }

    #[allow(unused)]
    pub fn device_closed(self) -> Option<()> {
        match self {
            Self::DeviceClosed => Some(()),
            _ => None,
        }
    }
}

fn do_read_1<T>(data: &[T], sink: &mut Sink<f32>)
where
    T: Sample + ToSample<f32>,
{
    if sink.try_grant(data.len()).unwrap() {
        let buf = sink.view_mut();
        data.iter()
            .copied()
            .map(<T as Sample>::to_sample)
            .collect_slice(&mut buf[..data.len()]);
        sink.release(data.len());
    } else {
        // println!("input fuck");
        // input will fall behind
    };
}

fn do_read_2<T>(data: &[T], sink: &mut Sink<f32>)
where
    T: Sample + ToSample<f32>,
{
    let buf_len = data.len() / 2;
    if sink.try_grant(buf_len).unwrap() {
        let buf = sink.view_mut();
        data.iter()
            .copied()
            .map(<T as Sample>::to_sample)
            .array_chunks::<2>()
            .map(|[a, b]| a + b)
            .collect_slice(&mut buf[..buf_len]);
        sink.release(buf_len);
    } else {
        // println!("input fuck");
        // input will fall behind
    };
}

macro_rules! handle_inps {
    ($fmt:ident, $dev:ident, $cfg:ident, $read_fn:ident, $sink:ident, $err_cb:ident, $($typ:ty: $tyn:tt),*) => {
        match $fmt {
            $(
                cpal::SampleFormat::$tyn => { $dev.build_input_stream(&$cfg, move |data: &[$typ], _| $read_fn(data, &mut $sink), $err_cb, None)? }
            ),*
                f => { return Err(::color_eyre::eyre::eyre!("I don't know how to handle {} samples", f)) }
        }
    };
}

fn input_stream(
    dev: cpal::Device,
) -> color_eyre::Result<(cpal::Stream, splittable::View<Source<f32>>)> {
    let (cfg, fmt) = if let Some(cfg) = dev
        .supported_input_configs()?
        .filter(|cfg| {
            cfg.min_sample_rate() <= SampleRate(48000) && cfg.max_sample_rate() >= SampleRate(48000)
        })
        .sorted_by_key(|cfg| cfg.channels())
        .next()
    {
        let cfg = cfg.with_sample_rate(SampleRate(48000));
        // let buf_size = match cfg.buffer_size() {
        //     cpal::SupportedBufferSize::Range { min, max: _ } => BufferSize::Fixed(*min),
        //     cpal::SupportedBufferSize::Unknown => BufferSize::Default,
        // };
        let fmt = cfg.sample_format();
        let cfg = cfg.config();
        // let mut cfg = cfg.config();
        // cfg.buffer_size = buf_size;

        (cfg, fmt)
    } else {
        return Err(color_eyre::eyre::eyre!(
            "Couldn't find a valid config for device"
        ));
    };

    tracing::info!(?cfg, "Selected input cfg");

    let (mut sink, source) = rivulet::circular_buffer::<f32>(8192);

    let err_cb = |err| tracing::warn!("output message: {:#?}", err);

    let stream = match cfg.channels {
        1 => handle_inps!(
            fmt,
            dev,
            cfg,
            do_read_1,
            sink,
            err_cb,
            i8: I8,
            i16: I16,
            i32: I32,
            i64: I64,
            u8: U8,
            u16: U16,
            u32: U32,
            u64: U64,
            f32: F32,
            f64: F64
        ),
        2 => handle_inps!(
            fmt,
            dev,
            cfg,
            do_read_2,
            sink,
            err_cb,
            i8: I8,
            i16: I16,
            i32: I32,
            i64: I64,
            u8: U8,
            u16: U16,
            u32: U32,
            u64: U64,
            f32: F32,
            f64: F64
        ),
        n => {
            return Err(color_eyre::eyre::eyre!(
                "I don't know how to support devices with {} channels, idk complain on github",
                n
            ));
        }
    };

    Ok((stream, source.into_view()))
}

struct CountingSignal {
    index: usize,
    inner: Vec<f32>,
}

impl CountingSignal {
    fn new() -> Self {
        Self {
            index: 0,
            inner: Vec::new(),
        }
    }

    fn prep(&mut self, buf: &[f32]) {
        self.inner.clear();
        self.inner.extend_from_slice(buf);
        self.index = 0;
    }
}

impl dasp_signal::Signal for CountingSignal {
    type Frame = f32;

    fn next(&mut self) -> Self::Frame {
        if let Some(x) = self.inner.get(self.index) {
            self.index += 1;
            *x
        } else {
            0.0
        }
    }

    fn is_exhausted(&self) -> bool {
        self.index > self.inner.len()
    }
}

fn do_write_1<T: Sample + FromSample<f32> + dasp_frame::Frame>(
    data: &mut [T],
    source: &mut splittable::View<Source<f32>>,
    trigger_catchup: &mut Arc<AtomicU8>,
    target_sample_rate: usize,
    mut resampler: &mut Converter<CountingSignal, Sinc<[f32; 16]>>,
) {
    let input_len = (data.len() as f32 * (48_000.0 / target_sample_rate as f32)) as usize;

    if source.try_grant(input_len).unwrap() {
        let input_view = source.view();

        let offs = input_view.len() - input_len;

        let allowed_latency = 2;

        if (trigger_catchup
            .fetch_update(
                atomig::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |x| Some(x.saturating_sub(1)),
            )
            .unwrap()
            > 0)
            && offs >= (input_len * allowed_latency)
        {
            tracing::debug!("Skipping {} samples so the output catches up", offs);
            resampler.source_mut().prep(&input_view[offs..]);

            Signal::until_exhausted(resampler)
                .map(|x| <T as Sample>::from_sample(x))
                .collect_slice(data);
            let len = input_view.len();
            source.release(len);
        } else {
            resampler.source_mut().prep(input_view);

            Signal::until_exhausted(&mut resampler)
                .map(|x| <T as Sample>::from_sample(x))
                .collect_slice(data);
            source.release(resampler.source().index);
        }
    } else {
        data.fill(<T as Sample>::from_sample(0.0f32));
        // println!("output fuck");
        // oops
    };
}

fn do_write_2<T: Sample + FromSample<f32>>(
    data: &mut [T],
    source: &mut splittable::View<Source<f32>>,
    trigger_catchup: &mut Arc<AtomicU8>,
    target_sample_rate: usize,
    resampler: &mut Converter<CountingSignal, Sinc<[f32; 16]>>,
) {
    let input_len = ((data.len() / 2) as f32 * (48_000.0 / target_sample_rate as f32)) as usize;

    if source.try_grant(input_len).unwrap() {
        let input_view = source.view();

        let offs = input_view.len() - input_len;

        let allowed_latency = 2;

        if (trigger_catchup
            .fetch_update(
                atomig::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |x| Some(x.saturating_sub(1)),
            )
            .unwrap()
            > 0)
            && offs >= (input_len * allowed_latency)
        {
            tracing::info!(
                "Skipping {} samples so the output catches up (max buffer: {})",
                offs,
                input_len * allowed_latency
            );
            resampler.source_mut().prep(&input_view[offs..]);

            for o in data.chunks_mut(2) {
                let x = <T as Sample>::from_sample(resampler.next());

                o.fill(x);
            }

            let len = input_view.len();
            source.release(len);
        } else {
            resampler.source_mut().prep(input_view);

            for o in data.chunks_mut(2) {
                let x = <T as Sample>::from_sample(resampler.next());

                o.fill(x);
            }

            source.release(resampler.source().index);
        }
    } else {
        data.fill(<T as Sample>::from_sample(0.0f32));
        // println!("output fuck");
        // oops
    };
}

macro_rules! handle_outs {
    ($fmt:ident, $dev:ident, $cfg:ident, $write_fn:ident, $source:ident, $trigger_catchup:ident, $target_sample_rate:ident, $resampler:ident, $err_cb:ident, $($typ:ty: $tyn:tt),*) => {
        match $fmt {
            $(
                cpal::SampleFormat::$tyn => { $dev.build_output_stream(&$cfg, move |data: &mut [$typ], _| $write_fn(data, &mut $source, &mut $trigger_catchup, $target_sample_rate, &mut $resampler), $err_cb, None)? }
            ),*
                f => { return Err(::color_eyre::eyre::eyre!("I don't know how to handle {} samples", f)) }
        }
    };
}

fn output_stream(
    dev: cpal::Device,
) -> color_eyre::Result<(cpal::Stream, Sink<f32>, Arc<AtomicU8>)> {
    let (cfg, fmt) = if let Some(cfg) = dev
        .supported_output_configs()?
        .sorted_by_key(|cfg| (cfg.channels(), cfg.max_sample_rate().0.abs_diff(48_000)))
        .next()
    {
        let cfg = cfg.with_max_sample_rate();
        // let buf_size = match cfg.buffer_size() {
        //     cpal::SupportedBufferSize::Range { min, max: _ } => BufferSize::Fixed(*min),
        //     cpal::SupportedBufferSize::Unknown => BufferSize::Default,
        // };
        let fmt = cfg.sample_format();
        let cfg = cfg.config();
        // let mut cfg = cfg.config();
        // cfg.buffer_size = buf_size;

        (cfg, fmt)
    } else {
        return Err(color_eyre::eyre::eyre!(
            "Couldn't find a valid config for device, supported: {:#?}",
            dev.supported_output_configs().unwrap().collect::<Vec<_>>()
        ));
    };

    tracing::info!(?cfg, "Selected output cfg");

    let (sink, source) = rivulet::circular_buffer::<f32>(8192);
    let mut source = source.into_view();

    let err_cb = |err| tracing::warn!("output message: {:#?}", err);

    let mut trigger_catchup = Arc::new(AtomicU8::new(0));
    let trigger_catchup_out = Arc::clone(&trigger_catchup);

    let target_sample_rate = cfg.sample_rate.0 as usize;
    let sinc = Sinc::new(dasp_ring_buffer::Fixed::from([0.0; 16]));
    let mut resampler = Converter::from_hz_to_hz(
        CountingSignal::new(),
        sinc,
        48_000.0,
        target_sample_rate as f64,
    );

    let stream = match cfg.channels {
        1 => handle_outs!(
            fmt,
            dev,
            cfg,
            do_write_1,
            source,
            trigger_catchup,
            target_sample_rate,
            resampler,
            err_cb,
            i8: I8,
            i16: I16,
            i32: I32,
            i64: I64,
            u8: U8,
            u16: U16,
            u32: U32,
            u64: U64,
            f32: F32,
            f64: F64
        ),
        2 => handle_outs!(
            fmt,
            dev,
            cfg,
            do_write_2,
            source,
            trigger_catchup,
            target_sample_rate,
            resampler,
            err_cb,
            i8: I8,
            i16: I16,
            i32: I32,
            i64: I64,
            u8: U8,
            u16: U16,
            u32: U32,
            u64: U64,
            f32: F32,
            f64: F64
        ),
        n => {
            return Err(color_eyre::eyre::eyre!(
                "I don't know how to support devices with {} channels, idk complain on github",
                n
            ));
        }
    };

    Ok((stream, sink, trigger_catchup_out))
}
