use std::sync::Arc;

use crate::{
    ids::NodeId,
    node::{Node, Perform},
};

pub mod distort;
pub mod input;
pub mod output;
pub mod reverb;

pub static NODES: &[(&str, fn(NodeId) -> Arc<dyn Perform>)] = &[
    ("Input", |id| Arc::new(input::Input::new(id))),
    ("Output", |id| Arc::new(output::Output::new(id))),
    ("Distort", |id| Arc::new(distort::Distort::new(id))),
    ("Reverb", |id| Arc::new(reverb::Reverb::new(id))),
];
