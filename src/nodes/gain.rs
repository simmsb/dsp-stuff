use std::{collections::HashMap, sync::Arc};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomic_float::AtomicF32;
use collect_slice::CollectSlice;

pub struct Gain {
    id: NodeId,
    inputs: PortStorage,
    outputs: PortStorage,
    level: AtomicF32,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct GainConfig {
    id: NodeId,
    level: f32,
    inputs: HashMap<String, PortId>,
    outputs: HashMap<String, PortId>,
}

impl Node for Gain {
    fn title(&self) -> &'static str {
        "Gain"
    }

    fn cfg_name(&self) -> &'static str {
        "gain"
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn save(&self) -> serde_json::Value {
        let cfg = GainConfig {
            id: self.id,
            level: self.level.load(std::sync::atomic::Ordering::Relaxed),
            inputs: self.inputs.all().as_ref().clone(),
            outputs: self.outputs.all().as_ref().clone(),
        };

        serde_json::to_value(cfg).unwrap()
    }

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized {

        let cfg: GainConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);

        this.level.store(cfg.level, std::sync::atomic::Ordering::Relaxed);
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
        let r = ui.horizontal(|ui| {
            ui.label("Level");

            let mut s = self.level.load(std::sync::atomic::Ordering::Relaxed);

            let r = ui.add(egui::Slider::new(&mut s, 0.0..=5.0));

            if r.changed() {
                self.level.store(s, std::sync::atomic::Ordering::Relaxed);
            }

            r
        });

        r.response
    }

    fn new(id: NodeId) -> Self {
        let this = Self {
            id,
            inputs: PortStorage::default(),
            outputs: PortStorage::default(),
            level: AtomicF32::new(1.0),
        };

        this
    }
}

impl SimpleNode for Gain {
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let level = self.level.load(std::sync::atomic::Ordering::Relaxed);

        let input_id = self.inputs.get("in").unwrap();
        let output_id = self.outputs.get("out").unwrap();

        inputs
            .get(&input_id)
            .unwrap()
            .iter()
            .cloned()
            .map(|x| x * level)
            .collect_slice(outputs.get_mut(&output_id).unwrap());
    }
}
