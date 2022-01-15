use std::collections::HashMap;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
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
                            Ok((stream, sink)) => {
                                stream.play().unwrap();
                                let id = DeviceId::generate();
                                devices.insert(id, stream);

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
}

pub enum DeviceResponse {
    Hosts(Vec<cpal::HostId>),
    Devices(Vec<String>),
    InputOpened(Option<(DeviceId, splittable::View<Source<f32>>)>),
    OutputOpened(Option<(DeviceId, Sink<f32>)>),
    DeviceClosed,
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

fn input_stream(
    dev: cpal::Device,
) -> color_eyre::Result<(cpal::Stream, splittable::View<Source<f32>>)> {
    let cfg = dev.default_input_config()?;
    println!("{:#?}, {:#?}", cfg, cfg.config());

    let lowest_buf_size = match cfg.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max: _ } => *min,
        cpal::SupportedBufferSize::Unknown => 1024, // shrug
    };

    let mut cfg_v = cfg.config();
    cfg_v.channels = 1;

    cfg_v.buffer_size = match cfg_v.buffer_size {
        cpal::BufferSize::Default => cpal::BufferSize::Fixed(lowest_buf_size),
        x @ cpal::BufferSize::Fixed(_) => x,
    };

    println!("cfg: {:#?}", cfg_v);

    println!("using buf size: {}", lowest_buf_size);

    let (mut sink, source) = rivulet::circular_buffer::<f32>(lowest_buf_size as usize * 8);

    // TODO: sample type conversion

    let stream = dev.build_input_stream(
        &cfg_v,
        move |data: &[f32], _| {
            if sink.try_grant(data.len()).unwrap() {
                let buf = sink.view_mut();
                buf[..data.len()].copy_from_slice(data);
                sink.release(data.len());
            } else {
                // println!("input fuck");
                // input will fall behind
            };
        },
        move |err| {
            eprintln!("input oops: {:#?}", err);
        },
    )?;

    Ok((stream, source.into_view()))
}

fn output_stream(dev: cpal::Device) -> color_eyre::Result<(cpal::Stream, Sink<f32>)> {
    let cfg = dev.default_output_config()?;
    println!("{:#?}, {:#?}", cfg, cfg.config());

    let lowest_buf_size = match cfg.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max: _ } => *min,
        cpal::SupportedBufferSize::Unknown => 1024, // shrug
    };

    let mut cfg_v = cfg.config();
    cfg_v.channels = 1;

    cfg_v.buffer_size = match cfg_v.buffer_size {
        cpal::BufferSize::Default => cpal::BufferSize::Fixed(lowest_buf_size),
        x @ cpal::BufferSize::Fixed(_) => x,
    };

    println!("cfg: {:#?}", cfg_v);

    println!("using buf size: {}", lowest_buf_size);

    let (sink, source) = rivulet::circular_buffer::<f32>(lowest_buf_size as usize * 8);
    let mut source = source.into_view();

    let stream = dev.build_output_stream(
        &cfg_v,
        move |data: &mut [f32], _| {
            if source.try_grant(data.len()).unwrap() {
                let buf = source.view();
                data.copy_from_slice(&buf[..data.len()]);
                source.release(data.len());
            } else {
                // println!("output fuck");
                // oops
            };
        },
        move |err| {
            eprintln!("output oops: {:#?}", err);
        },
    )?;

    Ok((stream, sink))
}
