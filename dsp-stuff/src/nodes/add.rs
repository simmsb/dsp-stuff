use crate::{ids::NodeId, node::*};
use collect_slice::CollectSlice;

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    input = "a",
    input = "b",
    output = "out",
    title = "add",
    cfg_name = "add",
    description = "add two signals together"
)]
pub struct Add {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,
}

impl SimpleNode for Add {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: ProcessInput, mut outputs: ProcessOutput) {
        let input_a = inputs.get("a").unwrap();
        let input_b = inputs.get("b").unwrap();
        let output = outputs.get("out").unwrap();

        input_a
            .iter()
            .zip(input_b)
            .map(|(a, b)| a + b)
            .collect_slice(output);
    }
}
