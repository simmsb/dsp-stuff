use std::{collections::HashMap, sync::Arc};

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
    inputs: Arc<HashMap<&'static str, PortId>>,
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

impl Node for Input {
    fn title(&self) -> &'static str {
        "Input"
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn inputs(&self) -> Arc<HashMap<&'static str, PortId>> {
        Arc::clone(&self.inputs)
    }

    fn outputs(&self) -> Arc<HashMap<&'static str, PortId>> {
        self.outputs.get_or_create("out");
        self.outputs.all()
    }

    fn render(&self, ui: &mut egui::Ui) -> egui::Response {
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
            let devices = devices::invoke(devices::DeviceCommand::ListInputs(selected_host))
                .devices()
                .unwrap();

            self.cached_devices.store(Arc::new(devices));
        }

        let (current_device, current_device_id) = self
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

        let r = cb.show_ui(ui, |ui| {
            for device in devices.iter() {
                ui.selectable_value(&mut selected_device, Some(device.clone()), device);
            }

            ui.selectable_value(&mut selected_device, None, "<none>");
        });

        if current_device != selected_device {
            let mut source = self.source.blocking_lock();

            if let Some(id) = current_device_id {
                devices::invoke(devices::DeviceCommand::CloseDevice(id));
            }

            if let Some(dev) = selected_device {
                let (id, new_source) = devices::invoke(devices::DeviceCommand::OpenInput(
                    selected_host,
                    dev.clone(),
                ))
                .input_opened()
                .unwrap();

                self.selected_device.store(Arc::new(Some((dev, id))));
                *source = Some(new_source);
            } else {
                self.selected_device.store(Arc::new(None));
                *source = None;
            }
        }

        r.response
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
            inputs: Default::default(),
            outputs: PortStorage::default(),
            source: Arc::new(Mutex::new(None)),

            cached_hosts: ArcSwap::new(Arc::new(hosts)),
            selected_host: ArcSwap::new(Arc::new(selected_host)),

            cached_devices: ArcSwap::new(Arc::new(devices)),
            selected_device: ArcSwap::new(Arc::new(None)),
        };

        this
    }
}

#[async_trait::async_trait]
impl Perform for Input {
    async fn perform(&self, _inputs: NodeInputs<'_, '_, '_>, outputs: NodeOutputs<'_, '_, '_>) {
        let buf_size = 128;

        let mut source = self.source.lock().await;

        if let Some(source) = source.as_mut() {
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
}
