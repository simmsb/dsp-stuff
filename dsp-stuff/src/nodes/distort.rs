use crate::{ids::NodeId, node::*};
use atomig::Atomic;
use collect_slice::CollectSlice;
use serde::{Deserialize, Serialize};

#[derive(
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    atomig::Atom,
    strum::EnumIter,
    strum::IntoStaticStr,
    Clone,
    Copy,
)]
#[repr(u8)]
enum Mode {
    SoftClip,
    Tanh,
    RecipSoftClip,
    Fuzz,
    Sin,
    Atan,
    Square,
    Chebyshev4,
}

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    input = "in",
    output = "out",
    title = "Distort",
    cfg_name = "distort",
    description = "Distortion effects"
)]
pub struct Distort {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(slider(range = "0.0..=10.0", as_input), save)]
    level: Atomic<f32>,

    #[dsp(select, save, default = "Mode::SoftClip")]
    mode: Atomic<Mode>,
}

fn do_soft_clip(sample: f32, level: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    let sample = sample * level;
    let sample = if sample > 1.0 {
        2.0 / 3.0
    } else if (-1.0..=1.0).contains(&sample) {
        sample - (sample.powi(3) / 3.0)
    } else {
        -2.0 / 3.0
    };

    sample / level
}

fn apply(f: fn(f32, f32) -> f32, input: &[f32], output: &mut [f32], level: &[f32]) {
    input
        .iter()
        .zip(level)
        .map(|(x, level)| f(*x, *level))
        .collect_slice(output);
}

fn do_recip_soft_clip(sample: f32, level: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    sample.signum() * (1.0 - 1.0 / (sample.abs() * level + 1.0))
}

fn do_tanh(sample: f32, level: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    (sample * level).tanh()
}

fn do_sin(sample: f32, level: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    (sample * level).sin()
}

fn do_atan(sample: f32, level: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    (sample * level).atan()
}

fn do_sqr(sample: f32, level: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    (sample * level).powi(2) * (sample * level).signum()
}

fn do_cheb_4(sample: f32, level: f32) -> f32 {
    if level < 0.001 {
        return sample;
    }

    let v = sample * level;

    8.0 * v.powi(4) - 8.0 * v.powi(2) + 1.0
}

fn fuzz(input: &[f32], output: &mut [f32], level: &[f32]) {
    let mx = input
        .iter()
        .map(|x| x.abs())
        .max_by(f32::total_cmp)
        .unwrap();
    let mut z = [0.0; BUF_SIZE];

    input
        .iter()
        .zip(level)
        .map(|(x, level)| {
            let q = x * level / mx;
            (1.0 - q.copysign(-1.0).exp()).copysign(-1.0)
        })
        .collect_slice(&mut z);

    let mz = z.iter().map(|x| x.abs()).max_by(f32::total_cmp).unwrap();

    let mut y = [0.0; BUF_SIZE];

    z.iter().map(|x| x * mx / mz).collect_slice(&mut y);

    let my = y.iter().map(|x| x.abs()).max_by(f32::total_cmp).unwrap();

    y.iter().map(|x| x * mx / my).collect_slice(output);
}

impl SimpleNode for Distort {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: ProcessInput, mut outputs: ProcessOutput) {
        let mut level = [0.0; BUF_SIZE];
        self.level_input(&inputs, &mut level);
        let input = inputs.get("in").unwrap();
        let output = outputs.get("out").unwrap();

        let mode = self.mode.load(std::sync::atomic::Ordering::Relaxed);

        match mode {
            Mode::SoftClip => apply(do_soft_clip, input, output, &level),
            Mode::Tanh => apply(do_tanh, input, output, &level),
            Mode::RecipSoftClip => apply(do_recip_soft_clip, input, output, &level),
            Mode::Fuzz => fuzz(input, output, &level),
            Mode::Sin => apply(do_sin, input, output, &level),
            Mode::Atan => apply(do_atan, input, output, &level),
            Mode::Square => apply(do_sqr, input, output, &level),
            Mode::Chebyshev4 => apply(do_cheb_4, input, output, &level),
        }
    }
}
