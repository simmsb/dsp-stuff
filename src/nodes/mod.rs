use std::sync::Arc;

use crate::{
    ids::NodeId,
    node::{Node, Perform},
};

pub mod distort;
pub mod input;
pub mod output;
pub mod reverb;
pub mod wave_view;

pub static NODES: &[(&str, fn(NodeId) -> Arc<dyn Perform>)] = &[
    ("Input", |id| Arc::new(input::Input::new(id))),
    ("Output", |id| Arc::new(output::Output::new(id))),
    ("Distort", |id| Arc::new(distort::Distort::new(id))),
    ("Reverb", |id| Arc::new(reverb::Reverb::new(id))),
    ("Wave view", |id| Arc::new(wave_view::WaveView::new(id))),
];

pub static RESTORE: &[(&str, fn(serde_json::Value) -> Arc<dyn Perform>)] = &[
    ("input", |v| {
        Arc::new(input::Input::restore(v))
    }),
    ("output", |v| {
        Arc::new(output::Output::restore(v))
    }),
    ("distort", |v| {
        Arc::new(distort::Distort::restore(v))
    }),
    ("reverb", |v| {
        Arc::new(reverb::Reverb::restore(v))
    }),
    ("wave_view", |v| {
        Arc::new(wave_view::WaveView::restore(v))
    }),
];
