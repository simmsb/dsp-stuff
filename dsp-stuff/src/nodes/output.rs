use std::{collections::HashMap, sync::Arc};

use crate::{
    devices,
    ids::{DeviceId, NodeId, PortId},
    node::*,
};
use arc_swap::ArcSwap;
use rivulet::{circular_buffer::Sink, View, ViewMut};
use tokio::sync::Mutex;

pub struct Output {
    id: NodeId,
    inputs: PortStorage,
    outputs: PortStorage,
    sink: Arc<Mutex<Option<Sink<f32>>>>,

    cached_hosts: ArcSwap<Vec<cpal::HostId>>,
    selected_host: ArcSwap<cpal::HostId>,
    cached_devices: ArcSwap<Vec<String>>,
    selected_device: ArcSwap<Option<(String, DeviceId)>>,
}

impl Drop for Output {
    fn drop(&mut self) {
        if let Some((_, device)) = self.selected_device.load().as_ref() {
            devices::invoke(devices::DeviceCommand::CloseDevice(*device));
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct OutputConfig {
    id: NodeId,
    selected_host: String,
    selected_device: Option<String>,
    inputs: HashMap<String, PortId>,
}

impl Output {
    fn load_device(&self, host: cpal::HostId, name: Option<String>) {
        let mut sink = self.sink.blocking_lock();

        let (_current_device, current_device_id) = self
            .selected_device
            .load()
            .as_ref()
            .clone()
            .map_or((None, None), |(dev, id)| (Some(dev), Some(id)));

        if let Some(id) = current_device_id {
            devices::invoke(devices::DeviceCommand::CloseDevice(id));
        }

        if let Some(dev) = name {
            if let Some((id, new_sink)) =
                devices::invoke(devices::DeviceCommand::OpenOutput(host, dev.clone()))
                    .output_opened()
                    .unwrap()
            {
                self.selected_device.store(Arc::new(Some((dev, id))));
                *sink = Some(new_sink);
            } else {
                self.selected_device.store(Arc::new(None));
                *sink = None;
            }
        } else {
            self.selected_device.store(Arc::new(None));
            *sink = None;
        }

        devices::invoke(devices::DeviceCommand::TriggerResync);
    }
}

impl Node for Output {
    fn title(&self) -> &'static str {
        "Output"
    }

    fn cfg_name(&self) -> &'static str {
        "output"
    }

    fn description(&self) -> &'static str {
        "Stream audio to an output device"
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn inputs(&self) -> &PortStorage {
        &self.inputs
    }

    fn outputs(&self) -> &PortStorage {
        &self.outputs
    }

    fn save(&self) -> serde_json::Value {
        let cfg = OutputConfig {
            id: self.id,
            selected_host: self.selected_host.load().name().to_owned(),
            selected_device: Option::as_ref(&self.selected_device.load())
                .map(|(n, _)| n.to_owned()),
            inputs: self.inputs.get_all(),
        };

        serde_json::to_value(cfg).unwrap()
    }

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized,
    {
        let cfg: OutputConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);

        if let Some(host) = devices::invoke(devices::DeviceCommand::ListHosts)
            .hosts()
            .unwrap()
            .into_iter()
            .find(|x| x.name() == cfg.selected_host)
        {
            this.load_device(host, cfg.selected_device);
        };

        this.inputs = PortStorage::new(cfg.inputs);

        this
    }

    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn render(&self, ui: &mut egui::Ui) {
        let current_host = **self.selected_host.load();
        let mut selected_host = current_host;

        egui::ComboBox::new(("host", self.id), "Audio host")
            .selected_text(selected_host.name())
            .show_ui(ui, |ui| {
                for host in self.cached_hosts.load().iter() {
                    ui.selectable_value(&mut selected_host, *host, host.name());
                }
            });

        if current_host != selected_host {
            self.selected_host.store(Arc::new(selected_host));
            let devices = devices::invoke(devices::DeviceCommand::ListOutputs(selected_host))
                .devices()
                .unwrap();

            self.cached_devices.store(Arc::new(devices));
        }

        let (current_device, _current_device_id) = self
            .selected_device
            .load()
            .as_ref()
            .clone()
            .map_or((None, None), |(dev, id)| (Some(dev), Some(id)));
        let mut selected_device = current_device.clone();

        let mut cb = egui::ComboBox::new(("device", self.id), "Device");

        if let Some(d) = &current_device {
            cb = cb.selected_text(d);
        }

        let devices = self.cached_devices.load();

        cb.show_ui(ui, |ui| {
            for device in devices.iter() {
                ui.selectable_value(&mut selected_device, Some(device.clone()), device);
            }

            ui.selectable_value(&mut selected_device, None, "<none>");
        });

        if current_device != selected_device {
            self.load_device(selected_host, selected_device);
        }
    }

    fn new(id: NodeId) -> Self {
        let hosts = devices::invoke(devices::DeviceCommand::ListHosts)
            .hosts()
            .unwrap();
        let selected_host = *hosts.first().expect("There are no audio hosts available");
        let devices = devices::invoke(devices::DeviceCommand::ListOutputs(selected_host))
            .devices()
            .unwrap();

        let inputs = PortStorage::default();
        inputs.add("in".to_owned());

        Self {
            id,
            inputs,
            outputs: Default::default(),
            sink: Arc::new(Mutex::new(None)),

            cached_hosts: ArcSwap::new(Arc::new(hosts)),
            selected_host: ArcSwap::new(Arc::new(selected_host)),

            cached_devices: ArcSwap::new(Arc::new(devices)),
            selected_device: ArcSwap::new(Arc::new(None)),
        }
    }
}

#[async_trait::async_trait]
impl Perform for Output {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    async fn perform(&self, inputs: NodeInputs<'_, '_, '_>, _outputs: NodeOutputs<'_, '_, '_>) {
        const BUF_SIZE: usize = 128;
        let mut buf = [0.0; BUF_SIZE];

        let collected_inputs = &mut inputs[self.inputs.get_idx("in").unwrap()];

        collect_and_average(&mut buf, collected_inputs).await;

        let mut sink = self.sink.lock().await;

        if let Some(sink) = sink.as_mut() {
            // tracing::debug!(merged = merged.len(), "Done a collection");

            sink.grant(BUF_SIZE).await.unwrap();

            // tracing::debug!(sink_view = sink.view_mut().len(), "Got a grant");

            sink.view_mut()[..BUF_SIZE].copy_from_slice(&buf);

            // tracing::debug!("Releasing sink");
            sink.release(BUF_SIZE);

            // tracing::debug!("Releasing inputs");
            for input_port in inputs.iter_mut() {
                for input_pipe in input_port.iter_mut() {
                    if input_pipe.view().len() < BUF_SIZE {
                        continue;
                    }
                    input_pipe.release(BUF_SIZE);
                }
            }
        }
    }
}
