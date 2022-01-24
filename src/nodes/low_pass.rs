use std::{collections::HashMap, sync::Arc};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;

pub struct LowPass {
    id: NodeId,
    inputs: PortStorage,
    outputs: PortStorage,
    ratio: Atomic<f32>,
    z: Atomic<f32>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct LowPassConfig {
    id: NodeId,
    ratio: f32,
    inputs: HashMap<String, PortId>,
    outputs: HashMap<String, PortId>,
}

impl Node for LowPass {
    fn title(&self) -> &'static str {
        "Low Pass"
    }

    fn cfg_name(&self) -> &'static str {
        "low_pass"
    }

    fn description(&self) -> &'static str {
        "Attenuates higher frequencies"
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn save(&self) -> serde_json::Value {
        let cfg = LowPassConfig {
            id: self.id,
            ratio: self.ratio.load(std::sync::atomic::Ordering::Relaxed),
            inputs: self.inputs.all().as_ref().clone(),
            outputs: self.outputs.all().as_ref().clone(),
        };

        serde_json::to_value(cfg).unwrap()
    }

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized,
    {
        let cfg: LowPassConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);

        this.ratio
            .store(cfg.ratio, std::sync::atomic::Ordering::Relaxed);
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

            let mut s = self.ratio.load(std::sync::atomic::Ordering::Relaxed);

            let r = ui.add(egui::Slider::new(&mut s, 0.0..=1.0));

            if r.changed() {
                self.ratio.store(s, std::sync::atomic::Ordering::Relaxed);
            }

            r
        });
    }

    fn new(id: NodeId) -> Self {
        let this = Self {
            id,
            inputs: PortStorage::default(),
            outputs: PortStorage::default(),
            ratio: Atomic::new(0.5),
            z: Atomic::new(0.0),
        };

        this
    }
}

impl SimpleNode for LowPass {
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let ratio = self.ratio.load(std::sync::atomic::Ordering::Relaxed);

        let input_id = self.inputs.get("in").unwrap();
        let output_id = self.outputs.get("out").unwrap();

        let input = inputs.get(&input_id).unwrap();
        let output = outputs.get_mut(&output_id).unwrap();

        let mut z = self.z.load(std::sync::atomic::Ordering::Relaxed);

        for (in_, out) in input.iter().zip(output.iter_mut()) {
            *out = *in_ * (1.0 - ratio) + ratio * z;
            z = *out;
        }

        self.z.store(z, std::sync::atomic::Ordering::Relaxed);
    }
}
