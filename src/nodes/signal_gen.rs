use std::{collections::HashMap, sync::Arc};

use atomig::Atomic;

use crate::{
    ids::{NodeId, PortId},
    node::*,
};

pub struct SignalGen {
    id: NodeId,
    inputs: Arc<HashMap<String, PortId>>,
    outputs: PortStorage,

    amplitude: Atomic<f32>,
    frequency: Atomic<f32>,

    clock: Atomic<f32>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SignalGenConfig {
    id: NodeId,
    outputs: HashMap<String, PortId>,

    amplitude: f32,
    frequency: f32,
}

impl Node for SignalGen {
    fn title(&self) -> &'static str {
        "SignalGen"
    }

    fn cfg_name(&self) -> &'static str {
        "signal_gen"
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn save(&self) -> serde_json::Value {
        let cfg = SignalGenConfig {
            id: self.id,
            outputs: self.outputs.all().as_ref().clone(),
            amplitude: self.amplitude.load(std::sync::atomic::Ordering::Relaxed),
            frequency: self.frequency.load(std::sync::atomic::Ordering::Relaxed),
        };

        serde_json::to_value(cfg).unwrap()
    }

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized,
    {
        let cfg: SignalGenConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);

        this.amplitude
            .store(cfg.amplitude, std::sync::atomic::Ordering::Relaxed);
        this.frequency
            .store(cfg.frequency, std::sync::atomic::Ordering::Relaxed);
        this.outputs = PortStorage::new(cfg.outputs);

        this
    }

    fn inputs(&self) -> Arc<HashMap<String, PortId>> {
        Arc::clone(&self.inputs)
    }

    fn outputs(&self) -> Arc<HashMap<String, PortId>> {
        self.outputs.ensure_name("out");
        self.outputs.all()
    }

    fn render(&self, ui: &mut egui::Ui) -> egui::Response {
        let _r = ui.horizontal(|ui| {
            ui.label("Amplitude");

            let mut s = self.amplitude.load(std::sync::atomic::Ordering::Relaxed);

            let r = ui.add(egui::Slider::new(&mut s, 0.0..=5.0));

            if r.changed() {
                self.amplitude
                    .store(s, std::sync::atomic::Ordering::Relaxed);
            }

            r
        });

        let r = ui.horizontal(|ui| {
            ui.label("Frequency");

            let mut s = self.frequency.load(std::sync::atomic::Ordering::Relaxed);

            let r = ui.add(
                egui::Slider::new(&mut s, 20.0..=20000.0)
                    .suffix(" hz")
                    .logarithmic(true),
            );

            if r.changed() {
                self.frequency
                    .store(s, std::sync::atomic::Ordering::Relaxed);
            }

            r
        });

        r.response
    }

    fn new(id: NodeId) -> Self {
        let this = Self {
            id,
            inputs: Default::default(),
            outputs: PortStorage::default(),
            amplitude: Atomic::new(0.5),
            frequency: Atomic::new(20.0),
            clock: Atomic::new(0.0),
        };

        this
    }
}

impl SimpleNode for SignalGen {
    fn process(
        &self,
        _inputs: &HashMap<PortId, &[f32]>,
        outputs: &mut HashMap<PortId, &mut [f32]>,
    ) {
        let output_id = self.outputs.get("out").unwrap();
        let output = outputs.get_mut(&output_id).unwrap();

        let amplitude = self.amplitude.load(std::sync::atomic::Ordering::Relaxed);
        let frequency = self.frequency.load(std::sync::atomic::Ordering::Relaxed);
        let clock = self.clock.load(std::sync::atomic::Ordering::Relaxed);

        let sample_rate = 48000.0;
        self.clock.store(
            (clock + output.len() as f32) % sample_rate,
            std::sync::atomic::Ordering::Relaxed,
        );

        let steps_per_sample = std::f32::consts::TAU * frequency / sample_rate;

        for (idx, v) in output.iter_mut().enumerate() {
            *v = (steps_per_sample * (clock + idx as f32)).sin() * amplitude;
        }
    }
}
