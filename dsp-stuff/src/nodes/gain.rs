use crate::{ids::NodeId, node::*};
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

    #[dsp(slider(range = "0.0..=10.0", as_input), save, default = "1.0")]
    level: Atomic<f32>,
}

impl SimpleNode for Gain {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: ProcessInput, mut outputs: ProcessOutput) {
        let mut level = [0.0; BUF_SIZE];
        self.level_input(&inputs, &mut level);
        let input = inputs.get("in").unwrap();
        let output = outputs.get("out").unwrap();

        input
            .iter()
            .zip(level)
            .map(|(x, level)| x * level)
            .collect_slice(output);
    }
}
