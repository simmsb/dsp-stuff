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
    fn process(&self, inputs: ProcessInput, mut outputs: ProcessOutput) {
        let ratio = self.ratio.load(std::sync::atomic::Ordering::Relaxed);

        let input_a = inputs.get("a").unwrap();
        let input_b = inputs.get("b").unwrap();
        let output = outputs.get("out").unwrap();

        input_a.iter()
            .zip(input_b.iter())
            .map(|(a, b)| (b * ratio) + (a * (1.0 - ratio)))
            .collect_slice(output);
    }
}
