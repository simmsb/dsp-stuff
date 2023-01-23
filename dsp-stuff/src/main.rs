#![allow(clippy::type_complexity)]
#![feature(async_fn_in_trait)]
#![feature(iter_array_chunks)]

use clap::Parser;

mod devices;
mod ids;
mod node;
mod nodes;
mod runtime;
mod theme;

#[derive(Parser)]
pub struct Params {
    /// Start up with a clean state
    #[clap(short, long)]
    clean: bool,
}

//fn install_tracing() -> color_eyre::Result<Box<dyn Any>> {
fn install_tracing() -> color_eyre::Result<()> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    #[cfg(feature = "console")]
    {
        let console_layer = console_subscriber::ConsoleLayer::builder()
            .with_default_env()
            .spawn();

        tracing_subscriber::registry()
            .with(tracing_error::ErrorLayer::default())
            .with(console_layer)
            .init();
    }
    #[cfg(not(feature = "console"))]
    {
        use tracing_subscriber::fmt::format::FmtSpan;
        let fmt_layer = tracing_subscriber::fmt::layer().with_span_events(FmtSpan::CLOSE);
        // .pretty();
        let filter_layer =
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::default()
                    .add_directive("dsp_stuff=info".parse().unwrap())
            });

        // let (flame_layer, guard) =
        // tracing_flame::FlameLayer::with_file("./tracing.folded").unwrap();

        tracing_subscriber::registry()
            .with(filter_layer)
            .with(tracing_error::ErrorLayer::default())
            .with(fmt_layer)
            // .with(flame_layer)
            .init();

        // return Ok(Box::new(guard));
    }

    Ok(())
    //Ok(Box::new(()))
}

fn main() -> color_eyre::Result<()> {
    let params = Params::parse();

    let _guard = install_tracing()?;

    color_eyre::install()?;

    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "DSP Stuff",
        options,
        Box::new(move |cc| Box::new(runtime::UiContext::new(cc, &params))),
    );

    Ok(())
}
