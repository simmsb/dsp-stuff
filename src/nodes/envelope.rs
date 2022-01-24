use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;
use collect_slice::CollectSlice;
use dasp_envelope::{detect::Peak, Detector};
use dasp_peak::FullWave;

pub struct Envelope {
    id: NodeId,
    inputs: PortStorage,
    outputs: PortStorage,
    detector: Arc<Mutex<dasp_envelope::Detector<f32, Peak<FullWave>>>>,
    attack: Atomic<f32>,
    release: Atomic<f32>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct EnvelopeConfig {
    id: NodeId,
    attack: f32,
    release: f32,
    inputs: HashMap<String, PortId>,
    outputs: HashMap<String, PortId>,
}

impl Node for Envelope {
    fn title(&self) -> &'static str {
        "Envelope"
    }

    fn cfg_name(&self) -> &'static str {
        "envelope"
    }

    fn description(&self) -> &'static str {
        "Envelope detection"
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn save(&self) -> serde_json::Value {
        let cfg = EnvelopeConfig {
            id: self.id,
            attack: self.attack.load(std::sync::atomic::Ordering::Relaxed),
            release: self.release.load(std::sync::atomic::Ordering::Relaxed),
            inputs: self.inputs.all().as_ref().clone(),
            outputs: self.outputs.all().as_ref().clone(),
        };

        serde_json::to_value(cfg).unwrap()
    }

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized,
    {
        let cfg: EnvelopeConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);

        this.attack
            .store(cfg.attack, std::sync::atomic::Ordering::Relaxed);
        this.release
            .store(cfg.release, std::sync::atomic::Ordering::Relaxed);
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
            ui.label("Attack");

            let mut s = self.attack.load(std::sync::atomic::Ordering::Relaxed);

            let r = ui.add(egui::Slider::new(&mut s, 0.0..=1000.0));

            if r.changed() {
                self.attack.store(s, std::sync::atomic::Ordering::Relaxed);
            }
        });

        ui.horizontal(|ui| {
            ui.label("Release");

            let mut s = self.release.load(std::sync::atomic::Ordering::Relaxed);

            let r = ui.add(egui::Slider::new(&mut s, 0.0..=1000.0));

            if r.changed() {
                self.release.store(s, std::sync::atomic::Ordering::Relaxed);
            }
        });
    }

    fn new(id: NodeId) -> Self {
        let this = Self {
            id,
            inputs: PortStorage::default(),
            outputs: PortStorage::default(),
            detector: Arc::new(Mutex::new(Detector::peak(0.0, 0.0))),
            attack: Atomic::new(0.0),
            release: Atomic::new(0.0),
        };

        this
    }
}

impl SimpleNode for Envelope {
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let attack = self.attack.load(std::sync::atomic::Ordering::Relaxed);
        let release = self.release.load(std::sync::atomic::Ordering::Relaxed);

        let input_id = self.inputs.get("in").unwrap();
        let output_id = self.outputs.get("out").unwrap();

        let input = inputs.get(&input_id).unwrap();
        let output = outputs.get_mut(&output_id).unwrap();

        let mut detector = self.detector.lock().unwrap();

        detector.set_attack_frames(attack);
        detector.set_release_frames(release);

        input
            .iter()
            .map(|v| detector.next(*v))
            .collect_slice(output);
    }
}
