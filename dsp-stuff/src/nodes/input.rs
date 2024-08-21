use std::{collections::HashMap, sync::Arc};
use eframe::egui;
use crate::{
    devices,
    ids::{DeviceId, NodeId, PortId},
    node::*,
};
use arc_swap::ArcSwap;
use rivulet::{circular_buffer::Source, splittable, View, ViewMut};
use tokio::sync::Mutex;

pub struct Input {
    id: NodeId,
    inputs: PortStorage,
    outputs: PortStorage,
    source: Arc<Mutex<Option<splittable::View<Source<f32>>>>>,

    cached_hosts: ArcSwap<Vec<cpal::HostId>>,
    selected_host: ArcSwap<cpal::HostId>,
    cached_devices: ArcSwap<Vec<String>>,
    selected_device: ArcSwap<Option<(String, DeviceId)>>,
}

impl Drop for Input {
    fn drop(&mut self) {
        if let Some((_, device)) = self.selected_device.load().as_ref() {
            devices::invoke(devices::DeviceCommand::CloseDevice(*device));
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct InputConfig {
    id: NodeId,
    selected_host: String,
    selected_device: Option<String>,
    outputs: HashMap<String, PortId>,
}

impl Input {
    fn load_device(&self, host: cpal::HostId, name: Option<String>) {
        let mut source = self.source.blocking_lock();

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
            if let Some((id, new_source)) =
                devices::invoke(devices::DeviceCommand::OpenInput(host, dev.clone()))
                    .input_opened()
                    .unwrap()
            {
                self.selected_device.store(Arc::new(Some((dev, id))));
                *source = Some(new_source);
            } else {
                self.selected_device.store(Arc::new(None));
                *source = None;
            }
        } else {
            self.selected_device.store(Arc::new(None));
            *source = None;
        }
    }
}

impl Node for Input {
    fn title(&self) -> &'static str {
        "Input"
    }

    fn cfg_name(&self) -> &'static str {
        "input"
    }

    fn description(&self) -> &'static str {
        "Stream audio from an input device"
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
        let cfg = InputConfig {
            id: self.id,
            selected_host: self.selected_host.load().name().to_owned(),
            selected_device: Option::as_ref(&self.selected_device.load())
                .map(|(n, _)| n.to_owned()),
            outputs: self.outputs.get_all(),
        };

        serde_json::to_value(cfg).unwrap()
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
            let devices = devices::invoke(devices::DeviceCommand::ListInputs(selected_host))
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
}

impl NodeStatic for Input {
    fn new(id: NodeId) -> Self {
        let hosts = devices::invoke(devices::DeviceCommand::ListHosts)
            .hosts()
            .unwrap();
        let selected_host = *hosts.first().expect("There are no audio hosts available");
        let devices = devices::invoke(devices::DeviceCommand::ListInputs(selected_host))
            .devices()
            .unwrap();

        let outputs = PortStorage::default();
        outputs.add("out".to_owned());

        Self {
            id,
            inputs: PortStorage::default(),
            outputs,
            source: Arc::new(Mutex::new(None)),

            cached_hosts: ArcSwap::new(Arc::new(hosts)),
            selected_host: ArcSwap::new(Arc::new(selected_host)),

            cached_devices: ArcSwap::new(Arc::new(devices)),
            selected_device: ArcSwap::new(Arc::new(None)),
        }
    }

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized,
    {
        let cfg: InputConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);

        if let Some(host) = devices::invoke(devices::DeviceCommand::ListHosts)
            .hosts()
            .unwrap()
            .into_iter()
            .find(|x| x.name() == cfg.selected_host)
        {
            this.load_device(host, cfg.selected_device);
        };

        this.outputs = PortStorage::new(cfg.outputs);

        this
    }
}

impl Perform for Input {
    // #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    async fn perform(&self, _inputs: NodeInputs<'_, '_, '_>, outputs: NodeOutputs<'_, '_, '_>) {
        let buf_size = 128;

        let mut source = self.source.lock().await;

        if let Some(source) = source.as_mut() {
            source.grant(buf_size).await.unwrap();

            for output in outputs.iter_mut() {
                for out in output.iter_mut() {
                    out.grant(buf_size).await.unwrap();
                    out.view_mut()[..buf_size].copy_from_slice(&source.view()[..buf_size]);
                }
            }

            // tracing::debug!("Releasing source");
            source.release(buf_size);

            // tracing::debug!("Releasing outputs");
            for output_port in outputs.iter_mut() {
                for output_pipe in output_port.iter_mut() {
                    output_pipe.release(buf_size);
                }
            }
        }
    }
}
