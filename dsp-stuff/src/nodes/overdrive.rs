use crate::{ids::NodeId, node::*};
use atomig::Atomic;
use collect_slice::CollectSlice;

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    input = "in",
    output = "out",
    title = "Overdrive",
    cfg_name = "overdrive",
    description = "Overdrive"
)]
pub struct Overdrive {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(slider(range = "0.0..=30.0", as_input), save)]
    boost: Atomic<f32>,

    #[dsp(slider(range = "0.0..=1.0", as_input), save)]
    drive: Atomic<f32>,

    #[dsp(slider(range = "0.0..=1.0", as_input), save)]
    level: Atomic<f32>,
}

fn do_overdrive(sample: f32, boost: f32, level: f32, drive: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    let a = sample * boost;
    let b = std::f32::consts::FRAC_PI_4 * a;
    let c = b.atan();
    let d = std::f32::consts::FRAC_2_PI * c;
    let mix = drive * d + (1.0 - drive) * sample;

    mix * level
}

fn apply(f: fn(f32, f32, f32, f32) -> f32, input: &[f32], output: &mut [f32],
         boost: &[f32],
         level: &[f32],
         drive: &[f32],
) {
    itertools::izip!(input, boost, level, drive)
        .map(|(x, boost, level, drive)| f(*x, *boost, *level, *drive))
        .collect_slice(output);
}

impl SimpleNode for Overdrive {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: ProcessInput, mut outputs: ProcessOutput) {
        let mut boost = [0.0; BUF_SIZE];
        let mut level = [0.0; BUF_SIZE];
        let mut drive = [0.0; BUF_SIZE];
        self.boost_input(&inputs, &mut boost);
        self.level_input(&inputs, &mut level);
        self.drive_input(&inputs, &mut drive);

        let input = inputs.get("in").unwrap();
        let output = outputs.get("out").unwrap();

        apply(do_overdrive, input, output, &boost, &level, &drive);
    }
}
