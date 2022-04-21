use crate::{
    devices,
    ids::{LinkId, NodeId, PortId},
    node::Perform,
    nodes,
    theme::{self, Theme},
    Params,
};
use eframe::CreationContext;
use egui::{pos2, Visuals};
use egui_nodes::{AttributeFlags, ColorStyle, LinkArgs, NodeArgs, NodeConstructor, PinArgs};
use itertools::Itertools;
use rivulet::{
    circular_buffer::{Sink, Source},
    splittable, SplittableView,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    io::Write,
    ops::DerefMut,
    rc::Rc,
    sync::Arc,
};
use tokio::{runtime::Handle, sync::Mutex};

pub struct UiContext {
    runtime: tokio::runtime::Runtime,

    theme: &'static Theme,

    node_ctx: egui_nodes::Context,

    links: HashMap<LinkId, LinkInstance>,

    inputs: HashMap<(NodeId, PortId), HashSet<LinkId>>,
    outputs: HashMap<(NodeId, PortId), HashSet<LinkId>>,

    nodes: HashMap<NodeId, NodeInstance>,
}

#[derive(Serialize, Deserialize)]
struct DSPConfig {
    nodes: Vec<NodeConfig>,
    links: Vec<LinkConfig>,
}

impl UiContext {
    pub fn new(cc: &CreationContext, params: &Params) -> Self {
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

        if let Some(s) = cc.storage {
            if !params.clean {
                if let Some(cfg) = s
                    .get_string("graph_state")
                    .and_then(|s| serde_json::from_str(&s).ok())
                {
                    let _guard = this.runtime.enter();
                    this.restore_config(cfg);
                }
            }
        }

        this
    }

    fn save_config(&self) -> DSPConfig {
        let nodes = self.nodes.values().map(|n| n.save()).collect();
        let links = self.links.values().map(|l| l.save()).collect();

        DSPConfig { nodes, links }
    }

    fn restore_config(&mut self, cfg: DSPConfig) {
        for node in self.nodes.values_mut() {
            node.stop()
        }

        self.links.clear();
        self.inputs.clear();
        self.outputs.clear();
        self.nodes.clear();

        for node in cfg.nodes {
            let restored = NodeInstance::restore(node);

            for port in restored.instance.inputs().get_all().values() {
                self.inputs.entry((restored.id, *port)).or_default();
            }

            for port in restored.instance.outputs().get_all().values() {
                self.outputs.entry((restored.id, *port)).or_default();
            }

            self.nodes.insert(restored.id, restored);
        }

        for link in cfg.links {
            self.add_link(link.lhs, link.rhs);
        }

        self.update_all();
    }

    fn add_link(&mut self, lhs: (NodeId, PortId), rhs: (NodeId, PortId)) {
        let id = LinkId::generate();
        let inst = LinkInstance::new(id, lhs, rhs);

        tracing::info!(link = ?inst, "Adding link");

        self.links.insert(id, inst);
        self.outputs.entry(lhs).or_default().insert(id);
        self.inputs.entry(rhs).or_default().insert(id);
    }

    fn update_all(&mut self) {
        let calculated = self
            .nodes
            .values()
            .map(|node| {
                (
                    self.compute_inputs_for(node.id),
                    self.compute_outputs_for(node.id),
                )
            })
            .collect::<Vec<_>>();

        for (node, (inputs, outputs)) in self.nodes.values_mut().zip(calculated) {
            node.restart(inputs, outputs);
        }
    }

    fn restart_node(&mut self, node: NodeId) {
        let inputs = self.compute_inputs_for(node);
        let outpus = self.compute_outputs_for(node);
        self.nodes.get_mut(&node).unwrap().restart(inputs, outpus);
    }

    fn compute_inputs_for(
        &self,
        node: NodeId,
    ) -> Vec<Vec<Arc<Mutex<splittable::View<Source<f32>>>>>> {
        let storage = self.nodes.get(&node).unwrap().instance.inputs();

        let g = self
            .inputs
            .iter()
            .filter(|((n, _), _)| *n == node)
            .group_by(|((_, p), _)| p);

        let mut v = g
            .into_iter()
            .map(|(p, v)| {
                let sources = v
                    .flat_map(|(_, ls)| {
                        ls.iter()
                            .map(|l| Arc::clone(&self.links.get(l).unwrap().source))
                    })
                    .collect::<Vec<_>>();

                (*p, sources)
            })
            .collect::<HashMap<_, _>>();

        storage
            .get_idxs()
            .into_iter()
            .map(|(pid, idx)| (idx, v.remove(&pid).unwrap_or_default()))
            .sorted_by_key(|(idx, _)| *idx)
            .map(|(_, v)| v)
            .collect_vec()
    }

