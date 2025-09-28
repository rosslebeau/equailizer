use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::{fmt, layer::*, util::SubscriberInitExt};

pub fn init_tracing() -> Result<WorkerGuard, Box<dyn std::error::Error>> {
    let base_path = crate::persist::base_path()?;
    let file_appender = tracing_appender::rolling::daily(base_path, "eq.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let stdout_layer = fmt::layer()
        .with_target(false)
        .with_ansi(false)
        .with_writer(std::io::stdout)
        .with_filter(LevelFilter::INFO);

    let file_layer = fmt::layer()
        .with_target(false)
        .with_ansi(false)
        .with_writer(file_writer)
        .with_filter(
            Targets::new()
                .with_default(LevelFilter::INFO)
                .with_target("equailizer", LevelFilter::DEBUG),
        );

    tracing_subscriber::registry()
        .with(stdout_layer)
        .with(file_layer)
        .init();

    return Ok(guard);
}
