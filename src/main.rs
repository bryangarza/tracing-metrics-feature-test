use std::time::Duration;

use opentelemetry::sdk::export::metrics::ExportKindSelector;
use opentelemetry_otlp::WithExportConfig;
use tracing::{info, instrument, Level, trace};
use tracing_subscriber::subscribe::CollectExt;
use tracing_subscriber::{fmt, EnvFilter, Registry};

use futures_util::{Stream, StreamExt as _};
use opentelemetry::global;
use opentelemetry::sdk::metrics::selectors;
use opentelemetry::sdk::trace as sdktrace;
use opentelemetry_aws::trace::XrayPropagator;
use tracing_subscriber::{util::SubscriberInitExt, util::TryInitError};

use axum::{routing::get, Router};

// From opentelemetry::sdk::util::
/// Helper which wraps `tokio::time::interval` and makes it return a stream
pub fn tokio_interval_stream(
    period: std::time::Duration,
) -> tokio_stream::wrappers::IntervalStream {
    tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(period))
}

// https://github.com/open-telemetry/opentelemetry-rust/blob/2585d109bf90d53d57c91e19c758dca8c36f5512/examples/basic-otlp/src/main.rs#L34-L37
// Skip first immediate tick from tokio, not needed for async_std.
fn delayed_interval(duration: Duration) -> impl Stream<Item = tokio::time::Instant> {
    tokio_interval_stream(duration).skip(1)
}

#[tokio::main]
async fn main() {
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint("http://localhost:4317"),
        )
        .with_trace_config(
            sdktrace::config()
                .with_sampler(sdktrace::Sampler::AlwaysOn)
                // Needed in order to convert the trace IDs into an Xray-compatible format
                .with_id_generator(sdktrace::XrayIdGenerator::default()),
        )
        .install_simple()
        .expect("Unable to initialize OtlpPipeline");

    let push_controller = opentelemetry_otlp::new_pipeline()
        .metrics(tokio::spawn, delayed_interval)
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint("http://localhost:4317"),
        )
        .with_export_kind(ExportKindSelector::Delta)
        .with_aggregator_selector(selectors::simple::Selector::Exact)
        .with_period(Duration::from_secs(10))
        .build()
        .unwrap();

    // Create a tracing layer with the configured tracer
    let opentelemetry = tracing_opentelemetry::subscriber()
        .with_tracer_and_push_controller(tracer, push_controller);

    let filter = EnvFilter::from_default_env().add_directive(Level::TRACE.into());
    let collector = Registry::default()
        .with(filter)
        .with(opentelemetry)
        .with(fmt::Subscriber::default());

    tracing::collect::set_global_default(collector).unwrap();

    info!("BLAH");
    trace!("BLAH2");
    //task_metrics().await;

    let app = Router::new().route("/", get(get_root));

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();

    global::shutdown_tracer_provider();
}

#[instrument]
async fn get_root() -> &'static str {
    info!(
        MONOTONIC_COUNTER_GET_ROOT_REQUEST_COUNT = 1 as u64
    );
    "foo"
}