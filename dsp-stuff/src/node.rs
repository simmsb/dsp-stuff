use std::{collections::HashMap, sync::Arc};

use once_cell::sync::Lazy;
use rivulet::{
    circular_buffer::{Sink, Source},
    splittable, View, ViewMut,
};
use rsor::Slice;
use serde::{Deserialize, Serialize};
use sharded_slab::{Clear, Pool};
use std::sync::RwLock;

use crate::ids::{NodeId, PortId};

pub type NodeInputs<'a, 'b, 'c> = &'a mut [&'b mut [&'c mut splittable::View<Source<f32>>]];
pub type NodeOutputs<'a, 'b, 'c> = &'a mut [&'b mut [&'c mut Sink<f32>]];

#[derive(Debug, Default, Clone)]
pub struct PortStorageInner {
    pub ports: HashMap<String, PortId>,
    pub local_indexes: HashMap<String, usize>,
    pub portid_indexes: HashMap<PortId, usize>,
    pub deleted: Vec<PortId>,
}

impl PortStorageInner {
    fn new(ports: HashMap<String, PortId>) -> Self {
        let local_indexes = ports
            .keys()
            .enumerate()
            .map(|(i, k)| (k.to_owned(), i))
            .collect();
        let portid_indexes = ports.values().enumerate().map(|(i, v)| (*v, i)).collect();
        Self {
            ports,
            local_indexes,
            portid_indexes,
            deleted: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct PortStorage(pub Arc<RwLock<PortStorageInner>>);

impl Serialize for PortStorage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.get_all().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PortStorage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let d = <HashMap<String, PortId> as Deserialize>::deserialize(deserializer)?;

        Ok(PortStorage::new(d))
    }
}

impl PortStorage {
    pub fn new(inner: HashMap<String, PortId>) -> Self {
        PortStorage(Arc::new(RwLock::new(PortStorageInner::new(inner))))
    }

    pub fn add(&self, name: String) {
        let mut inner = self.0.write().unwrap();
        let idx = inner.ports.len();
        let pid = PortId::generate();
        inner.ports.insert(name.clone(), pid);
        inner.local_indexes.insert(name, idx);
        inner.portid_indexes.insert(pid, idx);
    }

    pub fn get_id(&self, name: &str) -> Option<PortId> {
        self.0.read().unwrap().ports.get(name).copied()
    }

    pub fn get_idx(&self, name: &str) -> Option<usize> {
        self.0.read().unwrap().local_indexes.get(name).copied()
    }

    pub fn get_portid_idx(&self, id: PortId) -> Option<usize> {
        self.0.read().unwrap().portid_indexes.get(&id).copied()
    }

    pub fn get_all(&self) -> HashMap<String, PortId> {
        self.0.read().unwrap().ports.clone()
    }

    pub fn get_idxs(&self) -> HashMap<PortId, usize> {
        self.0.read().unwrap().portid_indexes.clone()
    }
}

pub trait Node: Send + Sync {
    fn title(&self) -> &'static str;

    fn cfg_name(&self) -> &'static str;

    fn description(&self) -> &'static str;

    fn id(&self) -> NodeId;

    /// The ids and names of the input nodes
    fn inputs(&self) -> &PortStorage;

    /// The ids and names of the output nodes
    fn outputs(&self) -> &PortStorage;

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
    fn process(&self, inputs: ProcessInput, outputs: ProcessOutput);
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
    output: &mut [f32],
    input: &mut [&mut splittable::View<Source<f32>>],
) -> bool {
    let mut num_frames = 0.0001;

    let buf_size = output.len();

    let mut r = false;

    for in_ in input.iter_mut() {
        in_.grant(buf_size).await.unwrap();
        if in_.view().len() < buf_size {
            continue;
        }

        r = true;
        num_frames += 1.0;

        for (a, b) in output.iter_mut().zip(in_.view()[..buf_size].iter()) {
            *a += b;
        }
    }

    // NOTE: this function doesn't release the views, that should be done later
    // as an atomic operation

    for v in output.iter_mut() {
        *v /= num_frames;
    }

    r
}

#[derive(Default, Debug)]
struct NoClear<T>(T);

impl<T> Clear for NoClear<T> {
    fn clear(&mut self) {}
}

impl<T> std::ops::Deref for NoClear<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for NoClear<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
pub struct ProcessInput<'ports, 'buf, 'ps> {
    storage: &'ps PortStorage,
    inputs: &'ports [&'buf [f32]],
    present: &'ports [bool],
}

