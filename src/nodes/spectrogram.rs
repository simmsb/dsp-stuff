use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;
use audioviz::spectrum::{config::ProcessorConfig, processor::Processor, Frequency};
use egui::{
    emath::RectTransform,
    epaint::{Mesh, Shape},
    lerp, vec2, Color32, Frame, Pos2, Rect, Rgba,
};
use rivulet::View;

pub struct Spectrogram {
    id: NodeId,
    outputs: Arc<HashMap<String, PortId>>,
    inputs: PortStorage,
    buffer: Arc<Mutex<VecDeque<Vec<Frequency>>>>,
    buffer_size: Atomic<usize>,
    fft_size: Atomic<usize>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SpectrogramConfig {
    id: NodeId,
    inputs: HashMap<String, PortId>,
    buffer_size: usize,
    fft_size: usize,
}

impl Node for Spectrogram {
    fn title(&self) -> &'static str {
        "Spectrogram"
    }

    fn cfg_name(&self) -> &'static str {
        "spectrogram"
    }

    fn description(&self) -> &'static str {
        "Inspect the volume of individual frequencies over time"
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
        let cfg = SpectrogramConfig {
            id: self.id,
            inputs: self.inputs.all().as_ref().clone(),
            buffer_size: self.buffer_size.load(atomig::Ordering::Relaxed),
            fft_size: self.fft_size.load(atomig::Ordering::Relaxed),
        };

        serde_json::to_value(cfg).unwrap()
    }

    fn restore(value: serde_json::Value) -> Self
    where
        Self: Sized,
    {
        let cfg: SpectrogramConfig = serde_json::from_value(value).unwrap();

        let mut this = Self::new(cfg.id);
        this.inputs = PortStorage::new(cfg.inputs);
        this.buffer_size
            .store(cfg.buffer_size, atomig::Ordering::Relaxed);
        this.fft_size.store(cfg.fft_size, atomig::Ordering::Relaxed);

        this
    }

    fn render(&self, ui: &mut egui::Ui) -> egui::Response {
        let _r = Frame::dark_canvas(ui.style()).show(ui, |ui| {
            ui.ctx().request_repaint();

            let desired_size = vec2(200.0, 140.0);
            let (_id, rect) = ui.allocate_space(desired_size);

            let to_screen =
                RectTransform::from_to(Rect::from_x_y_ranges(0.0..=1.0, 20_000.0..=40.0), rect);

            let freqs = self.buffer.lock().unwrap();

            if freqs.is_empty() {
                return;
            }

            let low_colour = Rgba::BLUE;
            let high_colour = Rgba::RED;

            let mut mesh = Mesh::default();

            let num_cols = freqs.len();
            let col_width = 1.0 / num_cols as f32;

            for (x, column) in freqs.iter().enumerate() {
                let mut prev_freq = 0.0;
                let mut last_colour = Color32::from(low_colour);
                for freq in column {
                    let colour = Color32::from(lerp(low_colour..=high_colour, freq.volume));

                    let top_left = to_screen * Pos2::new(x as f32 * col_width, freq.freq);
                    let bottom_right = to_screen * Pos2::new((x + 1) as f32 * col_width, prev_freq);

                    let this_rect = Rect::from_two_pos(top_left, bottom_right);

                    mesh.add_colored_rect(this_rect, colour);

                    prev_freq = freq.freq;
                    last_colour = colour;
                }

                let top_left = to_screen * Pos2::new(x as f32 * col_width, 20_000.0);
                let bottom_right = to_screen * Pos2::new((x + 1) as f32 * col_width, prev_freq);

                let this_rect = Rect::from_two_pos(top_left, bottom_right);

                mesh.add_colored_rect(this_rect, last_colour);
            }

            ui.painter().add(Shape::mesh(mesh));
        });

        let _r = ui.horizontal(|ui| {
            ui.label("FFT Size");

            let mut s = self.fft_size.load(atomig::Ordering::Relaxed);

            let r = ui.add(egui::Slider::new(&mut s, 128..=8192));

            if r.changed() {
                self.fft_size.store(s, atomig::Ordering::Relaxed);
            }
        });

        let r = ui.horizontal(|ui| {
            ui.label("Buffer Size");
            let mut s = self.buffer_size.load(atomig::Ordering::Relaxed);

            let r = ui.add(egui::Slider::new(&mut s, 10..=1024));

            if r.changed() {
                self.buffer_size.store(s, atomig::Ordering::Relaxed);
            }
        });

        r.response
    }

    fn new(id: NodeId) -> Self {
        let this = Self {
            id,
            inputs: PortStorage::default(),
            outputs: Default::default(),
            buffer: Arc::new(Mutex::new(VecDeque::with_capacity(10))),
            buffer_size: Atomic::new(250),
            fft_size: Atomic::new(512),
        };

        this
    }
}

#[async_trait::async_trait]
impl Perform for Spectrogram {
    async fn perform(&self, inputs: NodeInputs<'_, '_, '_>, _outputs: NodeOutputs<'_, '_, '_>) {
        let buf_size = self.fft_size.load(atomig::Ordering::Relaxed);
        let collected_inputs = inputs.get_mut(&self.inputs.get("in").unwrap()).unwrap();
        let merged = collect_and_average(buf_size, collected_inputs).await;

        let mut processor = Processor::from_raw_data(
            ProcessorConfig {
                sample_rate: 48000,
                frequency_bounds: [40, 20_000],
                resolution: Some(100),
                volume: 1.0,
                volume_normalisation: audioviz::spectrum::config::VolumeNormalisation::Mixture,
                position_normalisation: audioviz::spectrum::config::PositionNormalisation::Harmonic,
                manual_position_distribution: None,
                interpolation: audioviz::spectrum::config::Interpolation::Cubic,
            },
            merged,
        );

        processor.compute_all();

        {
            let mut queue = self.buffer.lock().unwrap();
            queue.push_back(processor.freq_buffer);

            let target_len = self.buffer_size.load(atomig::Ordering::Relaxed);

            while queue.len() > target_len {
                queue.pop_front();
            }
        }

        for input in inputs.values_mut() {
            for in_ in input.iter_mut() {
                in_.release(buf_size);
            }
        }
    }
}
