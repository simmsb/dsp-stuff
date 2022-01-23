use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
};

use collect_slice::CollectSlice;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Sample, SampleRate,
};
use itertools::Itertools;
use rivulet::{
    circular_buffer::{Sink, Source},
    splittable, SplittableView, View, ViewMut,
};

use crate::ids::DeviceId;

type DeviceCmdChan = std::sync::mpsc::SyncSender<(DeviceCommand, oneshot::Sender<DeviceResponse>)>;

lazy_static::lazy_static! {
    static ref DEVICE_CMD_CHAN: DeviceCmdChan = {
        let (sender, receiver): (_, std::sync::mpsc::Receiver<(DeviceCommand, oneshot::Sender<DeviceResponse>)>) = std::sync::mpsc::sync_channel(1);

        std::thread::spawn(move || {
            let mut devices: HashMap<DeviceId, cpal::Stream> = HashMap::new();
            let mut resync_flags: HashMap<DeviceId, Arc<AtomicBool>> = HashMap::new();

            for (cmd, resp_chan) in receiver {
                match cmd {
                    DeviceCommand::ListHosts => {
                        resp_chan.send(DeviceResponse::Hosts(cpal::available_hosts())).unwrap();
                    },
                    DeviceCommand::ListInputs(host) => {
                        let host = cpal::host_from_id(cpal::available_hosts()
                                                      .into_iter()
                                                      .find(|id| *id == host)
                                                      .unwrap())
                            .unwrap();

                        let devices = host.input_devices().unwrap()
                                .filter_map(|d| d.name().ok())
                                .collect::<Vec<_>>();

                        resp_chan.send(DeviceResponse::Devices(devices)).unwrap();
                    },
                    DeviceCommand::ListOutputs(host) => {
                        let host = cpal::host_from_id(cpal::available_hosts()
                                                      .into_iter()
                                                      .find(|id| *id == host)
                                                      .unwrap())
                            .unwrap();

                        let devices = host.output_devices().unwrap()
                                .filter_map(|d| d.name().ok())
                                .collect::<Vec<_>>();

                        resp_chan.send(DeviceResponse::Devices(devices)).unwrap();
                    },
                    DeviceCommand::OpenInput(host, dev) => {
                        tracing::info!("Opening input device {dev:?}");
                        let host = cpal::host_from_id(cpal::available_hosts()
                                                      .into_iter()
                                                      .find(|id| *id == host)
                                                      .unwrap())
                            .unwrap();

                        let device = host.input_devices().unwrap()
                                .find(|d| d.name().ok().as_ref() == Some(&dev))
                                .unwrap();

                        let r = match input_stream(device) {
                            Ok((stream, source)) => {
                                stream.play().unwrap();
                                let id = DeviceId::generate();
                                devices.insert(id, stream);

                                Some((id, source))
                            },
                            Err(e) => {
                                tracing::error!("Opening input failed: {:#}", e);
                                None
                            },
                        };

                        resp_chan.send(DeviceResponse::InputOpened(r)).unwrap();
                    },
                    DeviceCommand::OpenOutput(host, dev) => {
                        tracing::info!("Opening output device {dev:?}");
                        let host = cpal::host_from_id(cpal::available_hosts()
                                                      .into_iter()
                                                      .find(|id| *id == host)
                                                      .unwrap())
                            .unwrap();

                        let device = host.output_devices().unwrap()
                                .find(|d| d.name().ok().as_ref() == Some(&dev))
                                .unwrap();

                        let r = match output_stream(device) {
                            Ok((stream, sink, resync)) => {
                                stream.play().unwrap();
                                let id = DeviceId::generate();
                                devices.insert(id, stream);
                                resync_flags.insert(id, resync);

                                Some((id, sink))
                            },
                            Err(e) => {
                                tracing::error!("Opening output failed: {:#}", e);
                                None
                            },
                        };

                        resp_chan.send(DeviceResponse::OutputOpened(r)).unwrap();
                    },
                    DeviceCommand::CloseDevice(dev) => {
                        tracing::info!("Closing device {dev:?}");

                        if let Some(dev) = devices.remove(&dev) {
                            let _ = dev.pause();
                        }

                        resp_chan.send(DeviceResponse::DeviceClosed).unwrap();
                    },
                    DeviceCommand::TriggerResync => {
                        for flag in resync_flags.values() {
                            flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        }

                        resp_chan.send(DeviceResponse::Resynced).unwrap();
                    }
                }
            }
        });

        sender
    };
}

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

fn do_read_1<T: Sample>(data: &[T], sink: &mut Sink<f32>) {
    if sink.try_grant(data.len()).unwrap() {
        let buf = sink.view_mut();
        data.iter()
            .map(Sample::to_f32)
            .collect_slice(&mut buf[..data.len()]);
        sink.release(data.len());
    } else {
        // println!("input fuck");
        // input will fall behind
    };
}