    fn compute_outputs_for(&self, node: NodeId) -> Vec<Vec<Arc<Mutex<Sink<f32>>>>> {
        let storage = self.nodes.get(&node).unwrap().instance.outputs();

        let g = self
            .outputs
            .iter()
            .filter(|((n, _), _)| *n == node)
            .group_by(|((_, p), _)| p);

        let mut v = g
            .into_iter()
            .map(|(p, v)| {
                let sources = v
                    .flat_map(|(_, ls)| {
                        ls.iter()
                            .map(|l| Arc::clone(&self.links.get(l).unwrap().sink))
                    })
                    .collect::<Vec<_>>();

                (*p, sources)
            })
            .collect::<HashMap<_, _>>();

        storage
            .get_idxs()
            .into_iter()
            .map(|(pid, idx)| (idx, v.remove(&pid).unwrap_or_default()))
            .sorted_by_key(|(idx, _)| *idx)
            .map(|(_, v)| v)
            .collect_vec()
    }

    fn update_nodes(&mut self, ui: &mut egui::Ui) {
        for node in self.nodes.values_mut() {
            if let Some(pos) = self.node_ctx.get_node_pos_screen_space(node.id.get()) {
                node.position = pos;
            }
        }

        let nodes_to_delete = Rc::new(RefCell::new(Vec::new()));

        let nodes: Vec<NodeConstructor> = self
            .nodes
            .values()
            .map(|node| {
                let nodes_to_delete = Rc::clone(&nodes_to_delete);
                let mut n = NodeConstructor::new(node.id.get(), NodeArgs::default());
                n.with_title(move |ui| {
                    ui.horizontal(|ui| {
                        let mut inner_ui = egui::Ui::new(
                            ui.ctx().clone(),
                            ui.layer_id(),
                            egui::Id::new((node.id, "title")),
                            ui.max_rect(),
                            ui.clip_rect(),
                        );
                        ui.label(format!("{} ({})", node.instance.title(), node.id.get()))
                            .on_hover_text_at_pointer(node.instance.description());
                        inner_ui.with_layout(egui::Layout::right_to_left(), move |ui| {
                            if ui.add(egui::Button::new("Close")).clicked() {
                                nodes_to_delete.borrow_mut().push(node.id);
                            }
                        })
                    })
                    .response
                })
                .with_content(|ui| node.instance.render(ui))
                .with_origin(node.position);

                for (input, id) in node
                    .instance
                    .inputs()
                    .0
                    .read()
                    .unwrap()
                    .ports
                    .iter()
                    .sorted_by_key(|(k, _)| *k)
                    .map(|(k, v)| (k.to_owned(), *v))
                {
                    n.with_input_attribute(id.get(), PinArgs::default(), move |ui| {
                        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                            ui.label(input)
                        })
                        .inner
                    });
                }

                for (output, id) in node
                    .instance
                    .outputs()
                    .0
                    .read()
                    .unwrap()
                    .ports
                    .iter()
                    .sorted_by_key(|(k, _)| *k)
                    .map(|(k, v)| (k.to_owned(), *v))
                {
                    n.with_output_attribute(id.get(), PinArgs::default(), move |ui| {
                        ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                            ui.label(output)
                        })
                        .inner
                    });
                }

                n
            })
            .sorted_by_key(|n| n.id())
            .collect_vec();

        let links = self
            .links
            .values()
            .enumerate()
            .map(|(idx, link)| (idx, link.lhs.1.get(), link.rhs.1.get(), LinkArgs::default()))
            .collect::<Vec<_>>();

        self.node_ctx.show(nodes, links, ui);

        if let Some(idx) = self.node_ctx.link_destroyed() {
            if let Some(&id) = self.links.keys().nth(idx) {
                if let Some(inst) = self.links.remove(&id) {
                    tracing::info!(link = ?inst, "Removing link");
                    self.outputs.get_mut(&inst.lhs).unwrap().remove(&id);
                    self.inputs.get_mut(&inst.rhs).unwrap().remove(&id);

                    self.restart_node(inst.lhs.0);
                    self.restart_node(inst.rhs.0);
                    devices::invoke(devices::DeviceCommand::TriggerResync);
                } else {
                    tracing::warn!("GUI told us to remove link {:?} which isn't tracked", id);
                }
            } else {
                tracing::warn!(links = ?self.links, "GUI told us to remove link idx {} which isn't known", idx);
            }
        }

        if let Some((start_port, start_node, end_port, end_node, _)) =
            self.node_ctx.link_created_node()
        {
            let start = (NodeId::new(start_node), PortId::new(start_port));
            let end = (NodeId::new(end_node), PortId::new(end_port));

            if self.inputs.contains_key(&start) && self.outputs.contains_key(&end) {
                self.add_link(end, start);
                self.restart_node(end.0);
                self.restart_node(start.0);
                devices::invoke(devices::DeviceCommand::TriggerResync);
            } else if self.inputs.contains_key(&end) && self.outputs.contains_key(&start) {
                self.add_link(start, end);
                self.restart_node(end.0);
                self.restart_node(start.0);
                devices::invoke(devices::DeviceCommand::TriggerResync);
            } else {
                tracing::info!(
                    inputs = ?self.inputs,
                    outputs = ?self.outputs,
                    "Attempt to create out-out or in-in link between {:?}, {:?}",
                    start,
                    end,
                );
            };
        }

        for node_to_delete in nodes_to_delete.borrow().iter() {
            tracing::info!("Deleting node {:?}", node_to_delete);
            if let Some(n) = self.nodes.get_mut(node_to_delete) {
                n.stop();
            }

            let links_to_remove = self
                .links
                .iter()
                .filter(|(_, l)| *node_to_delete == l.lhs.0 || *node_to_delete == l.rhs.0)
                .map(|(k, l)| (*k, l.lhs, l.rhs))
                .collect_vec();

            let mut nodes_to_restart = HashSet::new();

            for (link, lhs, rhs) in &links_to_remove {
                self.outputs.get_mut(lhs).unwrap().remove(link);
                self.inputs.get_mut(rhs).unwrap().remove(link);

                if lhs.0 != *node_to_delete {
                    nodes_to_restart.insert(lhs.0);
                } else if rhs.0 != *node_to_delete {
                    nodes_to_restart.insert(rhs.0);
                }
            }

            for node_to_restart in nodes_to_restart {
                self.restart_node(node_to_restart);
            }

            // remove links here so that the buffers stay alive until the node restarts without them
            for (link, _, _) in links_to_remove {
                self.links.remove(&link);
            }

            self.nodes.remove(node_to_delete);
            self.inputs.retain(|(n, _), _| n != node_to_delete);
            self.outputs.retain(|(n, _), _| n != node_to_delete);
        }
    }

    fn add_node(&mut self, id: NodeId, instance: Arc<dyn Perform>) {
        let inst = NodeInstance::new(id, instance);
        for port in inst.instance.inputs().0.read().unwrap().ports.values() {
            self.inputs.insert((inst.id, *port), HashSet::new());
        }

        for port in inst.instance.outputs().0.read().unwrap().ports.values() {
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
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::epi::Frame) {
        let _guard = self.runtime.enter();

        let mut visuals = if self.theme.dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        visuals.window_rounding = egui::Rounding::from(3.0);
        visuals.override_text_color = Some(self.theme.text);
        visuals.widgets.active.bg_fill = self.theme.node_background;
        visuals.widgets.hovered.bg_fill = self.theme.node_background_hovered;
        visuals.widgets.open.bg_fill = self.theme.grid_background;
        visuals.widgets.inactive.bg_fill = self.theme.grid_background;

        ctx.set_visuals(visuals);

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                egui::menu::menu_button(ui, "File", |ui| {
                    if ui.button("Save").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Save")
                            .add_filter("config", &["json"])
                            .set_file_name("config.json")
                            .save_file()
                        {
                            tracing::info!("Saving to {:?}", path);
                            if let Ok(mut file) = std::fs::File::create(path) {
                                let buf = serde_json::to_vec_pretty(&self.save_config()).unwrap();
                                file.write_all(&buf).unwrap();
                            }
                        }
                    }

                    if ui.button("Load").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Load")
                            .add_filter("config", &["json"])
                            .pick_file()
                        {
                            tracing::info!("Restoring from {:?}", path);
                            if let Ok(file) = std::fs::File::open(path) {
                                let cfg: DSPConfig = serde_json::from_reader(file).unwrap();
                                self.restore_config(cfg);
                            }
                        }
                    }
                });

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
                });

                if ui
                    .button("Sync output")
                    .on_hover_text_at_pointer("Flush buffers to get rid of any built up latency")
                    .clicked()
                {
                    devices::invoke(devices::DeviceCommand::TriggerResync);

                    for node in self.nodes.values_mut() {
                        node.stop();
                    }

                    for link in self.links.values() {
                        link.resync();
                    }

                    self.update_all();
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.update_nodes(ui);
        });

        frame.set_window_size(ctx.used_size());
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let cfg = serde_json::to_string(&self.save_config()).unwrap();
        storage.set_string("graph_state", cfg);
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

