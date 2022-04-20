use std::collections::HashMap;

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    input = "in",
    output = "out",
    title = "Low Pass",
    cfg_name = "high_pass",
    description = "Attenuates higher frequencies"
)]
pub struct LowPass {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(slider(range = "0.0..=1.0"), save, default = "0.5")]
    ratio: Atomic<f32>,

    z: Atomic<f32>,
}

impl SimpleNode for LowPass {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: ProcessInput, mut outputs: ProcessOutput) {
        let ratio = self.ratio.load(std::sync::atomic::Ordering::Relaxed);

        let input = inputs.get("in").unwrap();
        let output = outputs.get("out").unwrap();

        let mut z = self.z.load(std::sync::atomic::Ordering::Relaxed);

        for (in_, out) in input.iter().zip(output.iter_mut()) {
            *out = *in_ * (1.0 - ratio) + ratio * z;
            z = *out;
        }

        self.z.store(z, std::sync::atomic::Ordering::Relaxed);
    }
}
