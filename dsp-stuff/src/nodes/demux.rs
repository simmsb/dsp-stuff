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
    input = "in",
    output = "a",
    output = "b",
    title = "demux",
    cfg_name = "demux",
    description = "Toggle between two output signals"
)]
pub struct Demux {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(select, save, default = "Port::A")]
    out_port: Atomic<Port>,
}

impl SimpleNode for Demux {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: ProcessInput, mut outputs: ProcessOutput) {
        let port = self.out_port.load(atomig::Ordering::Relaxed);

        let input = inputs.get("in").unwrap();

        match port {
            Port::A => {
                outputs.get("a").unwrap().copy_from_slice(input);
            }
            Port::B => {
                outputs.get("b").unwrap().copy_from_slice(input);
            }
        }
    }
}
