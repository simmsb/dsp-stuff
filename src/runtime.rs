use crate::ids::{LinkId, NodeId, PortId};
use crate::node::{Node, Perform};
use crate::nodes;
use egui_nodes::{LinkArgs, NodeArgs, NodeConstructor, PinArgs, PinShape};
use itertools::Itertools;
use rivulet::{
    circular_buffer::{Sink, Source},
    splittable, SplittableView, View, ViewMut,
};
use std::any::Any;
use std::{
    collections::{HashMap, HashSet},
    ops::DerefMut,
    sync::Arc,
};
use tokio::sync::Mutex;

pub struct UiContext {
    runtime: tokio::runtime::Runtime,

    node_ctx: egui_nodes::Context,

    links: HashMap<LinkId, LinkInstance>,

    inputs: HashMap<(NodeId, PortId), HashSet<LinkId>>,
    outputs: HashMap<(NodeId, PortId), HashSet<LinkId>>,

    nodes: HashMap<NodeId, NodeInstance>,
}

impl UiContext {
    pub fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .thread_name("dsp-runtime-worker")
            .build()
            .unwrap();

        Self {
            runtime,
            node_ctx: egui_nodes::Context::default(),
            links: HashMap::new(),
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            nodes: HashMap::new(),
        }
    }

    fn add_link(&mut self, lhs: (NodeId, PortId), rhs: (NodeId, PortId)) {
        tracing::debug!(?lhs, ?rhs, "Adding link");

        let inst = LinkInstance::new(lhs, rhs);
        let id = inst.id;

        self.links.insert(id, inst);
        self.outputs.entry(lhs).or_default().insert(id);
        self.inputs.entry(rhs).or_default().insert(id);

        self.update_links(lhs, rhs)
    }

    fn update_links(&mut self, lhs: (NodeId, PortId), rhs: (NodeId, PortId)) {
        let _guard = self.runtime.enter();

        let lhs_inputs = self.compute_inputs_for(lhs.0);
        let lhs_outpus = self.compute_outputs_for(lhs.0);
        self.nodes
            .get_mut(&lhs.0)
            .unwrap()
            .restart(lhs_inputs, lhs_outpus);

        let rhs_inputs = self.compute_inputs_for(rhs.0);
        let rhs_outpus = self.compute_outputs_for(rhs.0);
        self.nodes
            .get_mut(&rhs.0)
            .unwrap()
            .restart(rhs_inputs, rhs_outpus);
    }

    fn compute_inputs_for(
        &self,
        node: NodeId,
    ) -> HashMap<PortId, Vec<Arc<Mutex<splittable::View<Source<f32>>>>>> {
        let g = self
            .inputs
            .iter()
            .filter(|((n, _), _)| *n == node)
            .group_by(|((_, p), _)| p);

        g.into_iter()
            .map(|(p, v)| {
                let sources = v
                    .flat_map(|(_, ls)| {
                        ls.iter()
                            .map(|l| Arc::clone(&self.links.get(l).unwrap().source))
                    })
                    .collect::<Vec<_>>();

                (*p, sources)
            })
            .collect::<HashMap<_, _>>()
    }

    fn compute_outputs_for(&self, node: NodeId) -> HashMap<PortId, Vec<Arc<Mutex<Sink<f32>>>>> {
        let g = self
            .outputs
            .iter()
            .filter(|((n, _), _)| *n == node)
            .group_by(|((_, p), _)| p);

        g.into_iter()
            .map(|(p, v)| {
                let sources = v
                    .flat_map(|(_, ls)| {
                        ls.iter()
                            .map(|l| Arc::clone(&self.links.get(l).unwrap().sink))
                    })
                    .collect::<Vec<_>>();

                (*p, sources)
            })
            .collect::<HashMap<_, _>>()
    }

    fn update_nodes(&mut self, ui: &mut egui::Ui) {
        let nodes = self.nodes.values().map(|node| {
            let mut n = NodeConstructor::new(node.id.0, NodeArgs::default())
                .with_title(|ui| ui.label(format!("{} ({:?})", node.instance.title(), node.id)))
                .with_content(|ui| node.instance.render(ui));

            for (&input, &id) in node.instance.inputs().iter() {
                n = n.with_input_attribute(id.0, PinArgs::default(), move |ui| ui.label(input));
            }

            for (&output, &id) in node.instance.outputs().iter() {
                n = n.with_output_attribute(id.0, PinArgs::default(), move |ui| ui.label(output));
            }

            n
        });

        let links = self
            .links
            .iter()
            .map(|(id, link)| (id.0, link.lhs.1 .0, link.rhs.1 .0, LinkArgs::default()));

        self.node_ctx.show(nodes, links, ui);

        if let Some(id) = self.node_ctx.link_destroyed() {
            let id = LinkId(id);

            let inst = self.links.remove(&id).unwrap();

            self.outputs.get_mut(&inst.lhs).unwrap().remove(&id);
            self.inputs.get_mut(&inst.rhs).unwrap().remove(&id);

            self.update_links(inst.lhs, inst.rhs);
        }

        if let Some((start_port, start_node, end_port, end_node, _)) =
            self.node_ctx.link_created_node()
        {
            let start = (NodeId(start_node), PortId(start_port));
            let end = (NodeId(end_node), PortId(end_port));

            if self.inputs.contains_key(&start) && self.outputs.contains_key(&end) {
                self.add_link(end, start);
            } else if self.inputs.contains_key(&end) && self.outputs.contains_key(&start) {
                self.add_link(start, end);
            } else {
                println!("attempt to create out-out or in-in link");
            };
        }
    }

    fn add_node(&mut self, instance: Arc<dyn Perform>, instance_data: Box<dyn Any>) {
        let inst = NodeInstance::new(instance, instance_data);
        for (_, port) in inst.instance.inputs().iter() {
            self.inputs.insert((inst.id, *port), HashSet::new());
        }

        for (_, port) in inst.instance.outputs().iter() {
            self.outputs.insert((inst.id, *port), HashSet::new());
        }

        tracing::debug!(inputs = ?inst.instance.inputs(), outputs = ?inst.instance.outputs(), id = ?inst.id, "Adding node");

        self.nodes.insert(inst.id, inst);
    }
}

