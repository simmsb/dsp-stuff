use std::{any::Any, collections::HashMap, sync::Arc};

use crate::{devices::input_stream, ids::PortId, node::*};
use cpal::traits::{DeviceTrait, StreamTrait};
use rivulet::{circular_buffer::Source, splittable, View, ViewMut};
use tokio::sync::Mutex;

pub struct Input {
    inputs: Arc<HashMap<&'static str, PortId>>,
    outputs: PortStorage,
    name: Arc<String>,
    source: Arc<Mutex<splittable::View<Source<f32>>>>,
}

impl Node for Input {
    fn title(&self) -> &'static str {
        "Input"
    }

    fn inputs(&self) -> Arc<HashMap<&'static str, PortId>> {
        Arc::clone(&self.inputs)
    }

    fn outputs(&self) -> Arc<HashMap<&'static str, PortId>> {
        self.outputs.get_or_create("out");
        self.outputs.all()
    }

    fn render(&self, ui: &mut egui::Ui) -> egui::Response {
        ui.label(self.name.as_str())
    }

    fn new() -> (Self, Box<dyn Any>) {
        let (dev, stream, source) = input_stream();
        stream.play().unwrap();

        let this = Self {
            inputs: Default::default(),
            outputs: PortStorage::default(),
            name: Arc::new(dev.name().unwrap()),
            source: Arc::new(Mutex::new(source)),
        };

        (this, Box::new(stream))
    }
}

#[async_trait::async_trait]
impl Perform for Input {
    async fn perform(&self, _inputs: NodeInputs<'_, '_, '_>, outputs: NodeOutputs<'_, '_, '_>) {
        let buf_size = 128;

        let mut source = self.source.lock().await;

        source.grant(buf_size).await.unwrap();

        for output in outputs.values_mut() {
            for out in output.iter_mut() {
                out.grant(buf_size).await.unwrap();
                out.view_mut()[..buf_size].copy_from_slice(&source.view()[..buf_size]);
            }
        }

        // tracing::debug!("Releasing source");
        source.release(buf_size);

        // tracing::debug!("Releasing outputs");
        for output in outputs.values_mut() {
            for out in output.iter_mut() {
                out.release(buf_size);
            }
        }
    }
}
