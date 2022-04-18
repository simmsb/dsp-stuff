use std::collections::HashMap;

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;
use collect_slice::CollectSlice;

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(input = "a",
      input = "b",
      output = "out",
      title = "Mix",
      cfg_name = "mix",
      description = "Mix two signals together"
)]
pub struct Mix {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(slider(range = "0.0..=1.0"), label = "Ratio (a:b)", save, default="0.5")]
    ratio: Atomic<f32>,
}

impl SimpleNode for Mix {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let ratio = self.ratio.load(std::sync::atomic::Ordering::Relaxed);

        let input_a_id = self.inputs.get("a").unwrap();
        let input_b_id = self.inputs.get("b").unwrap();
        let output_id = self.outputs.get("out").unwrap();

        let a = inputs.get(&input_a_id).unwrap();
        let b = inputs.get(&input_b_id).unwrap();

        a.iter()
            .zip(b.iter())
            .map(|(a, b)| (b * ratio) + (a * (1.0 - ratio)))
            .collect_slice(outputs.get_mut(&output_id).unwrap());
    }
}