#[derive(Serialize, Deserialize)]
struct LinkConfig {
    lhs: (NodeId, PortId),
    rhs: (NodeId, PortId),
}

impl LinkInstance {
    fn new(id: LinkId, lhs: (NodeId, PortId), rhs: (NodeId, PortId)) -> Self {
        let (sink, source) = rivulet::circular_buffer::<f32>(8192);
        let source = source.into_view();

        Self {
            id,
            lhs,
            sink: Arc::new(Mutex::new(sink)),
            rhs,
            source: Arc::new(Mutex::new(source)),
        }
    }

    fn save(&self) -> LinkConfig {
        LinkConfig {
            lhs: self.lhs,
            rhs: self.rhs,
        }
    }

    fn resync(&self) {
        use rivulet::View;

        let mut source = self.source.blocking_lock();
        let len = source.view().len();
        source.release(len);
    }
}

struct NodeInstance {
    id: NodeId,
    instance: Arc<dyn Perform>,
    position: egui::Pos2,
    task: Option<(
        tokio::task::JoinHandle<()>,
        tokio::sync::oneshot::Sender<()>,
    )>,
}

#[derive(Serialize, Deserialize)]
struct NodeConfig {
    id: NodeId,
    typename: String,
    position: (f32, f32),
    cfg: serde_json::Value,
}

