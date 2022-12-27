use crate::{ids::NodeId, node::*};
use atomig::Atomic;
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
enum Port {
    A,
    B,
}

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    input = "a",
    input = "b",
    output = "out",
    title = "mux",
    cfg_name = "mux",
    description = "Toggle between two input signals"
)]
pub struct Mux {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(select, save, default = "Port::A")]
    in_port: Atomic<Port>,
}

impl SimpleNode for Mux {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: ProcessInput, mut outputs: ProcessOutput) {
        let port = self.in_port.load(atomig::Ordering::Relaxed);

        let input = match port {
            Port::A => inputs.get("a").unwrap(),
            Port::B => inputs.get("b").unwrap(),
        };

        let output = outputs.get("out").unwrap();
        output.copy_from_slice(input);
    }
}
