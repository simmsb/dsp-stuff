use std::{collections::HashMap, sync::Arc};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;
use collect_slice::CollectSlice;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

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
enum Mode {
    SoftClip,
    Tanh,
    RecipSoftClip,
}

pub struct Distort {
    id: NodeId,
    inputs: PortStorage,
    outputs: PortStorage,
    level: Atomic<f32>,
    mode: Atomic<Mode>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct DistortConfig {
    id: NodeId,
    level: f32,
    inputs: HashMap<String, PortId>,
    outputs: HashMap<String, PortId>,
    mode: Mode,
}

impl Node for Distort {
    fn title(&self) -> &'static str {
        "Distort"
    }

    fn cfg_name(&self) -> &'static str {
        "distort"
    }

    fn description(&self) -> &'static str {
        "Distortion effects"
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn save(&self) -> serde_json::Value {
        let cfg = DistortConfig {
            id: self.id,
            level: self.level.load(std::sync::atomic::Ordering::Relaxed),
            inputs: self.inputs.all().as_ref().clone(),
            outputs: self.outputs.all().as_ref().clone(),
            mode: self.mode.load(std::sync::atomic::Ordering::Relaxed),
        };

        serde_json::to_value(cfg).unwrap()
    }

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized,
    {
        let cfg: DistortConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);

        this.level
            .store(cfg.level, std::sync::atomic::Ordering::Relaxed);
        this.mode
            .store(cfg.mode, std::sync::atomic::Ordering::Relaxed);
        this.inputs = PortStorage::new(cfg.inputs);
        this.outputs = PortStorage::new(cfg.outputs);

        this
    }

    fn inputs(&self) -> Arc<HashMap<String, PortId>> {
        self.inputs.ensure_name("in");
        self.inputs.all()
    }

    fn outputs(&self) -> Arc<HashMap<String, PortId>> {
        self.outputs.ensure_name("out");
        self.outputs.all()
    }

    fn render(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Level");

            let mut s = self.level.load(std::sync::atomic::Ordering::Relaxed);

            let r = ui.add(egui::Slider::new(&mut s, 0.0..=50.0));

            if r.changed() {
                self.level.store(s, std::sync::atomic::Ordering::Relaxed);
            }
        });

        let current_mode = self.mode.load(std::sync::atomic::Ordering::Relaxed);
        let mut mode = current_mode;

        egui::ComboBox::from_id_source(("distort_mode", self.id))
            .with_label("Mode")
            .selected_text(<&'static str>::from(mode))
            .show_ui(ui, |ui| {
                for possible_mode in Mode::iter() {
                    ui.selectable_value(
                        &mut mode,
                        possible_mode,
                        <&'static str>::from(possible_mode),
                    );
                }
            });

        if mode != current_mode {
            self.mode.store(mode, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn new(id: NodeId) -> Self {
        let this = Self {
            id,
            inputs: PortStorage::default(),
            outputs: PortStorage::default(),
            level: Atomic::new(0.0),
            mode: Atomic::new(Mode::SoftClip),
        };

        this
    }
}

fn do_soft_clip(sample: f32, level: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    let sample = sample * level;
    let sample = if sample > 1.0 {
        2.0 / 3.0
    } else if (-1.0 <= sample) && (sample <= 1.0) {
        sample - (sample.powi(3) / 3.0)
    } else {
        -2.0 / 3.0
    };

    sample / level
}

fn soft_clip(input: &[f32], output: &mut [f32], level: f32) {
    input
        .iter()
        .copied()
        .map(|x| do_soft_clip(x, level))
        .collect_slice(output);
}

fn do_recip_soft_clip(sample: f32, level: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    let sample = sample.signum() * (1.0 - 1.0 / (sample.abs() * level + 1.0));

    sample
}

fn recip_soft_clip(input: &[f32], output: &mut [f32], level: f32) {
    input
        .iter()
        .copied()
        .map(|x| do_recip_soft_clip(x, level))
        .collect_slice(output);
}

fn do_tanh(sample: f32, level: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    (sample * level).tanh()
}

fn tanh(input: &[f32], output: &mut [f32], level: f32) {
    input
        .iter()
        .copied()
        .map(|x| do_tanh(x, level))
        .collect_slice(output);
}

impl SimpleNode for Distort {
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let level = self.level.load(std::sync::atomic::Ordering::Relaxed);

        let input_id = self.inputs.get("in").unwrap();
        let output_id = self.outputs.get("out").unwrap();

        let input = inputs.get(&input_id).unwrap();
        let output = outputs.get_mut(&output_id).unwrap();

        let mode = self.mode.load(std::sync::atomic::Ordering::Relaxed);

        match mode {
            Mode::SoftClip => soft_clip(input, output, level),
            Mode::Tanh => tanh(input, output, level),
            Mode::RecipSoftClip => recip_soft_clip(input, output, level),
        }
    }
}
