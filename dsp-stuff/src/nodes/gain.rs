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
    title = "Gain",
    cfg_name = "gain",
    description = "Adjust gain of a signal"
)]
pub struct Gain {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(slider(range = "0.0..=10.0"), save, default="1.0")]
    level: Atomic<f32>,
}

impl SimpleNode for Gain {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let level = self.level.load(std::sync::atomic::Ordering::Relaxed);

        let input_id = self.inputs.get("in").unwrap();
        let output_id = self.outputs.get("out").unwrap();

        inputs
            .get(&input_id)
            .unwrap()
            .iter()
            .copied()
            .map(|x| x * level)
            .collect_slice(outputs.get_mut(&output_id).unwrap());
    }
}