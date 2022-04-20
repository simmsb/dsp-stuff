use std::collections::HashMap;

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;
use collect_slice::CollectSlice;

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    input = "in",
    output = "out",
    title = "Chebyshev",
    cfg_name = "chebyshev",
    description = "Chebyshev Distortion"
)]
pub struct Chebyshev {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(slider(range = "0.0..=50.0"), save, default = "0.0")]
    level_pos: Atomic<f32>,

    #[dsp(slider(range = "0.0..=50.0"), save, default = "0.0")]
    level_neg: Atomic<f32>,
}


fn do_chebyshev(sample: f32, level_pos: f32, level_neg: f32) -> f32 {
    if sample >= 0.0 {
        if level_pos < 0.001 {
            return sample;
        }

        (sample * level_pos).tanh() / level_pos.tanh()
    } else {
        if level_neg < 0.001 {
            return sample;
        }

        (sample * level_neg).tanh() / level_neg.tanh()
    }
}

fn chebyshev(input: &[f32], output: &mut [f32], level_pos: f32, level_neg: f32) {
    input
        .iter()
        .copied()
        .map(|x| do_chebyshev(x, level_pos, level_neg))
        .collect_slice(output);
}

impl SimpleNode for Chebyshev {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: ProcessInput, mut outputs: ProcessOutput) {
        let level_pos = self.level_pos.load(std::sync::atomic::Ordering::Relaxed);
        let level_neg = self.level_neg.load(std::sync::atomic::Ordering::Relaxed);

        let input = inputs.get("in").unwrap();
        let output = outputs.get("out").unwrap();

        chebyshev(input, output, level_pos, level_neg);
    }
}
