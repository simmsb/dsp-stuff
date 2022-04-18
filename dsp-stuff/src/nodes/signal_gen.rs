use std::collections::HashMap;

use atomig::Atomic;
use serde::{Deserialize, Serialize};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};

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
    input = "in",
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

    #[dsp(slider(range = "-1.0..=1.0"), save, default = "0.5")]
    amplitude: Atomic<f32>,
    #[dsp(slider(range = "0.1..=20000.0", logarithmic, suffix = " hz"), save, default = "100.0")]
    frequency: Atomic<f32>,

    clock: Atomic<f32>,

    #[dsp(select, default = "Mode::Sine")]
    mode: Atomic<Mode>,
}

impl SignalGen {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn do_sine(&self, output: &mut [f32]) {
        let clock = self.clock.load(std::sync::atomic::Ordering::Relaxed);

        let amplitude = self.amplitude.load(std::sync::atomic::Ordering::Relaxed);
        let frequency = self.frequency.load(std::sync::atomic::Ordering::Relaxed);

        let sample_rate = 48000.0;
        let steps_per_sample = frequency / sample_rate;

        self.clock.store(
            (clock + output.len() as f32 * steps_per_sample) % 1.0,
            std::sync::atomic::Ordering::Relaxed,
        );

        for (idx, v) in output.iter_mut().enumerate() {
            *v =
                ((clock + steps_per_sample * idx as f32) * std::f32::consts::TAU).sin() * amplitude;
        }
    }

    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn do_const(&self, output: &mut [f32]) {
        let amplitude = self.amplitude.load(std::sync::atomic::Ordering::Relaxed);

        output.fill(amplitude);
    }
}

impl SimpleNode for SignalGen {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(
        &self,
        _inputs: &HashMap<PortId, &[f32]>,
        outputs: &mut HashMap<PortId, &mut [f32]>,
    ) {
        let output_id = self.outputs.get("out").unwrap();
        let output = outputs.get_mut(&output_id).unwrap();

        let mode = self.mode.load(std::sync::atomic::Ordering::Relaxed);

        match mode {
            Mode::Sine => self.do_sine(output),
            Mode::Constant => self.do_const(output),
        }
    }
}
