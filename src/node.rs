use std::{collections::HashMap, sync::Arc};

use arc_swap::ArcSwap;
use rivulet::{
    circular_buffer::{Sink, Source},
    splittable, View, ViewMut,
};

use crate::ids::{NodeId, PortId};

pub type NodeInputs<'a, 'b, 'c> =
    &'a mut HashMap<PortId, &'b mut [&'c mut splittable::View<Source<f32>>]>;
pub type NodeOutputs<'a, 'b, 'c> = &'a mut HashMap<PortId, &'b mut [&'c mut Sink<f32>]>;

#[derive(Debug, Default)]
pub struct PortStorage(ArcSwap<HashMap<String, PortId>>);

impl PortStorage {
    pub fn new(inner: HashMap<String, PortId>) -> Self {
        PortStorage(ArcSwap::new(Arc::new(inner)))
    }

    pub fn ensure_name(&self, name: &str) {
        self.0.rcu(|x| {
            let mut map = HashMap::new();
            map.clone_from(x);
            map.entry(name.to_owned()).or_insert_with(PortId::generate);
            map
        });
    }

    pub fn get(&self, name: &str) -> Option<PortId> {
        self.0.load().get(name).cloned()
    }

    pub fn all(&self) -> Arc<HashMap<String, PortId>> {
        self.0.load_full()
    }
}

pub trait Node: Send + Sync {
    fn title(&self) -> &'static str;

    fn cfg_name(&self) -> &'static str;

    fn description(&self) -> &'static str;

    fn id(&self) -> NodeId;

    /// The ids and names of the input nodes
    fn inputs(&self) -> Arc<HashMap<String, PortId>>;

    /// The ids and names of the output nodes
    fn outputs(&self) -> Arc<HashMap<String, PortId>>;

    fn render(&self, ui: &mut egui::Ui);

    fn new(id: NodeId) -> Self
    where
        Self: Sized;

    fn save(&self) -> serde_json::Value;

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized;
}

pub trait SimpleNode: Node {
    /// Perform this node over given inputs and outputs
    ///
    /// The input to this function is a slice of frames, one for each input
    /// declared via 'inputs'
    ///
    /// This function should write one frame into each output
    ///
    /// Inputs are pre-mixed, that is, that a frame from each connection to a
    /// given input is collected, and averaged.
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>);
}

#[async_trait::async_trait]
pub trait Perform: Node {
    /// Perform this node
    ///
    /// This function may await things for as long as it likes, however it
    /// should probably way of quitting early if the node is modified in some
    /// way.
    async fn perform(&self, inputs: NodeInputs<'_, '_, '_>, outputs: NodeOutputs<'_, '_, '_>);
}

pub async fn collect_and_average(
    buf_size: usize,
    input: &mut [&mut splittable::View<Source<f32>>],
) -> Vec<f32> {
    let mut out = vec![0.0; buf_size];

    let mut num_frames = 0.0001;

    for in_ in input.iter_mut() {
        in_.grant(buf_size).await.unwrap();
        if in_.view().len() < buf_size {
            continue;
        }

        num_frames += 1.0;

        for (a, b) in out.iter_mut().zip(in_.view()[..buf_size].iter()) {
            *a += b;
        }
    }

    // NOTE: this function doesn't release the views, that should be done later
    // as an atomic operation

    for v in out.iter_mut() {
        *v /= num_frames;
    }

    out
}

#[async_trait::async_trait]
impl<T: SimpleNode> Perform for T {
    async fn perform(&self, inputs: NodeInputs<'_, '_, '_>, outputs: NodeOutputs<'_, '_, '_>) {
        let buf_size = 128;

        // prep outputs

        let mut output_bufs = outputs
            .keys()
            .map(|k| (*k, vec![0.0f32; buf_size]))
            .collect::<HashMap<_, _>>();
        let mut collected_output_bufs = output_bufs
            .iter_mut()
            .map(|(k, v)| (*k, &mut v[..buf_size]))
            .collect::<HashMap<_, _>>();

        for (k, output) in outputs.iter_mut() {
            tracing::trace!(name = self.title(), id = ?self.id(), "Waiting for {} outputs on port {:?}", output.len(), k);
            for out in output.iter_mut() {
                out.grant(buf_size).await.unwrap();
            }
        }

        // prep inputs

        let mut collected_inputs: HashMap<PortId, Vec<f32>> = HashMap::with_capacity(inputs.len());
        for (k, input) in inputs.iter_mut() {
            tracing::trace!(name = self.title(), id = ?self.id(), "collecting {} inputs for port {:?}", input.len(), k);

            collected_inputs.insert(*k, collect_and_average(buf_size, input).await);
        }

        let collected_input_bufs = collected_inputs
            .iter_mut()
            .map(|(k, v)| (*k, v.as_slice()))
            .collect::<HashMap<_, _>>();

        // run process

        self.process(&collected_input_bufs, &mut collected_output_bufs);

        // copy outputs

        for (id, output) in outputs.iter_mut() {
            let buf = collected_output_bufs.get(id).unwrap();
            for out in output.iter_mut() {
                out.view_mut()[..buf_size].copy_from_slice(buf);
            }
        }

        // release inputs

        for input in inputs.values_mut() {
            for in_ in input.iter_mut() {
                // if the view is less than the buf size, then we didn't actually read from it
                // but skip it anyway
                in_.release(buf_size.min(in_.view().len()));
            }
        }

        // release outputs

        for output in outputs.values_mut() {
            for out in output.iter_mut() {
                out.release(buf_size);
            }
        }
    }
}
