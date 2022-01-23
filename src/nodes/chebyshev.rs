use std::{collections::HashMap, sync::Arc};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;
use collect_slice::CollectSlice;

pub struct Chebyshev {
    id: NodeId,
    inputs: PortStorage,
    outputs: PortStorage,
    level_pos: Atomic<f32>,
    level_neg: Atomic<f32>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct ChebyshevConfig {
    id: NodeId,
    level_pos: f32,
    level_neg: f32,
    inputs: HashMap<String, PortId>,
    outputs: HashMap<String, PortId>,
}

impl Node for Chebyshev {
    fn title(&self) -> &'static str {
        "Chebyshev"
    }

    fn cfg_name(&self) -> &'static str {
        "chebyshev"
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn save(&self) -> serde_json::Value {
        let cfg = ChebyshevConfig {
            id: self.id,
            level_pos: self.level_pos.load(std::sync::atomic::Ordering::Relaxed),
            level_neg: self.level_neg.load(std::sync::atomic::Ordering::Relaxed),
            inputs: self.inputs.all().as_ref().clone(),
            outputs: self.outputs.all().as_ref().clone(),
        };

        serde_json::to_value(cfg).unwrap()
    }

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized,
    {
        let cfg: ChebyshevConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);

        this.level_pos
            .store(cfg.level_pos, std::sync::atomic::Ordering::Relaxed);
        this.level_neg
            .store(cfg.level_neg, std::sync::atomic::Ordering::Relaxed);
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

    fn render(&self, ui: &mut egui::Ui) -> egui::Response {
        let _r = ui.horizontal(|ui| {
            ui.label("Level (pos)");

            let mut s = self.level_pos.load(std::sync::atomic::Ordering::Relaxed);

            let r = ui.add(egui::Slider::new(&mut s, 0.0..=50.0));

            if r.changed() {
                self.level_pos.store(s, std::sync::atomic::Ordering::Relaxed);
            }
        });

        let r = ui.horizontal(|ui| {
            ui.label("Level (neg)");

            let mut s = self.level_neg.load(std::sync::atomic::Ordering::Relaxed);

            let r = ui.add(egui::Slider::new(&mut s, 0.0..=50.0));

            if r.changed() {
                self.level_neg.store(s, std::sync::atomic::Ordering::Relaxed);
            }
        });

        r.response
    }

    fn new(id: NodeId) -> Self {
        let this = Self {
            id,
            inputs: PortStorage::default(),
            outputs: PortStorage::default(),
            level_pos: Atomic::new(0.0),
            level_neg: Atomic::new(0.0),
        };

        this
    }
}

fn do_tanh(sample: f32, level_pos: f32, level_neg: f32) -> f32 {
    if sample >= 0.0 {
        if level_pos < 0.001 {
            return sample;
        }

        (sample * level_pos).tanh() / level_pos.tanh()
    } else {
        if level_neg < 0.001 {
            return sample;
        }

        (sample * level_neg).tanh() / level_neg.tanh()
    }
}

fn tanh(input: &[f32], output: &mut [f32], level_pos: f32, level_neg: f32) {
    input
        .iter()
        .cloned()
        .map(|x| do_tanh(x, level_pos, level_neg))
        .collect_slice(output);
}

impl SimpleNode for Chebyshev {
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let level_pos = self.level_pos.load(std::sync::atomic::Ordering::Relaxed);
        let level_neg = self.level_neg.load(std::sync::atomic::Ordering::Relaxed);

        let input_id = self.inputs.get("in").unwrap();
        let output_id = self.outputs.get("out").unwrap();

        let input = inputs.get(&input_id).unwrap();
        let output = outputs.get_mut(&output_id).unwrap();

        tanh(input, output, level_pos, level_neg);
    }
}
