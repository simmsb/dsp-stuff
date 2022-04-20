use atomig::Atomic;
use serde::{Deserialize, Serialize};

use crate::{ids::NodeId, node::*};

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
    Sine,
    Constant,
}

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    output = "out",
    title = "Signal Generator",
    cfg_name = "signal_gen",
    description = "Generate a signal with a given frequency and amplitude"
)]
pub struct SignalGen {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(slider(range = "-1.0..=1.0", as_input), save, default = "0.5")]
    amplitude: Atomic<f32>,
    #[dsp(
        slider(range = "0.1..=20000.0", logarithmic, suffix = " hz", as_input),
        save,
        default = "100.0"
    )]
    frequency: Atomic<f32>,

    clock: Atomic<f32>,

    #[dsp(select, default = "Mode::Sine")]
    mode: Atomic<Mode>,
}

impl SignalGen {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn do_sine(&self, output: &mut [f32], amplitude: &[f32], frequency: &[f32]) {
        let clock = self.clock.load(std::sync::atomic::Ordering::Relaxed);

        let sample_rate = 48000.0;
        let mut total = 0.0;

        for ((v, amplitude), frequency) in output.iter_mut().zip(amplitude).zip(frequency) {
            let step = frequency / sample_rate;
            total += step;
            *v = ((clock + total) * std::f32::consts::TAU).sin() * amplitude;
        }

        self.clock
            .store((clock + total) % 1.0, std::sync::atomic::Ordering::Relaxed);
    }

    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn do_const(&self, output: &mut [f32], amplitude: &[f32]) {
        output.copy_from_slice(amplitude);
    }
}

impl SimpleNode for SignalGen {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: ProcessInput, mut outputs: ProcessOutput) {
        let mut amplitude = [0.0; BUF_SIZE];
        self.amplitude_input(&inputs, &mut amplitude);
        let mut frequency = [0.0; BUF_SIZE];
        self.frequency_input(&inputs, &mut frequency);

        let output = outputs.get("out").unwrap();

        let mode = self.mode.load(std::sync::atomic::Ordering::Relaxed);

        match mode {
            Mode::Sine => self.do_sine(output, &amplitude, &frequency),
            Mode::Constant => self.do_const(output, &amplitude),
        }
    }
}
