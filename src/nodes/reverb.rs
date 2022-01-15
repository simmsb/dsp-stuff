use std::{collections::HashMap, sync::Arc};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomic_float::AtomicF32;
use collect_slice::CollectSlice;
use rivulet::{
    circular_buffer::{Sink, Source},
    splittable, SplittableView, View, ViewMut,
};
use std::sync::Mutex;

pub struct Reverb {
    _id: NodeId,
    inputs: PortStorage,
    outputs: PortStorage,
    seconds: AtomicF32,
    decay: AtomicF32,
    buffer: Arc<Mutex<(splittable::View<Source<f32>>, Sink<f32>)>>,
}

impl Node for Reverb {
    fn title(&self) -> &'static str {
        "Reverb"
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
        ui.horizontal(|ui| {
            ui.label("Delay (s)");

            let mut s = self.seconds.load(std::sync::atomic::Ordering::Relaxed);
            let r = ui.add(egui::Slider::new(&mut s, 0.0..=10.0));

            if r.changed() {
                self.seconds.store(s, std::sync::atomic::Ordering::Relaxed);

                let num_samples = ((s * 48000.0) as usize).max(128);

                let (mut new_sink, new_source) = rivulet::circular_buffer::<f32>(num_samples);
                let new_source = new_source.into_view();

                let mut guard = self.buffer.lock().unwrap();

                let _ = new_sink.try_grant(num_samples);
                new_sink.view_mut().fill(0.0);
                let num_zeros = new_sink.view().len();
                new_sink.release(num_zeros);

                *guard = (new_source, new_sink);
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
            _id: id,
            inputs: PortStorage::default(),
            outputs: PortStorage::default(),
            seconds: AtomicF32::new(0.0),
            decay: AtomicF32::new(0.3),
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
            tracing::info!("not doing it in");
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
            tracing::info!("Not doing it out");
        }
    }
}
