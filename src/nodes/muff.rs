use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;

use dsp_stuff_gpl::muff::*;

pub struct Muff {
    id: NodeId,
    inputs: PortStorage,
    outputs: PortStorage,
    toan: Atomic<f32>,
    level: Atomic<f32>,
    sustain: Atomic<f32>,
    state: Arc<Mutex<MuffState>>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct MuffConfig {
    id: NodeId,
    inputs: HashMap<String, PortId>,
    outputs: HashMap<String, PortId>,
    toan: f32,
    level: f32,
    sustain: f32,
}

impl Node for Muff {
    fn title(&self) -> &'static str {
        "Muff"
    }

    fn cfg_name(&self) -> &'static str {
        "muff"
    }

    fn description(&self) -> &'static str {
        "Adjust muff of a signal"
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn save(&self) -> serde_json::Value {
        let cfg = MuffConfig {
            id: self.id,
            inputs: self.inputs.all().as_ref().clone(),
            outputs: self.outputs.all().as_ref().clone(),
            toan: self.toan.load(std::sync::atomic::Ordering::Relaxed),
            level: self.level.load(std::sync::atomic::Ordering::Relaxed),
            sustain: self.sustain.load(std::sync::atomic::Ordering::Relaxed),
        };

        serde_json::to_value(cfg).unwrap()
    }

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized,
    {
        let cfg: MuffConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);

        this.toan
            .store(cfg.toan, std::sync::atomic::Ordering::Relaxed);
        this.level
            .store(cfg.level, std::sync::atomic::Ordering::Relaxed);
        this.sustain
            .store(cfg.sustain, std::sync::atomic::Ordering::Relaxed);
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
            ui.label("Toan");

            let mut s = self.toan.load(std::sync::atomic::Ordering::Relaxed);

            let r = ui.add(egui::Slider::new(&mut s, 0.0..=1.0));

            if r.changed() {
                self.toan.store(s, std::sync::atomic::Ordering::Relaxed);
            }

            r
        });

        ui.horizontal(|ui| {
            ui.label("Level");

            let mut s = self.level.load(std::sync::atomic::Ordering::Relaxed);

            let r = ui.add(egui::Slider::new(&mut s, 0.0..=1.0));

            if r.changed() {
                self.level.store(s, std::sync::atomic::Ordering::Relaxed);
            }

            r
        });

        ui.horizontal(|ui| {
            ui.label("Sustain");

            let mut s = self.sustain.load(std::sync::atomic::Ordering::Relaxed);

            let r = ui.add(egui::Slider::new(&mut s, 0.0..=1.0));

            if r.changed() {
                self.sustain.store(s, std::sync::atomic::Ordering::Relaxed);
            }

            r
        });
    }

    fn new(id: NodeId) -> Self {
        let this = Self {
            id,
            inputs: PortStorage::default(),
            outputs: PortStorage::default(),
            toan: Atomic::new(0.5),
            level: Atomic::new(0.5),
            sustain: Atomic::new(0.5),
            state: Arc::new(Mutex::new(MuffState::default())),
        };

        this
    }
}

impl SimpleNode for Muff {
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let toan = self.toan.load(std::sync::atomic::Ordering::Relaxed);
        let level = self.level.load(std::sync::atomic::Ordering::Relaxed);
        let sustain = self.sustain.load(std::sync::atomic::Ordering::Relaxed);

        let input_id = self.inputs.get("in").unwrap();
        let input = inputs.get(&input_id).unwrap();
        let output_id = self.outputs.get("out").unwrap();
        let output = outputs.get_mut(&output_id).unwrap();

        let mut muff_state = self.state.lock().unwrap();
        perform(input, output, toan, level, sustain, &mut muff_state);
    }
}
