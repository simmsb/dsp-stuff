use collect_slice::CollectSlice;
use std::error::Error;

use cpal::traits::StreamTrait;
use rivulet::{View, ViewMut};

mod devices;

async fn do_copy(
    mut in_: impl View<Item = f32>,
    mut out: impl ViewMut<Item = f32>,
) -> Result<(), String> {
    let buf_size = 128usize;

    loop {
        in_.grant(buf_size).await.map_err(|_| "input died")?;
        out.grant(buf_size).await.map_err(|_| "output died")?;

        in_.view()[..buf_size]
            .iter()
            .cloned()
            .map(do_distort)
            .collect_slice(&mut out.view_mut()[..buf_size]);

        in_.release(buf_size);
        out.release(buf_size);
    }
}

fn do_distort(sample: f32) -> f32 {
    let sample = sample * 70.0;
    let sample = if sample > 1.0 {
        2.0 / 3.0
    } else if (-1.0 <= sample) && (sample <= 1.0) {
        sample
    } else {
        -2.0 / 3.0
    };

    sample / 70.0
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (in_stream, input_source) = devices::input_stream();
    let (out_stream, output_source) = devices::output_stream();
    in_stream.play().unwrap();
    out_stream.play().unwrap();

    console_subscriber::init();

    tokio::spawn(do_copy(input_source, output_source)).await??;

    Ok(())
}
