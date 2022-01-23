use std::sync::Arc;

use crate::{
    ids::NodeId,
    node::{Node, Perform},
};

pub mod chebyshev;
pub mod distort;
pub mod gain;
pub mod high_pass;
pub mod input;
pub mod low_pass;
pub mod mix;
pub mod output;
pub mod reverb;
pub mod signal_gen;
pub mod spectrogram;
pub mod wave_view;

pub static NODES: &[(&str, fn(NodeId) -> Arc<dyn Perform>)] = &[
    ("Input", |id| Arc::new(input::Input::new(id))),
    ("Output", |id| Arc::new(output::Output::new(id))),
    ("Gain", |id| Arc::new(gain::Gain::new(id))),
    ("Mix", |id| Arc::new(mix::Mix::new(id))),
    ("Distort", |id| Arc::new(distort::Distort::new(id))),
    ("Chebyshev", |id| Arc::new(chebyshev::Chebyshev::new(id))),
    ("Reverb", |id| Arc::new(reverb::Reverb::new(id))),
    ("Wave view", |id| Arc::new(wave_view::WaveView::new(id))),
    ("Spectrogram", |id| {
        Arc::new(spectrogram::Spectrogram::new(id))
    }),
    ("Signal gen", |id| Arc::new(signal_gen::SignalGen::new(id))),
    ("Low pass", |id| Arc::new(low_pass::LowPass::new(id))),
    ("High pass", |id| Arc::new(high_pass::HighPass::new(id))),
];

pub static RESTORE: &[(&str, fn(serde_json::Value) -> Arc<dyn Perform>)] = &[
    ("input", |v| Arc::new(input::Input::restore(v))),
    ("output", |v| Arc::new(output::Output::restore(v))),
    ("gain", |v| Arc::new(gain::Gain::restore(v))),
    ("mix", |v| Arc::new(mix::Mix::restore(v))),
    ("distort", |v| Arc::new(distort::Distort::restore(v))),
    ("chebyshev", |v| Arc::new(chebyshev::Chebyshev::restore(v))),
    ("reverb", |v| Arc::new(reverb::Reverb::restore(v))),
    ("wave_view", |v| Arc::new(wave_view::WaveView::restore(v))),
    ("spectrogram", |v| {
        Arc::new(spectrogram::Spectrogram::restore(v))
    }),
    ("signal_gen", |v| {
        Arc::new(signal_gen::SignalGen::restore(v))
    }),
    ("low_pass", |v| Arc::new(low_pass::LowPass::restore(v))),
    ("high_pass", |v| Arc::new(high_pass::HighPass::restore(v))),
];