impl eframe::epi::App for UiContext {
    fn name(&self) -> &str {
        "DSP Stuff"
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &eframe::epi::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                egui::menu::menu_button(ui, "Effects", |ui| {
                    if ui.button("Input").clicked() {
                        let (instance, instance_data) = nodes::input::Input::new();
                        self.add_node(Arc::new(instance), instance_data);
                    }

                    if ui.button("Output").clicked() {
                        let (instance, instance_data) = nodes::output::Output::new();
                        self.add_node(Arc::new(instance), instance_data);
                    }

                    if ui.button("Distort").clicked() {
                        let (instance, instance_data) = nodes::distort::Distort::new();
                        self.add_node(Arc::new(instance), instance_data);
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.update_nodes(ui);
        });

        frame.set_window_size(ctx.used_size());
    }
}

struct LinkInstance {
    id: LinkId,

    lhs: (NodeId, PortId),
    sink: Arc<Mutex<Sink<f32>>>,

    rhs: (NodeId, PortId),
    source: Arc<Mutex<splittable::View<Source<f32>>>>,
}

impl LinkInstance {
    fn new(lhs: (NodeId, PortId), rhs: (NodeId, PortId)) -> Self {
        let id = LinkId::generate();

        let (sink, source) = rivulet::circular_buffer::<f32>(128);
        let source = source.into_view();

        Self {
            id,
            lhs,
            sink: Arc::new(Mutex::new(sink)),
            rhs,
            source: Arc::new(Mutex::new(source)),
        }
    }
}

struct NodeInstance {
    id: NodeId,
    instance: Arc<dyn Perform>,
    instance_data: Box<dyn Any>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl NodeInstance {
    fn new(instance: Arc<dyn Perform>, instance_data: Box<dyn Any>) -> Self {
        Self {
            id: NodeId::generate(),
            instance,
            instance_data,
            task: None,
        }
    }

    fn start(
        &mut self,
        mut inputs: HashMap<PortId, Vec<Arc<Mutex<splittable::View<Source<f32>>>>>>,
        mut outputs: HashMap<PortId, Vec<Arc<Mutex<Sink<f32>>>>>,
    ) {
        assert!(self.task.is_none());
        let id = self.id;
        tracing::debug!(?id, "Starting node");

        let instance = Arc::clone(&self.instance);

        let coro = async move {
            // this is horrible
            // should be fine though since we only do this if the graph is edited

            let mut input_slices_v = HashMap::with_capacity(inputs.len());
            for (id, input) in &mut inputs {
                let mut guards = Vec::with_capacity(input.len());

                for source in input {
                    guards.push(Arc::clone(source).lock_owned().await);
                }

                input_slices_v.insert(*id, guards);
            }

            let mut input_slices = input_slices_v
                .iter_mut()
                .map(|(id, input)| {
                    (
                        *id,
                        input.iter_mut().map(|g| g.deref_mut()).collect::<Vec<_>>(),
                    )
                })
                .collect::<HashMap<_, _>>();

            let mut output_slices_v = HashMap::with_capacity(outputs.len());
            for (id, output) in &mut outputs {
                let mut guards = Vec::new();

                for sink in output {
                    guards.push(Arc::clone(sink).lock_owned().await);
                }

                output_slices_v.insert(*id, guards);
            }

            let mut output_slices = output_slices_v
                .iter_mut()
                .map(|(id, output)| {
                    (
                        *id,
                        output.iter_mut().map(|g| g.deref_mut()).collect::<Vec<_>>(),
                    )
                })
                .collect::<HashMap<_, _>>();

            let mut input_slices = input_slices
                .iter_mut()
                .map(|(id, x)| (*id, x.as_mut_slice()))
                .collect::<HashMap<_, _>>();

            let mut output_slices = output_slices
                .iter_mut()
                .map(|(id, x)| (*id, x.as_mut_slice()))
                .collect::<HashMap<_, _>>();

            tracing::debug!(?id, "Started node, beginning to loop");
            loop {
                // tracing::debug!(?id, "Performing node");

                instance
                    .perform(&mut input_slices, &mut output_slices)
                    .await
            }
        };

        self.task = Some(tokio::spawn(coro));
    }

    fn stop(&mut self) {
        tracing::debug!(id = ?self.id, "Stopping node");
        if let Some(handle) = self.task.take() {
            handle.abort();
        }
    }

    fn restart(
        &mut self,
        inputs: HashMap<PortId, Vec<Arc<Mutex<splittable::View<Source<f32>>>>>>,
        outputs: HashMap<PortId, Vec<Arc<Mutex<Sink<f32>>>>>,
    ) {
        tracing::debug!(id = ?self.id, "Restarting node");
        self.stop();
        self.start(inputs, outputs)
    }
}
