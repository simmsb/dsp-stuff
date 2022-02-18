use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;
use egui::{emath::RectTransform, epaint::Shape, pos2, vec2, Color32, Frame, Rect, Stroke};
use rivulet::{
    circular_buffer::{Sink, Source},
    splittable, SplittableView, View, ViewMut,
};
use simple_moving_average::{SumTreeSMA, SMA};

pub struct WaveView {
    id: NodeId,
    outputs: Arc<HashMap<String, PortId>>,
    inputs: PortStorage,
    average_throughput: Arc<Mutex<SumTreeSMA<f32, f32, 32>>>,
    view_sink: Arc<Mutex<Sink<f32>>>,
    view_source: Arc<Mutex<splittable::View<Source<f32>>>>,
    should_count_input: Atomic<bool>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct WaveViewConfig {
    id: NodeId,
    inputs: HashMap<String, PortId>,
}

impl Node for WaveView {
    fn title(&self) -> &'static str {
        "Wave View"
    }

    fn cfg_name(&self) -> &'static str {
        "wave_view"
    }

    fn description(&self) -> &'static str {
        "Inspect the waveform of a signal"
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn inputs(&self) -> Arc<HashMap<String, PortId>> {
        self.inputs.ensure_name("in");
        self.inputs.all()
    }

    fn outputs(&self) -> Arc<HashMap<String, PortId>> {
        Arc::clone(&self.outputs)
    }

    fn save(&self) -> serde_json::Value {
        let cfg = WaveViewConfig {
            id: self.id,

            inputs: self.inputs.all().as_ref().clone(),
        };

        serde_json::to_value(cfg).unwrap()
    }

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized,
    {
        let cfg: WaveViewConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);
        this.inputs = PortStorage::new(cfg.inputs);

        this
    }

    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn render(&self, ui: &mut egui::Ui) {
        let mut source = self.view_source.lock().unwrap();
        // we need to do this so the source updates itself
        let _ = source.try_grant(128);
        let view = source.view();

        let mut averager = self.average_throughput.lock().unwrap();

        if self
            .should_count_input
            .swap(false, atomig::Ordering::Relaxed)
        {
            averager.add_sample(view.len() as f32);
        } else {
            averager.add_sample(0.0);
        }

        let current_average = averager.get_average() as usize;
        let samples_this_render = current_average.max(0).min(view.len());

        Frame::dark_canvas(ui.style()).show(ui, |ui| {
            ui.ctx().request_repaint();

            let desired_size = vec2(120.0, 50.0);
            let (_id, rect) = ui.allocate_space(desired_size);

            let to_screen =
                RectTransform::from_to(Rect::from_x_y_ranges(0.0..=1.0, -1.0..=1.0), rect);

            let points = view[..samples_this_render]
                .iter()
                .enumerate()
                .map(|(i, y)| {
                    let x = (i as f32) / samples_this_render as f32;
                    //let y = y.min(1.0).max(-1.0);

                    to_screen * pos2(x, *y)
                })
                .collect::<Vec<_>>();

            let thickness = 1.3;

            let line = Shape::line(
                points,
                Stroke::new(thickness, Color32::from_additive_luminance(196)),
            );

            ui.painter().extend(vec![line]);
        });

        ui.label(format!("Samples per frame: {}", samples_this_render));

        source.release(samples_this_render);
    }

    fn new(id: NodeId) -> Self {
        let (sink, source) = rivulet::circular_buffer::<f32>(4096);
        let source = source.into_view();
        let this = Self {
            id,
            inputs: PortStorage::default(),
            outputs: Default::default(),
            average_throughput: Arc::new(Mutex::new(SumTreeSMA::new())),
            view_sink: Arc::new(Mutex::new(sink)),
            view_source: Arc::new(Mutex::new(source)),
            should_count_input: Atomic::new(false),
        };

        this
    }
}

impl SimpleNode for WaveView {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(
        &self,
        inputs: &HashMap<PortId, &[f32]>,
        _outputs: &mut HashMap<PortId, &mut [f32]>,
    ) {
        let input_id = self.inputs.get("in").unwrap();
        let input = inputs.get(&input_id).unwrap();

        let mut sink = self.view_sink.lock().unwrap();

        if sink.try_grant(input.len()).unwrap_or(false) {
            let view = &mut sink.view_mut()[..input.len()];

            self.should_count_input
                .store(true, atomig::Ordering::Relaxed);

            view.copy_from_slice(input);
            sink.release(input.len());
        } else {
            tracing::trace!("Wave view buffer is full");
        }
    }
}
