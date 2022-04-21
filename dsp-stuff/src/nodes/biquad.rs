use std::sync::{Arc, Mutex};

use crate::{ids::NodeId, node::*};
use atomig::Atomic;
use biquad::{Biquad as _, DirectForm1};
use collect_slice::CollectSlice;

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    input = "in",
    output = "out",
    title = "Biquad",
    cfg_name = "biquad",
    description = "Generic biquad filter",
    after_settings_change = "BiQuad::regenerate_filter"
)]
pub struct BiQuad {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(slider(range = "-10.0..=10.0"), default = "1.0", save)]
    a0: Atomic<f32>,

    #[dsp(slider(range = "-10.0..=10.0"), default = "-0.24", save)]
    a1: Atomic<f32>,

    #[dsp(slider(range = "-10.0..=10.0"), default = "0.0", save)]
    a2: Atomic<f32>,

    #[dsp(slider(range = "-10.0..=10.0"), default = "0.758", save)]
    b0: Atomic<f32>,

    #[dsp(slider(range = "-10.0..=10.0"), default = "0.0", save)]
    b1: Atomic<f32>,

    #[dsp(slider(range = "-10.0..=10.0"), default = "0.0", save)]
    b2: Atomic<f32>,

    #[dsp(default = "BiQuad::initial_filter()")]
    filter: Arc<Mutex<biquad::DirectForm1<f32>>>,
}

impl BiQuad {
    fn initial_filter() -> Arc<Mutex<biquad::DirectForm1<f32>>> {
        let coeffs = biquad::Coefficients {
            a1: -0.24,
            a2: 0.0,
            b0: 0.758,
            b1: 0.0,
            b2: 0.0,
        };

        let filter = DirectForm1::<f32>::new(coeffs);

        Arc::new(Mutex::new(filter))
    }

    fn regenerate_filter(&self) {
        let a0 = self.a0.load(atomig::Ordering::Relaxed);

        let coeffs = biquad::Coefficients {
            a1: self.a1.load(atomig::Ordering::Relaxed) / a0,
            a2: self.a2.load(atomig::Ordering::Relaxed) / a0,
            b0: self.b0.load(atomig::Ordering::Relaxed) / a0,
            b1: self.b1.load(atomig::Ordering::Relaxed) / a0,
            b2: self.b2.load(atomig::Ordering::Relaxed) / a0,
        };

        let mut filter = self.filter.lock().unwrap();
        filter.reset_state();
        filter.update_coefficients(coeffs);
    }
}

impl SimpleNode for BiQuad {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: ProcessInput, mut outputs: ProcessOutput) {
        let input = inputs.get("in").unwrap();
        let output = outputs.get("out").unwrap();

        let mut filter = self.filter.lock().unwrap();

        input.iter().map(|x| filter.run(*x)).collect_slice(output);
    }
}
