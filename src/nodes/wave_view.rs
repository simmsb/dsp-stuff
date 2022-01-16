use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use egui::{emath::RectTransform, epaint::Shape, pos2, vec2, Color32, Frame, Rect, Stroke};
use rivulet::{
    circular_buffer::{Sink, Source},
    splittable, SplittableView, View, ViewMut,
};

pub struct WaveView {
    id: NodeId,
    outputs: Arc<HashMap<String, PortId>>,
    inputs: PortStorage,
    view_sink: Arc<Mutex<Sink<f32>>>,
    view_source: Arc<Mutex<splittable::View<Source<f32>>>>,
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

    fn render(&self, ui: &mut egui::Ui) -> egui::Response {
        let mut source = self.view_source.lock().unwrap();
        // we need to do this so the source updates itself
        let _ = source.try_grant(128);
        let view = source.view();

        let r = Frame::dark_canvas(ui.style()).show(ui, |ui| {
            ui.ctx().request_repaint();

            let desired_size = vec2(120.0, 50.0);
            let (_id, rect) = ui.allocate_space(desired_size);

            let to_screen =
                RectTransform::from_to(Rect::from_x_y_ranges(0.0..=1.0, -1.0..=1.0), rect);

            let num_samples = view.len() as f32;

            let points = view
                .iter()
                .enumerate()
                .map(|(i, y)| {
                    let x = (i as f32) / num_samples;
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

        let l = view.len();
        source.release(l);

        r.response
    }

    fn new(id: NodeId) -> Self {
        let (sink, source) = rivulet::circular_buffer::<f32>(4096);
        let source = source.into_view();
        let this = Self {
            id,
            inputs: PortStorage::default(),
            outputs: Default::default(),
            view_sink: Arc::new(Mutex::new(sink)),
            view_source: Arc::new(Mutex::new(source)),
        };

        this
    }
}

impl SimpleNode for WaveView {
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

            view.copy_from_slice(input);
            sink.release(input.len());
        } else {
            tracing::trace!("Wave view buffer is full");
        }
    }
}