fn do_read_2<T: Sample>(data: &[T], sink: &mut Sink<f32>) {
    let buf_len = data.len() / 2;
    if sink.try_grant(buf_len).unwrap() {
        let buf = sink.view_mut();
        data.iter()
            .step_by(2)
            .map(Sample::to_f32)
            .collect_slice(&mut buf[..buf_len]);
        sink.release(data.len());
    } else {
        // println!("input fuck");
        // input will fall behind
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
        1 => match fmt {
            cpal::SampleFormat::I16 => dev.build_input_stream(
                &cfg,
                move |data: &[i16], _| do_read_1(data, &mut sink),
                err_cb,
            )?,
            cpal::SampleFormat::U16 => dev.build_input_stream(
                &cfg,
                move |data: &[u16], _| do_read_1(data, &mut sink),
                err_cb,
            )?,
            cpal::SampleFormat::F32 => dev.build_input_stream(
                &cfg,
                move |data: &[f32], _| do_read_1(data, &mut sink),
                err_cb,
            )?,
        },
        2 => match fmt {
            cpal::SampleFormat::I16 => dev.build_input_stream(
                &cfg,
                move |data: &[i16], _| do_read_2(data, &mut sink),
                err_cb,
            )?,
            cpal::SampleFormat::U16 => dev.build_input_stream(
                &cfg,
                move |data: &[u16], _| do_read_2(data, &mut sink),
                err_cb,
            )?,
            cpal::SampleFormat::F32 => dev.build_input_stream(
                &cfg,
                move |data: &[f32], _| do_read_2(data, &mut sink),
                err_cb,
            )?,
        },
        n => {
            return Err(color_eyre::eyre::eyre!(
                "I don't know how to support devices with {} channels, idk complain on github",
                n
            ));
        }
    };

    Ok((stream, source.into_view()))
}

fn do_write_1<T: Sample>(
    data: &mut [T],
    source: &mut splittable::View<Source<f32>>,
    trigger_catchup: &mut Arc<AtomicBool>,
) {
    if source.try_grant(data.len()).unwrap() {
        let buf = source.view();

        let offs = buf.len() - data.len();

        let allowed_latency = 2;

        if trigger_catchup.swap(false, std::sync::atomic::Ordering::Relaxed)
            && offs >= (data.len() * allowed_latency)
        {
            tracing::debug!("Skipping {} samples so the output catches up", offs);
            buf[offs..][..data.len()]
                .iter()
                .map(<T as Sample>::from)
                .collect_slice(data);
            let len = buf.len();
            source.release(len);
        } else {
            buf[..data.len()]
                .iter()
                .map(<T as Sample>::from)
                .collect_slice(data);
            source.release(data.len());
        }
    } else {
        data.fill(<T as Sample>::from(&0.0f32));
        // println!("output fuck");
        // oops
    };
}

fn do_write_2<T: Sample>(
    data: &mut [T],
    source: &mut splittable::View<Source<f32>>,
    trigger_catchup: &mut Arc<AtomicBool>,
) {
    let buf_len = data.len() / 2;

    if source.try_grant(buf_len).unwrap() {
        let buf = source.view();

        let offs = buf.len() - buf_len;

        let allowed_latency = 2;

        if trigger_catchup.swap(false, std::sync::atomic::Ordering::Relaxed)
            && offs >= (buf_len * allowed_latency)
        {
            tracing::debug!("Skipping {} samples so the output catches up", offs);
            Itertools::intersperse(
                buf[offs..][..buf_len].iter().map(<T as Sample>::from),
                <T as Sample>::from(&0.0f32),
            )
            .collect_slice(data);
            let len = buf.len();
            source.release(len);
        } else {
            Itertools::intersperse(
                buf[..buf_len].iter().map(<T as Sample>::from),
                <T as Sample>::from(&0.0f32),
            )
            .collect_slice(data);
            source.release(buf_len);
        }
    } else {
        data.fill(<T as Sample>::from(&0.0f32));
        // println!("output fuck");
        // oops
    };
}

fn output_stream(
    dev: cpal::Device,
) -> color_eyre::Result<(cpal::Stream, Sink<f32>, Arc<AtomicBool>)> {
    let (cfg, fmt) = if let Some(cfg) = dev
        .supported_output_configs()?
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

    tracing::info!(?cfg, "Selected output cfg");

    let (sink, source) = rivulet::circular_buffer::<f32>(8192);
    let mut source = source.into_view();

    let err_cb = |err| tracing::warn!("output message: {:#?}", err);

    let mut trigger_catchup = Arc::new(AtomicBool::new(false));
    let trigger_catchup_out = Arc::clone(&trigger_catchup);

    let stream = match cfg.channels {
        1 => match fmt {
            cpal::SampleFormat::I16 => dev.build_output_stream(
                &cfg,
                move |data: &mut [i16], _| do_write_1(data, &mut source, &mut trigger_catchup),
                err_cb,
            )?,
            cpal::SampleFormat::U16 => dev.build_output_stream(
                &cfg,
                move |data: &mut [u16], _| do_write_1(data, &mut source, &mut trigger_catchup),
                err_cb,
            )?,
            cpal::SampleFormat::F32 => dev.build_output_stream(
                &cfg,
                move |data: &mut [f32], _| do_write_1(data, &mut source, &mut trigger_catchup),
                err_cb,
            )?,
        },
        2 => match fmt {
            cpal::SampleFormat::I16 => dev.build_output_stream(
                &cfg,
                move |data: &mut [i16], _| do_write_2(data, &mut source, &mut trigger_catchup),
                err_cb,
            )?,
            cpal::SampleFormat::U16 => dev.build_output_stream(
                &cfg,
                move |data: &mut [u16], _| do_write_2(data, &mut source, &mut trigger_catchup),
                err_cb,
            )?,
            cpal::SampleFormat::F32 => dev.build_output_stream(
                &cfg,
                move |data: &mut [f32], _| do_write_2(data, &mut source, &mut trigger_catchup),
                err_cb,
            )?,
        },
        n => {
            return Err(color_eyre::eyre::eyre!(
                "I don't know how to support devices with {} channels, idk complain on github",
                n
            ));
        }
    };

    Ok((stream, sink, trigger_catchup_out))
}
