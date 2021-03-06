use std::sync::{Arc, Mutex};

use crate::{ids::NodeId, node::*};
use atomig::Atomic;

use dsp_stuff_gpl::muff::*;

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    input = "in",
    output = "out",
    title = "Muff",
    cfg_name = "muff",
    description = "Big Muff"
)]
pub struct Muff {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(slider(range = "0.0..=1.0"), save, default = "0.5")]
    toan: Atomic<f32>,
    #[dsp(slider(range = "0.0..=1.0"), save, default = "0.5")]
    level: Atomic<f32>,
    #[dsp(slider(range = "0.0..=1.0"), save, default = "0.5")]
    sustain: Atomic<f32>,

    state: Arc<Mutex<MuffState>>,
}

impl SimpleNode for Muff {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: ProcessInput, mut outputs: ProcessOutput) {
        let toan = self.toan.load(std::sync::atomic::Ordering::Relaxed);
        let level = self.level.load(std::sync::atomic::Ordering::Relaxed);
        let sustain = self.sustain.load(std::sync::atomic::Ordering::Relaxed);

        let input = inputs.get("in").unwrap();
        let output = outputs.get("out").unwrap();

        let mut muff_state = self.state.lock().unwrap();
        perform(input, output, toan, level, sustain, &mut muff_state);
    }
}
