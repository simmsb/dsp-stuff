use std::sync::Arc;

use crate::{
    ids::NodeId,
    node::{Node, NodeStatic, Perform},
};

use self::{
    add::Add, biquad::BiQuad, chebyshev::Chebyshev, demux::Demux, distort::Distort,
    envelope::Envelope, fir::FIR, gain::Gain, high_pass::HighPass, input::Input, low_pass::LowPass,
    mix::Mix, muff::Muff, mux::Mux, output::Output, overdrive::Overdrive, reverb::Reverb,
    signal_gen::SignalGen, spectrogram::Spectrogram, wave_view::WaveView,
};

pub mod add;
pub mod biquad;
pub mod chebyshev;
pub mod demux;
pub mod distort;
pub mod envelope;
pub mod fir;
pub mod gain;
pub mod high_pass;
pub mod input;
pub mod low_pass;
pub mod mix;
#[cfg(feature = "gpl_effects")]
pub mod muff;
pub mod mux;
pub mod output;
pub mod overdrive;
pub mod reverb;
pub mod signal_gen;
pub mod spectrogram;
pub mod wave_view;

#[enum_dispatch::enum_dispatch(Perform)]
#[enum_dispatch::enum_dispatch(Node)]
pub enum Nodes {
    Input,
    Output,
    Gain,
    Mix,
    Mux,
    Demux,
    Add,
    Distort,
    Overdrive,
    BiQuad,
    #[cfg(feature = "gpl_effects")]
    Muff,
    Chebyshev,
    Reverb,
    WaveView,
    Spectrogram,
    SignalGen,
    LowPass,
    HighPass,
    Envelope,
    FIR,
}

pub static NODES: &[(&str, fn(NodeId) -> Arc<Nodes>)] = &[
    ("Input", |id| Arc::new(Nodes::from(Input::new(id)))),
    ("Output", |id| Arc::new(Nodes::from(Output::new(id)))),
    ("Gain", |id| Arc::new(Nodes::from(Gain::new(id)))),
    ("Mix", |id| Arc::new(Nodes::from(Mix::new(id)))),
    ("Mux", |id| Arc::new(Nodes::from(Mux::new(id)))),
    ("Demux", |id| Arc::new(Nodes::from(Demux::new(id)))),
    ("Add", |id| Arc::new(Nodes::from(Add::new(id)))),
    ("Distort", |id| Arc::new(Nodes::from(Distort::new(id)))),
    ("Overdrive", |id| Arc::new(Nodes::from(Overdrive::new(id)))),
    ("Biquad", |id| Arc::new(Nodes::from(BiQuad::new(id)))),
    #[cfg(feature = "gpl_effects")]
    ("Muff", |id| Arc::new(Nodes::from(Muff::new(id)))),
    ("Chebyshev", |id| Arc::new(Nodes::from(Chebyshev::new(id)))),
    ("Reverb", |id| Arc::new(Nodes::from(Reverb::new(id)))),
    ("Wave view", |id| Arc::new(Nodes::from(WaveView::new(id)))),
    ("Spectrogram", |id| {
        Arc::new(Nodes::from(Spectrogram::new(id)))
    }),
    ("Signal gen", |id| Arc::new(Nodes::from(SignalGen::new(id)))),
    ("Low pass", |id| Arc::new(Nodes::from(LowPass::new(id)))),
    ("High pass", |id| Arc::new(Nodes::from(HighPass::new(id)))),
    ("Envelope", |id| Arc::new(Nodes::from(Envelope::new(id)))),
    ("FIR", |id| Arc::new(Nodes::from(FIR::new(id)))),
];

pub static RESTORE: &[(&str, fn(serde_json::Value) -> Arc<Nodes>)] = &[
    ("input", |v| Arc::new(Nodes::from(Input::restore(v)))),
    ("output", |v| Arc::new(Nodes::from(Output::restore(v)))),
    ("gain", |v| Arc::new(Nodes::from(Gain::restore(v)))),
    ("mix", |v| Arc::new(Nodes::from(Mix::restore(v)))),
    ("mux", |v| Arc::new(Nodes::from(Mux::restore(v)))),
    ("demux", |v| Arc::new(Nodes::from(Demux::restore(v)))),
    ("add", |v| Arc::new(Nodes::from(Add::restore(v)))),
    ("distort", |v| Arc::new(Nodes::from(Distort::restore(v)))),
    ("overdrive", |v| {
        Arc::new(Nodes::from(Overdrive::restore(v)))
    }),
    ("biquad", |v| Arc::new(Nodes::from(BiQuad::restore(v)))),
    #[cfg(feature = "gpl_effects")]
    ("muff", |v| Arc::new(Nodes::from(Muff::restore(v)))),
    ("chebyshev", |v| {
        Arc::new(Nodes::from(Chebyshev::restore(v)))
    }),
    ("reverb", |v| Arc::new(Nodes::from(Reverb::restore(v)))),
    ("wave_view", |v| Arc::new(Nodes::from(WaveView::restore(v)))),
    ("spectrogram", |v| {
        Arc::new(Nodes::from(Spectrogram::restore(v)))
    }),
    ("signal_gen", |v| {
        Arc::new(Nodes::from(SignalGen::restore(v)))
    }),
    ("low_pass", |v| Arc::new(Nodes::from(LowPass::restore(v)))),
    ("high_pass", |v| Arc::new(Nodes::from(HighPass::restore(v)))),
    ("envelope", |v| Arc::new(Nodes::from(Envelope::restore(v)))),
    ("fir", |v| Arc::new(Nodes::from(FIR::restore(v)))),
];
