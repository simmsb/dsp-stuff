use std::{collections::HashMap, sync::Arc};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;
use collect_slice::CollectSlice;

pub struct Mix {
    id: NodeId,
    inputs: PortStorage,
    outputs: PortStorage,
    ratio: Atomic<f32>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct MixConfig {
    id: NodeId,
    ratio: f32,
    inputs: HashMap<String, PortId>,
    outputs: HashMap<String, PortId>,
}

impl Node for Mix {
    fn title(&self) -> &'static str {
        "Mix"
    }

    fn cfg_name(&self) -> &'static str {
        "mix"
    }

    fn description(&self) -> &'static str {
        "Mix two signals together"
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn save(&self) -> serde_json::Value {
        let cfg = MixConfig {
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
        let cfg: MixConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);

        this.ratio
            .store(cfg.ratio, std::sync::atomic::Ordering::Relaxed);
        this.inputs = PortStorage::new(cfg.inputs);
        this.outputs = PortStorage::new(cfg.outputs);

        this
    }

    fn inputs(&self) -> Arc<HashMap<String, PortId>> {
        self.inputs.ensure_name("a");
        self.inputs.ensure_name("b");
        self.inputs.all()
    }

    fn outputs(&self) -> Arc<HashMap<String, PortId>> {
        self.outputs.ensure_name("out");
        self.outputs.all()
    }

    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn render(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Ratio (a:b)");

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
        };

        this
    }
}

impl SimpleNode for Mix {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let ratio = self.ratio.load(std::sync::atomic::Ordering::Relaxed);

        let input_a_id = self.inputs.get("a").unwrap();
        let input_b_id = self.inputs.get("b").unwrap();
        let output_id = self.outputs.get("out").unwrap();

        let a = inputs.get(&input_a_id).unwrap();
        let b = inputs.get(&input_b_id).unwrap();

        a.iter()
            .zip(b.iter())
            .map(|(a, b)| (b * ratio) + (a * (1.0 - ratio)))
            .collect_slice(outputs.get_mut(&output_id).unwrap());
    }
}
