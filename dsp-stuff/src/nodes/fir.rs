use std::collections::VecDeque;
use eframe::egui;
use std::iter::zip;
use std::sync::Mutex;
use atomig::Atomic;
use dasp_interpolate::sinc::Sinc;
use dasp_signal::Signal;
use egui::Ui;
use serde::{Deserialize, Serialize};
use symphonia_core::audio::SampleBuffer;
use symphonia_core::formats::FormatOptions;
use symphonia_core::io::MediaSourceStream;
use symphonia_core::meta::MetadataOptions;
use symphonia_core::probe::Hint;

use crate::ids::NodeId;
use crate::node::{PortStorage, SimpleNode};

#[derive(
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    atomig::Atom,
    strum::EnumIter,
    strum::IntoStaticStr,
    Clone,
    Copy,
)]
#[repr(u8)]
enum Mode {
    Average,
    Balanced,
}

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    input = "in",
    output = "out",
    title = "FIR Filter",
    cfg_name = "fir",
    description = "Perform a FIR operation",
    custom_render = "Fir::render"
)]
pub struct Fir {
    #[dsp(id)]
    id: NodeId,

    #[dsp(inputs)]
    inputs: PortStorage,

    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(select, save, default = "Mode::Balanced")]
    mode: Atomic<Mode>,

    #[dsp(default = "Mutex::new(None)", save)]
    file_name: Mutex<Option<String>>,

    #[dsp(default = "Mutex::new(vec![1.0])", save)]
    taps: Mutex<Vec<f64>>,

    #[dsp(default = "Mutex::new(VecDeque::new())")]
    state: Mutex<VecDeque<f64>>,
}

impl Fir {
    fn render(&self, ui: &mut Ui) {
        let mut file_name = self.file_name.lock().unwrap();

        ui.label(if let Some(name) = &*file_name {
            format!("Loaded IR: {name}")
        } else {
            "No IR loaded".to_owned()
        });

        if ui.button("Set Impulse").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Load IR")
                .add_filter("wave file", &["wav"])
                .pick_file()
            {
                tracing::info!("loading IR from file {:?}", path);

                let f = std::fs::File::open(&path).unwrap();

                let mss = MediaSourceStream::new(Box::new(f), Default::default());

                let hint = Hint::new();

                // Use the default options when reading and decoding.
                let format_opts: FormatOptions = Default::default();
                let metadata_opts: MetadataOptions = Default::default();
                // Probe the media source stream for a format.
                let probed = symphonia::default::get_probe()
                    .format(&hint, mss, &format_opts, &metadata_opts)
                    .unwrap();

                // Get the format reader yielded by the probe operation.
                let mut reader = probed.format;

                let track = reader.default_track().unwrap().clone();

                let mut decoder = symphonia::default::get_codecs()
                    .make(&track.codec_params, &Default::default())
                    .unwrap();

                let mut samples: Vec<f64> = Vec::new();
                let sample_rate = track.codec_params.sample_rate.unwrap();

                loop {
                    let packet = match reader.next_packet() {
                        Ok(packet) => packet,
                        Err(e) => {
                            tracing::info!("Bad decode after {} samples: {e:?}", samples.len());
                            break;
                        }
                    };

                    while !reader.metadata().is_latest() {
                        reader.metadata().pop();
                    }

                    if packet.track_id() != track.id {
                        continue;
                    }

                    match decoder.decode(&packet) {
                        Ok(decoded) => {
                            let spec = *decoded.spec();

                            // Get the capacity of the decoded buffer. Note: This is capacity, not length!
                            let duration = decoded.capacity() as u64;
                            let num_channels = spec.channels.count();

                            let mut buf = SampleBuffer::<f64>::new(duration, spec);
                            buf.copy_interleaved_ref(decoded);

                            samples.extend(
                                buf.samples()
                                    .chunks(num_channels)
                                    .map(|s| s.iter().sum::<f64>() / num_channels as f64),
                            )
                        }
                        Err(symphonia_core::errors::Error::DecodeError(e)) => {
                            panic!("Bad decode: {e:?}")
                        }
                        Err(_) => break,
                    }
                }

                *self.taps.lock().unwrap() = if sample_rate != 48_000 {
                    let sinc = Sinc::new(dasp_ring_buffer::Fixed::from([0.0; 16]));

                    tracing::info!("Resampling taps from {sample_rate}Hz to 48_000Hz");

                    let mut taps = dasp_signal::from_iter(samples)
                        .from_hz_to_hz(sinc, sample_rate as f64, 48_000.0)
                        .until_exhausted()
                        .collect::<Vec<f64>>();

                    taps.reverse();

                    taps
                } else {
                    let mut taps = samples;
                    taps.reverse();

                    taps
                };

                *file_name = Some(path.to_string_lossy().to_string());
            }
        }
    }
}

impl SimpleNode for Fir {
    fn process(&self, inputs: crate::node::ProcessInput, mut outputs: crate::node::ProcessOutput) {
        let input = inputs.get("in").unwrap();
        let output = outputs.get("out").unwrap();

        let taps = self.taps.lock().unwrap();
        let mut state = self.state.lock().unwrap();

        let divisor = match self.mode.load(atomig::Ordering::Relaxed) {
            Mode::Average => 1.0 / taps.len() as f32,
            Mode::Balanced => 1.0,
        };

        for (in_, out) in zip(input.iter(), output.iter_mut()) {
            state.push_back(*in_ as f64);

            if state.len() > taps.len() {
                state.pop_front();
            }

            // this needs to be split because the auto-vectoriser craps out with VecDeque's
            // iterator
            let (a, b) = state.as_slices();
            let n_a = a.len();

            let a = zip(a.iter(), taps.iter())
                .map(|(x, c)| *x * c)
                .sum::<f64>() as f32;

            let b = if n_a < taps.len() {
                zip(b.iter(), taps[n_a..].iter())
                    .map(|(x, c)| *x * c)
                    .sum::<f64>() as f32
            } else {
                0.0
            };

            let val = a + b;

            // let val = zip(state.iter(), taps.iter())
            //     .map(|(x, c)| *x as f64 * c)
            //     .sum::<f64>() as f32;
            //
            *out = val * divisor;
        }
    }
}
