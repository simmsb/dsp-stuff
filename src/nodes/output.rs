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
    outputs: Arc<HashMap<String, PortId>>,
    inputs: PortStorage,
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

    fn inputs(&self) -> Arc<HashMap<String, PortId>> {
        self.inputs.ensure_name("in");
        self.inputs.all()
    }

    fn outputs(&self) -> Arc<HashMap<String, PortId>> {
        Arc::clone(&self.outputs)
    }

    fn save(&self) -> serde_json::Value {
        let cfg = OutputConfig {
            id: self.id,
            selected_host: self.selected_host.load().name().to_owned(),
            selected_device: Option::as_ref(&self.selected_device.load())
                .map(|(n, _)| n.to_owned()),
            inputs: self.inputs.all().as_ref().clone(),
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
            .find(|x| x.name() == &cfg.selected_host)
        {
            this.load_device(host, cfg.selected_device);
        };

        this.inputs = PortStorage::new(cfg.inputs);

        this
    }

    fn render(&self, ui: &mut egui::Ui) {
        let current_host = **self.selected_host.load();
        let mut selected_host = current_host;

        egui::ComboBox::from_id_source(("host", self.id))
            .with_label("Audio host")
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

        let mut cb = egui::ComboBox::from_id_source(("device", self.id)).with_label("Device");

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
        let devices = devices::invoke(devices::DeviceCommand::ListInputs(selected_host))
            .devices()
            .unwrap();

        let this = Self {
            id,
            inputs: PortStorage::default(),
            outputs: Default::default(),
            sink: Arc::new(Mutex::new(None)),

            cached_hosts: ArcSwap::new(Arc::new(hosts)),
            selected_host: ArcSwap::new(Arc::new(selected_host)),

            cached_devices: ArcSwap::new(Arc::new(devices)),
            selected_device: ArcSwap::new(Arc::new(None)),
        };

        this
    }
}

#[async_trait::async_trait]
impl Perform for Output {
    async fn perform(&self, inputs: NodeInputs<'_, '_, '_>, _outputs: NodeOutputs<'_, '_, '_>) {
        let buf_size = 128;

        let collected_inputs = inputs.get_mut(&self.inputs.get("in").unwrap()).unwrap();

        let merged = collect_and_average(buf_size, collected_inputs).await;

        let mut sink = self.sink.lock().await;

        if let Some(sink) = sink.as_mut() {
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
}
