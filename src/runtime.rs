use crate::{
    ids::{LinkId, NodeId, PortId},
    node::Perform,
    nodes,
    theme::{self, Theme},
};
use egui::Visuals;
use egui_nodes::{AttributeFlags, ColorStyle, LinkArgs, NodeArgs, NodeConstructor, PinArgs};
use itertools::Itertools;
use rivulet::{
    circular_buffer::{Sink, Source},
    splittable, SplittableView,
};
use std::{
    collections::{HashMap, HashSet},
    ops::DerefMut,
    sync::Arc,
};
use tokio::sync::Mutex;

pub struct UiContext {
    runtime: tokio::runtime::Runtime,

    theme: &'static Theme,

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

        let mut node_ctx = egui_nodes::Context::default();
        node_ctx.attribute_flag_push(AttributeFlags::EnableLinkDetachWithDragClick);

        let mut this = Self {
            runtime,
            node_ctx,
            theme: &theme::MONOKAI,
            links: HashMap::new(),
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            nodes: HashMap::new(),
        };

        this.update_theme(&theme::MONOKAI);

        this
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
                n = n.with_input_attribute(id.0, PinArgs::default(), move |ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                        ui.label(input)
                    })
                    .inner
                });
            }

            for (&output, &id) in node.instance.outputs().iter() {
                n = n.with_output_attribute(id.0, PinArgs::default(), move |ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                        ui.label(output)
                    })
                    .inner
                });
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

            if let Some(inst) = self.links.remove(&id) {
                self.outputs.get_mut(&inst.lhs).unwrap().remove(&id);
                self.inputs.get_mut(&inst.rhs).unwrap().remove(&id);

                self.update_links(inst.lhs, inst.rhs);
                tracing::debug!(link = ?inst, "Removing link");
            } else {
                tracing::warn!("GUI told us to remove link {:?} which isn't tracked", id);
            }
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
                tracing::debug!("Attempt to create out-out or in-in link between {:?}, {:?}", start, end);
            };
        }
    }

    fn add_node(&mut self, id: NodeId, instance: Arc<dyn Perform>) {
        let inst = NodeInstance::new(id, instance);
        for (_, port) in inst.instance.inputs().iter() {
            self.inputs.insert((inst.id, *port), HashSet::new());
        }

        for (_, port) in inst.instance.outputs().iter() {
            self.outputs.insert((inst.id, *port), HashSet::new());
        }

        tracing::debug!(inputs = ?inst.instance.inputs(), outputs = ?inst.instance.outputs(), id = ?inst.id, "Adding node");

        self.nodes.insert(inst.id, inst);
    }

    fn update_theme(&mut self, theme: &'static Theme) {
        self.theme = theme;
        self.node_ctx.style.colors[ColorStyle::Pin as usize] = theme.link;
        self.node_ctx.style.colors[ColorStyle::PinHovered as usize] = theme.link_hovered;
        self.node_ctx.style.colors[ColorStyle::Link as usize] = theme.link;
        self.node_ctx.style.colors[ColorStyle::LinkHovered as usize] = theme.link_hovered;
        self.node_ctx.style.colors[ColorStyle::LinkSelected as usize] = theme.link_hovered;
        self.node_ctx.style.colors[ColorStyle::TitleBar as usize] = theme.titlebar;
        self.node_ctx.style.colors[ColorStyle::TitleBarHovered as usize] = theme.titlebar_hovered;
        self.node_ctx.style.colors[ColorStyle::TitleBarSelected as usize] = theme.titlebar_hovered;
        self.node_ctx.style.colors[ColorStyle::NodeBackground as usize] = theme.node_background;
        self.node_ctx.style.colors[ColorStyle::NodeBackgroundHovered as usize] =
            theme.node_background_hovered;
        self.node_ctx.style.colors[ColorStyle::NodeBackgroundSelected as usize] =
            theme.node_background_hovered;
        self.node_ctx.style.colors[ColorStyle::GridBackground as usize] = theme.grid_background;
    }
}

impl eframe::epi::App for UiContext {
    fn name(&self) -> &str {
        "DSP Stuff"
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &eframe::epi::Frame) {
        let mut visuals = if self.theme.dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        visuals.window_corner_radius = 3.0;
        visuals.override_text_color = Some(self.theme.text);
        visuals.widgets.active.bg_fill = self.theme.node_background;
        visuals.widgets.hovered.bg_fill = self.theme.node_background_hovered;
        visuals.widgets.open.bg_fill = self.theme.grid_background;
        visuals.widgets.inactive.bg_fill = self.theme.grid_background;

        ctx.set_visuals(visuals);

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                egui::menu::menu_button(ui, "Effects", |ui| {
                    for (name, ctor) in nodes::NODES {
                        if ui.button(*name).clicked() {
                            let id = NodeId::generate();
                            self.add_node(id, ctor(id));
                        }
                    }
                });

                egui::menu::menu_button(ui, "Theme", |ui| {
                    for (name, theme) in theme::THEMES {
                        if ui.button(*name).clicked() {
                            self.update_theme(theme);
                        }
                    }
                })
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.update_nodes(ui);
        });

        frame.set_window_size(ctx.used_size());
    }
}

#[derive(derivative::Derivative)]
#[derivative(Debug)]
struct LinkInstance {
    id: LinkId,

    lhs: (NodeId, PortId),
    #[derivative(Debug = "ignore")]
    sink: Arc<Mutex<Sink<f32>>>,

    rhs: (NodeId, PortId),
    #[derivative(Debug = "ignore")]
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
    task: Option<tokio::task::JoinHandle<()>>,
}

impl NodeInstance {
    fn new(id: NodeId, instance: Arc<dyn Perform>) -> Self {
        Self {
            id,
            instance,
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
