use std::{collections::HashMap, sync::Arc};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;
use collect_slice::CollectSlice;
use rivulet::{
    circular_buffer::{Sink, Source},
    splittable, SplittableView, View, ViewMut,
};
use std::sync::Mutex;

pub struct Reverb {
    id: NodeId,
    inputs: PortStorage,
    outputs: PortStorage,
    seconds: Atomic<f32>,
    decay: Atomic<f32>,
    buffer: Arc<Mutex<(splittable::View<Source<f32>>, Sink<f32>)>>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct ReverbConfig {
    id: NodeId,
    seconds: f32,
    decay: f32,
    inputs: HashMap<String, PortId>,
    outputs: HashMap<String, PortId>,
}

impl Reverb {
    fn set_seconds(&self, seconds: f32) {
        self.seconds
            .store(seconds, std::sync::atomic::Ordering::Relaxed);

        let num_samples = ((seconds * 48000.0) as usize).max(128);

        let (mut new_sink, new_source) = rivulet::circular_buffer::<f32>(num_samples);
        let new_source = new_source.into_view();

        let mut guard = self.buffer.lock().unwrap();

        let _ = new_sink.try_grant(num_samples);
        new_sink.view_mut().fill(0.0);
        let num_zeros = new_sink.view().len();
        new_sink.release(num_zeros);

        *guard = (new_source, new_sink);
    }
}

impl Node for Reverb {
    fn title(&self) -> &'static str {
        "Reverb"
    }

    fn cfg_name(&self) -> &'static str {
        "reverb"
    }

    fn description(&self) -> &'static str {
        "Repeat/ echo sounds with a given delay and decay factor"
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn inputs(&self) -> Arc<HashMap<String, PortId>> {
        self.inputs.ensure_name("in");
        self.inputs.all()
    }

    fn outputs(&self) -> Arc<HashMap<String, PortId>> {
        self.outputs.ensure_name("out");
        self.outputs.all()
    }

    fn save(&self) -> serde_json::Value {
        let cfg = ReverbConfig {
            id: self.id,
            seconds: self.seconds.load(std::sync::atomic::Ordering::Relaxed),
            decay: self.decay.load(std::sync::atomic::Ordering::Relaxed),
            inputs: self.inputs.all().as_ref().clone(),
            outputs: self.outputs.all().as_ref().clone(),
        };

        serde_json::to_value(cfg).unwrap()
    }

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized,
    {
        let cfg: ReverbConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);

        this.decay
            .store(cfg.decay, std::sync::atomic::Ordering::Relaxed);
        this.set_seconds(cfg.seconds);
        this.inputs = PortStorage::new(cfg.inputs);
        this.outputs = PortStorage::new(cfg.outputs);

        this
    }

    fn render(&self, ui: &mut egui::Ui) -> egui::Response {
        ui.horizontal(|ui| {
            ui.label("Delay (s)");

            let mut s = self.seconds.load(std::sync::atomic::Ordering::Relaxed);
            let r = ui.add(egui::Slider::new(&mut s, 0.0..=10.0));

            if r.changed() {
                self.set_seconds(s);
            }
        });

        let r = ui.horizontal(|ui| {
            ui.label("Decay");

            let mut s = self.decay.load(std::sync::atomic::Ordering::Relaxed);
            let r = ui.add(egui::Slider::new(&mut s, 0.0..=1.0));

            if r.changed() {
                self.decay.store(s, std::sync::atomic::Ordering::Relaxed);
            }
        });

        r.response
    }

    fn new(id: NodeId) -> Self {
        let (mut sink, source) = rivulet::circular_buffer::<f32>(128);
        let source = source.into_view();
        let _ = sink.try_grant(128);
        sink.view_mut().fill(0.0);
        sink.release(sink.view().len());

        let this = Self {
            id,
            inputs: PortStorage::default(),
            outputs: PortStorage::default(),
            seconds: Atomic::new(0.0),
            decay: Atomic::new(0.3),
            buffer: Arc::new(Mutex::new((source, sink))),
        };

        this
    }
}

impl SimpleNode for Reverb {
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let input_id = self.inputs.get("in").unwrap();
        let input = inputs.get(&input_id).unwrap();
        let output_id = self.outputs.get("out").unwrap();
        let output = outputs.get_mut(&output_id).unwrap();

        let mut guard = self.buffer.lock().unwrap();

        let decay = self.decay.load(std::sync::atomic::Ordering::Relaxed);

        if guard.0.try_grant(input.len()).unwrap_or(false) {
            let view = &guard.0.view()[..input.len()];

            input
                .iter()
                .zip(view.iter())
                .map(|(a, b)| a + b * decay)
                .collect_slice(*output);

            guard.0.release(input.len());
        } else {
            tracing::trace!("Reverb buffer is empty");
            output.copy_from_slice(input);
        }

        if guard.1.try_grant(input.len()).unwrap_or(false) {
            let view = &mut guard.1.view_mut()[..input.len()];

            view.copy_from_slice(output);
            guard.1.release(input.len());

            // for v in output.iter_mut() {
            //     *v = (*v + 0.5).cos();
            // }
        } else {
            tracing::trace!("Not copying frame into reverb buffer");
        }
    }
}
