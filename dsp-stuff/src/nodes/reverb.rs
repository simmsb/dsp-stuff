use std::{collections::HashMap, sync::Arc};

use crate::{
    ids::{NodeId, PortId},
    node::*,
};
use atomig::Atomic;
use collect_slice::CollectSlice;
use rivulet::{
    circular_buffer::{Sink, Source},
    splittable, SplittableView, View, ViewMut,
};
use std::sync::Mutex;

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    input = "in",
    output = "out",
    title = "Reverb",
    cfg_name = "reverb",
    description = "Repeat/ echo sounds with a given delay and decay factor",
    after_settings_change = "Reverb::refresh_seconds"
)]
pub struct Reverb {
    #[dsp(id)]
    id: NodeId,
    #[dsp(inputs)]
    inputs: PortStorage,
    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(slider(range = "0.0..=1.0", suffix = "s"), label = "Delay", save, default = "0.5")]
    seconds: Atomic<f32>,
    #[dsp(slider(range = "0.0..=1.0"), save, default = "0.5")]
    decay: Atomic<f32>,

    #[dsp(default = "make_buffer()")]
    buffer: Arc<Mutex<(splittable::View<Source<f32>>, Sink<f32>)>>,
}

fn make_buffer() -> Arc<Mutex<(splittable::View<Source<f32>>, Sink<f32>)>> {
    let (mut sink, source) = rivulet::circular_buffer::<f32>(128);
    let source = source.into_view();
    let _ = sink.try_grant(128);
    sink.view_mut().fill(0.0);
    sink.release(sink.view().len());

    Arc::new(Mutex::new((source, sink)))
}

impl Reverb {
    fn refresh_seconds(&self) {
        let seconds = self.seconds.load(std::sync::atomic::Ordering::Relaxed);

        let num_samples = ((seconds * 48000.0) as usize).max(128);

        let (mut new_sink, new_source) = rivulet::circular_buffer::<f32>(num_samples);
        let new_source = new_source.into_view();

        let mut guard = self.buffer.lock().unwrap();

        let _ = new_sink.try_grant(num_samples);
        new_sink.view_mut().fill(0.0);
        let num_zeros = new_sink.view().len();
        new_sink.release(num_zeros);

        *guard = (new_source, new_sink);
    }
}

impl SimpleNode for Reverb {
    #[tracing::instrument(level = "TRACE", skip_all, fields(node_id = self.id.get()))]
    fn process(&self, inputs: &HashMap<PortId, &[f32]>, outputs: &mut HashMap<PortId, &mut [f32]>) {
        let input_id = self.inputs.get("in").unwrap();
        let input = inputs.get(&input_id).unwrap();
        let output_id = self.outputs.get("out").unwrap();
        let output = outputs.get_mut(&output_id).unwrap();

        let mut guard = self.buffer.lock().unwrap();

        let decay = self.decay.load(std::sync::atomic::Ordering::Relaxed);

        if guard.0.try_grant(input.len()).unwrap_or(false) {
            let view = &guard.0.view()[..input.len()];

            input
                .iter()
                .zip(view.iter())
                .map(|(a, b)| a + b * decay)
                .collect_slice(*output);

            guard.0.release(input.len());
        } else {
            tracing::trace!("Reverb buffer is empty");
            output.copy_from_slice(input);
        }

        if guard.1.try_grant(input.len()).unwrap_or(false) {
            let view = &mut guard.1.view_mut()[..input.len()];

            view.copy_from_slice(output);
            guard.1.release(input.len());

            // for v in output.iter_mut() {
            //     *v = (*v + 0.5).cos();
            // }
        } else {
            tracing::trace!("Not copying frame into reverb buffer");
        }
    }
}