impl NodeInstance {
    fn new(id: NodeId, instance: Arc<dyn Perform>) -> Self {
        Self {
            id,
            instance,
            position: pos2(100.0, 100.0),
            task: None,
        }
    }

    fn save(&self) -> NodeConfig {
        NodeConfig {
            id: self.id,
            typename: self.instance.cfg_name().to_owned(),
            position: self.position.into(),
            cfg: self.instance.save(),
        }
    }

    fn restore(cfg: NodeConfig) -> Self {
        let (_, restorer) = crate::nodes::RESTORE
            .iter()
            .find(|(n, _)| n == &cfg.typename)
            .unwrap();

        let inst = restorer(cfg.cfg);

        let mut this = Self::new(cfg.id, inst);
        this.position = egui::Pos2::from(cfg.position);
        this
    }

    fn start(
        &mut self,
        mut inputs: Vec<Vec<Arc<Mutex<splittable::View<Source<f32>>>>>>,
        mut outputs: Vec<Vec<Arc<Mutex<Sink<f32>>>>>,
    ) {
        assert!(self.task.is_none());
        let id = self.id;

        let instance = Arc::clone(&self.instance);

        let num_inputs: usize = inputs.iter().map(|v| v.len()).sum();
        let num_outputs: usize = outputs.iter().map(|v| v.len()).sum();

        tracing::debug!(?id, num_inputs, num_outputs, "Starting node");

        if num_inputs == 0 && num_outputs == 0 {
            tracing::debug!(
                ?id,
                "Abandoning node startup, it has no inputs and no outputs"
            );
            // if the node has no inputs or outputs, do nothing
            return;
        }

        let (cancel_in, mut cancel_out) = tokio::sync::oneshot::channel();

        let coro = async move {
            // this is horrible
            // should be fine though since we only do this if the graph is edited

            let mut input_slices_v = Vec::with_capacity(inputs.len());
            for input_port in &mut inputs {
                let mut guards = Vec::with_capacity(input_port.len());

                for input_pipe in input_port {
                    guards.push(Arc::clone(input_pipe).lock_owned().await);
                }

                input_slices_v.push(guards);
            }

            let mut input_slices = input_slices_v
                .iter_mut()
                .map(|input| input.iter_mut().map(|g| g.deref_mut()).collect::<Vec<_>>())
                .collect_vec();

            let mut output_slices_v = Vec::with_capacity(outputs.len());
            for output_port in &mut outputs {
                let mut guards = Vec::new();

                for output_pipe in output_port {
                    guards.push(Arc::clone(output_pipe).lock_owned().await);
                }

                output_slices_v.push(guards);
            }

            let mut output_slices = output_slices_v
                .iter_mut()
                .map(|output| output.iter_mut().map(|g| g.deref_mut()).collect::<Vec<_>>())
                .collect_vec();

            let mut input_slices = input_slices
                .iter_mut()
                .map(|x| x.as_mut_slice())
                .collect_vec();

            let mut output_slices = output_slices
                .iter_mut()
                .map(|x| x.as_mut_slice())
                .collect_vec();

            loop {
                let mut perform = instance.perform(&mut input_slices, &mut output_slices);

                tokio::select! {
                    _ = &mut cancel_out => {
                        return;
                    },
                    _ = &mut perform => {}
                }
            }
        };

        self.task = Some((tokio::spawn(coro), cancel_in));
    }

    fn stop(&mut self) {
        tracing::debug!(id = ?self.id, "Stopping node");
        if let Some((handle, stop)) = self.task.take() {
            let rthandle = Handle::current();
            let _ = stop.send(());
            let _ = rthandle.block_on(handle);
        }
    }

    fn restart(
        &mut self,
        inputs: Vec<Vec<Arc<Mutex<splittable::View<Source<f32>>>>>>,
        outputs: Vec<Vec<Arc<Mutex<Sink<f32>>>>>,
    ) {
        // tracing::debug!(id = ?self.id, "Restarting node");
        self.stop();
        self.start(inputs, outputs)
    }
}
