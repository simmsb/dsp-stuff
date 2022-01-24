use std::{collections::HashMap, sync::Arc};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use collect_slice::CollectSlice;

pub struct Multiply {
    id: NodeId,
    inputs: PortStorage,
    outputs: PortStorage,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct MultiplyConfig {
    id: NodeId,
    inputs: HashMap<String, PortId>,
    outputs: HashMap<String, PortId>,
}

impl Node for Multiply {
    fn title(&self) -> &'static str {
        "Multiply"
    }

    fn cfg_name(&self) -> &'static str {
        "multiply"
    }

    fn description(&self) -> &'static str {
        "Multiply two signals together"
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn save(&self) -> serde_json::Value {
        let cfg = MultiplyConfig {
            id: self.id,
            inputs: self.inputs.all().as_ref().clone(),
            outputs: self.outputs.all().as_ref().clone(),
        };

        serde_json::to_value(cfg).unwrap()
    }

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized,
    {
        let cfg: MultiplyConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);

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

    fn render(&self, _ui: &mut egui::Ui) {
    }

    fn new(id: NodeId) -> Self {
        let this = Self {
            id,
            inputs: PortStorage::default(),
            outputs: PortStorage::default(),
        };

        this
    }
}

impl SimpleNode for Multiply {
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let input_a_id = self.inputs.get("a").unwrap();
        let input_b_id = self.inputs.get("b").unwrap();
        let output_id = self.outputs.get("out").unwrap();

        let a = inputs.get(&input_a_id).unwrap();
        let b = inputs.get(&input_b_id).unwrap();

        a.iter()
            .zip(b.iter())
            .map(|(a, b)| a * b)
            .collect_slice(outputs.get_mut(&output_id).unwrap());
    }
}
