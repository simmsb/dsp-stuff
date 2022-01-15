mod devices;
mod ids;
mod node;
mod nodes;
mod runtime;
mod theme;

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

        tracing_subscriber::registry()
            .with(filter_layer)
            .with(tracing_error::ErrorLayer::default())
            .with(fmt_layer)
            .init();
    }

    Ok(())
}

fn main() -> color_eyre::eyre::Result<()> {
    install_tracing()?;

    color_eyre::install()?;

    let app = runtime::UiContext::new();

    eframe::run_native(Box::new(app), eframe::NativeOptions::default());
}
