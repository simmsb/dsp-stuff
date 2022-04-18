use std::collections::HashMap;

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;
use collect_slice::CollectSlice;
use serde::{Deserialize, Serialize};

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
    SoftClip,
    Tanh,
    RecipSoftClip,
}

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    input = "in",
    output = "out",
    title = "Distort",
    cfg_name = "distort",
    description = "Distortion effects"
)]
pub struct Distort {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(slider(range = "0.0..=10.0"), save)]
    level: Atomic<f32>,

    #[dsp(select, save, default = "Mode::SoftClip")]
    mode: Atomic<Mode>,
}

fn do_soft_clip(sample: f32, level: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    let sample = sample * level;
    let sample = if sample > 1.0 {
        2.0 / 3.0
    } else if (-1.0..=1.0).contains(&sample) {
        sample - (sample.powi(3) / 3.0)
    } else {
        -2.0 / 3.0
    };

    sample / level
}

fn soft_clip(input: &[f32], output: &mut [f32], level: f32) {
    input
        .iter()
        .copied()
        .map(|x| do_soft_clip(x, level))
        .collect_slice(output);
}

fn do_recip_soft_clip(sample: f32, level: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    sample.signum() * (1.0 - 1.0 / (sample.abs() * level + 1.0))
}

fn recip_soft_clip(input: &[f32], output: &mut [f32], level: f32) {
    input
        .iter()
        .copied()
        .map(|x| do_recip_soft_clip(x, level))
        .collect_slice(output);
}

fn do_tanh(sample: f32, level: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    (sample * level).tanh()
}

fn tanh(input: &[f32], output: &mut [f32], level: f32) {
    input
        .iter()
        .copied()
        .map(|x| do_tanh(x, level))
        .collect_slice(output);
}

impl SimpleNode for Distort {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let level = self.level.load(std::sync::atomic::Ordering::Relaxed);

        let input_id = self.inputs.get("in").unwrap();
        let output_id = self.outputs.get("out").unwrap();

        let input = inputs.get(&input_id).unwrap();
        let output = outputs.get_mut(&output_id).unwrap();

        let mode = self.mode.load(std::sync::atomic::Ordering::Relaxed);

        match mode {
            Mode::SoftClip => soft_clip(input, output, level),
            Mode::Tanh => tanh(input, output, level),
            Mode::RecipSoftClip => recip_soft_clip(input, output, level),
        }
    }
}