impl<'ports, 'buf, 'ps> ProcessInput<'ports, 'buf, 'ps> {
    pub fn get(&self, name: &str) -> Option<&'buf [f32]> {
        let idx = self.storage.get_idx(name)?;
        Some(self.inputs[idx])
    }

    pub fn get_checked(&self, name: &str) -> Option<&'buf [f32]> {
        let idx = self.storage.get_idx(name)?;
        if self.present[idx] {
            Some(self.inputs[idx])
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct ProcessOutput<'ports, 'buf, 'ps> {
    storage: &'ps PortStorage,
    outputs: &'ports mut [&'buf mut [f32]],
}

impl<'ports, 'buf, 'ps> ProcessOutput<'ports, 'buf, 'ps> {
    pub fn get(&mut self, name: &str) -> Option<&mut [f32]> {
        let idx = self.storage.get_idx(name)?;
        Some(self.outputs[idx])
    }
}

static PRESENT_INPUT_POOL: Lazy<Arc<Pool<Vec<bool>>>> = Lazy::new(|| Arc::new(Pool::new()));
static BUF_POOL: Lazy<Arc<Pool<Vec<f32>>>> = Lazy::new(|| Arc::new(Pool::new()));
static REF_POOL: Lazy<Arc<Pool<NoClear<Slice<[f32]>>>>> = Lazy::new(|| Arc::new(Pool::new()));

#[async_trait::async_trait]
impl<T: SimpleNode> Perform for T {
    async fn perform(&self, inputs: NodeInputs<'_, '_, '_>, outputs: NodeOutputs<'_, '_, '_>) {
        let buf_size = 128;

        // prep outputs

        let mut output_buf = BUF_POOL.clone().create_owned().unwrap();
        output_buf.resize(outputs.len() * buf_size, 0.0);

        let mut output_slice_slice = REF_POOL.clone().create_owned().unwrap();
        let output_slice = output_slice_slice.from_iter_mut(output_buf.chunks_mut(buf_size));

        for (idx, output_port) in outputs.iter_mut().enumerate() {
            tracing::trace!(name = self.title(), id = ?self.id(), "Waiting for {} outputs on port {}", output_port.len(), idx);
            for output_pipe in output_port.iter_mut() {
                output_pipe.grant(buf_size).await.unwrap();
            }
        }

        // prep inputs

        let mut present_inputs = PRESENT_INPUT_POOL.clone().create_owned().unwrap();
        let mut input_buf = BUF_POOL.clone().create_owned().unwrap();
        input_buf.resize(inputs.len() * buf_size, 0.0);

        for (idx, (pipes, buf)) in inputs.iter_mut().zip(input_buf.chunks_mut(buf_size)).enumerate() {
            tracing::trace!(name = self.title(), id = ?self.id(), "Waiting for {} inputs on port {}", pipes.len(), idx);

            let present = collect_and_average(buf, *pipes).await;
            present_inputs.push(present);
        }

        let mut input_slice_slice = REF_POOL.create().unwrap();
        let input_slice = input_slice_slice.from_iter(input_buf.chunks(buf_size));

        // run process

        let pinput = ProcessInput {
            storage: self.inputs(),
            inputs: input_slice,
            present: &present_inputs,
        };

        let poutput = ProcessOutput {
            storage: self.outputs(),
            outputs: output_slice,
        };

        self.process(pinput, poutput);

        REF_POOL.clear(input_slice_slice.key());
        REF_POOL.clear(output_slice_slice.key());
        BUF_POOL.clear(input_buf.key());
        BUF_POOL.clear(output_buf.key());

        // copy outputs

        for (output_port, buf) in outputs.iter_mut().zip(output_buf.chunks(buf_size)) {
            for output_pipe in output_port.iter_mut() {
                output_pipe.view_mut()[..buf_size].copy_from_slice(buf);
            }
        }

        // release inputs

        for input_port in inputs.iter_mut() {
            for input_pipe in input_port.iter_mut() {
                // if the view is less than the buf size, then we didn't actually read from it
                // but skip it anyway
                // however this shouldn't actuall happen
                input_pipe.release(buf_size.min(input_pipe.view().len()));
            }
        }

        // release outputs

        for output_port in outputs.iter_mut() {
            for output_pipe in output_port.iter_mut() {
                output_pipe.release(buf_size);
            }
        }
    }
}
