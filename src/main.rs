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
        out.view_mut()[..buf_size].copy_from_slice(&in_.view()[..buf_size]);
        in_.release(buf_size);
        out.release(buf_size);
    }
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
