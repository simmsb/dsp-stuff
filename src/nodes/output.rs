use std::{any::Any, collections::HashMap, sync::Arc};

use crate::{devices::output_stream, ids::PortId, node::*};
use cpal::traits::{DeviceTrait, StreamTrait};
use rivulet::{circular_buffer::Sink, View, ViewMut};
use tokio::sync::Mutex;

pub struct Output {
    outputs: Arc<HashMap<&'static str, PortId>>,
    inputs: PortStorage,
    name: Arc<String>,
    sink: Arc<Mutex<Sink<f32>>>,
}

impl Node for Output {
    fn title(&self) -> &'static str {
        "Output"
    }

    fn inputs(&self) -> Arc<HashMap<&'static str, PortId>> {
        self.inputs.get_or_create("in");
        self.inputs.all()
    }

    fn outputs(&self) -> Arc<HashMap<&'static str, PortId>> {
        Arc::clone(&self.outputs)
    }

    fn render(&self, ui: &mut egui::Ui) -> egui::Response {
        ui.label(self.name.as_str())
    }

    fn new() -> (Self, Box<dyn Any>) {
        let (dev, stream, sink) = output_stream();
        stream.play().unwrap();

        let this = Self {
            inputs: PortStorage::default(),
            outputs: Default::default(),
            name: Arc::new(dev.name().unwrap()),
            sink: Arc::new(Mutex::new(sink)),
        };

        (this, Box::new(stream))
    }
}

#[async_trait::async_trait]
impl Perform for Output {
    async fn perform(&self, inputs: NodeInputs<'_, '_, '_>, _outputs: NodeOutputs<'_, '_, '_>) {
        let buf_size = 128;

        let mut sink = self.sink.lock().await;

        let collected_inputs = inputs.get_mut(&self.inputs.get("in").unwrap()).unwrap();

        let merged = collect_and_average(buf_size, collected_inputs).await;

        // tracing::debug!(merged = merged.len(), "Done a collection");

        sink.grant(buf_size).await.unwrap();

        // tracing::debug!(sink_view = sink.view_mut().len(), "Got a grant");

        sink.view_mut()[..buf_size].copy_from_slice(&merged);

        // tracing::debug!("Releasing sink");
        sink.release(buf_size);

        // tracing::debug!("Releasing inputs");
        for input in inputs.values_mut() {
            for in_ in input.iter_mut() {
                in_.release(buf_size);
            }
        }
    }
}
