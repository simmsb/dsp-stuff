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
    title = "High Pass",
    cfg_name = "high_pass",
    description = "Attenuates lower frequencies"
)]
pub struct HighPass {
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

impl SimpleNode for HighPass {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let ratio = self.ratio.load(std::sync::atomic::Ordering::Relaxed);

        let input_id = self.inputs.get("in").unwrap();
        let output_id = self.outputs.get("out").unwrap();

        let input = inputs.get(&input_id).unwrap();
        let output = outputs.get_mut(&output_id).unwrap();

        let mut z = self.z.load(std::sync::atomic::Ordering::Relaxed);

        for (in_, out) in input.iter().zip(output.iter_mut()) {
            z = *in_ * (1.0 - ratio) + ratio * z;
            *out = *in_ - z;
        }

        self.z.store(z, std::sync::atomic::Ordering::Relaxed);
    }
}
