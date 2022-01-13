mod devices;
mod ids;
mod node;
mod nodes;
mod runtime;

// async fn do_copy(
//     mut in_: impl View<Item = f32>,
//     mut out: impl ViewMut<Item = f32>,
// ) -> Result<(), String> {
//     let buf_size = 128usize;

//     loop {
//         in_.grant(buf_size).await.map_err(|_| "input died")?;
//         out.grant(buf_size).await.map_err(|_| "output died")?;

//         in_.view()[..buf_size]
//             .iter()
//             .cloned()
//             .map(do_distort)
//             .collect_slice(&mut out.view_mut()[..buf_size]);

//         in_.release(buf_size);
//         out.release(buf_size);
//     }
// }

// fn do_distort(sample: f32) -> f32 {
//     let sample = sample * 70.0;
//     let sample = if sample > 1.0 {
//         2.0 / 3.0
//     } else if (-1.0 <= sample) && (sample <= 1.0) {
//         sample - (sample.powi(3) / 3.0)
//     } else {
//         -2.0 / 3.0
//     };

//     sample / 70.0
// }

fn install_tracing() -> color_eyre::Result<()> {
    use tracing_subscriber::fmt::format::FmtSpan;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let fmt_layer = tracing_subscriber::fmt::layer().with_span_events(FmtSpan::CLOSE);
    // .pretty();
    let filter_layer = tracing_subscriber::EnvFilter::from_default_env();

    tracing_subscriber::registry()
        .with(tracing_error::ErrorLayer::default())
        .with(filter_layer)
        .with(fmt_layer)
        .init();

    Ok(())
}

//#[tokio::main]
fn main() -> color_eyre::eyre::Result<()> {
    // let (in_stream, input_source) = devices::input_stream();
    // let (out_stream, output_source) = devices::output_stream();
    // in_stream.play().unwrap();
    // out_stream.play().unwrap();

    install_tracing()?;

    color_eyre::install()?;
    // console_subscriber::init();

    let app = runtime::UiContext::new();

    eframe::run_native(Box::new(app), eframe::NativeOptions::default());

    // tokio::spawn(do_copy(input_source, output_source)).await??;
}
