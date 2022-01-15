use std::{collections::HashMap, sync::Arc};

use crate::{ids::{PortId, NodeId}, node::*};
use atomic_float::AtomicF32;
use collect_slice::CollectSlice;

pub struct Distort {
    _id: NodeId,
    inputs: PortStorage,
    outputs: PortStorage,
    level: AtomicF32,
}

impl Node for Distort {
    fn title(&self) -> &'static str {
        "Distort"
    }

    fn inputs(&self) -> Arc<HashMap<&'static str, PortId>> {
        self.inputs.get_or_create("in");
        self.inputs.all()
    }

    fn outputs(&self) -> Arc<HashMap<&'static str, PortId>> {
        self.outputs.get_or_create("out");
        self.outputs.all()
    }

    fn render(&self, ui: &mut egui::Ui) -> egui::Response {
        let mut s = self.level.load(std::sync::atomic::Ordering::Relaxed);

        let r = ui.add(egui::Slider::new(&mut s, 0.0..=100.0));

        if r.changed() {
            self.level.store(s, std::sync::atomic::Ordering::Relaxed);
        }

        r
    }

    fn new(id: NodeId) -> Self {
        let this = Self {
            _id: id,
            inputs: PortStorage::default(),
            outputs: PortStorage::default(),
            level: AtomicF32::new(0.0),
        };

        this
    }
}

fn do_distort(sample: f32, level: f32) -> f32 {
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

impl SimpleNode for Distort {
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let level = self.level.load(std::sync::atomic::Ordering::Relaxed);

        let input_id = self.inputs.get("in").unwrap();
        let output_id = self.outputs.get("out").unwrap();

        tracing::debug!(level, "Doing a distort");

        inputs
            .get(&input_id)
            .unwrap()
            .iter()
            .cloned()
            .map(|x| do_distort(x, level))
            .collect_slice(outputs.get_mut(&output_id).unwrap());
    }
}
