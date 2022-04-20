use std::sync::{Arc, Mutex};

use crate::{ids::NodeId, node::*};
use atomig::Atomic;
use collect_slice::CollectSlice;
use dasp_envelope::{detect::Peak, Detector};
use dasp_peak::FullWave;

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    input = "in",
    output = "out",
    title = "Envelope",
    cfg_name = "envelope",
    description = "Envelope detection"
)]
pub struct Envelope {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(default = "Arc::new(Mutex::new(Detector::peak(0.0, 0.0)))")]
    detector: Arc<Mutex<Detector<f32, Peak<FullWave>>>>,

    #[dsp(slider(range = "0.0..=1000.0"), save, default = "0.0")]
    attack: Atomic<f32>,
    #[dsp(slider(range = "0.0..=1000.0"), save, default = "0.0")]
    release: Atomic<f32>,
}

impl SimpleNode for Envelope {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: ProcessInput, mut outputs: ProcessOutput) {
        let attack = self.attack.load(std::sync::atomic::Ordering::Relaxed);
        let release = self.release.load(std::sync::atomic::Ordering::Relaxed);

        let input = inputs.get("in").unwrap();
        let output = outputs.get("out").unwrap();

        let mut detector = self.detector.lock().unwrap();

        detector.set_attack_frames(attack);
        detector.set_release_frames(release);

        input
            .iter()
            .map(|v| detector.next(*v))
            .collect_slice(output);
    }
}
