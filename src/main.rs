use std::time::Duration;

use opentelemetry::sdk::export::metrics::ExportKindSelector;
use opentelemetry_otlp::WithExportConfig;
use tracing::metric::Metric;
use tracing::{event, metric, Level, instrument};
use tracing_subscriber::subscribe::CollectExt;
use tracing_subscriber::{fmt, EnvFilter, Registry};

use futures_util::{Stream, StreamExt as _};
use opentelemetry::global;
use opentelemetry::sdk::trace as sdktrace;
use opentelemetry::sdk::metrics::selectors;
use opentelemetry_aws::trace::XrayPropagator;
use tracing_subscriber::{util::SubscriberInitExt, util::TryInitError};

use axum::{routing::get, Router};

mod metric_constants {
    pub(crate) const GET_ROOT_REQUEST_COUNT: &'static str = "GET_ROOT_REQUEST_COUNT";
}

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
        .with_exporter(opentelemetry_otlp::new_exporter().tonic().with_endpoint("http://localhost:4317"))
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
        .with_exporter(opentelemetry_otlp::new_exporter().tonic().with_endpoint("http://localhost:4317"))
        .with_export_kind(ExportKindSelector::Delta)
        .with_aggregator_selector(selectors::simple::Selector::Exact)
        .with_period(Duration::from_secs(10))
        .build()
        .unwrap();

    // Create a tracing layer with the configured tracer
    let opentelemetry = tracing_opentelemetry::subscriber().with_tracer_and_push_controller(tracer, push_controller);

    let filter = EnvFilter::from_default_env().add_directive(Level::TRACE.into());
    let collector = Registry::default()
        .with(filter)
        .with(opentelemetry)
        .with(fmt::Subscriber::default());

    tracing::collect::set_global_default(collector).unwrap();

    event!(tracing::Level::INFO, "BLAH");
    event!(tracing::Level::TRACE, "BLAH2");
    //metric!(Metric::new("METRIC_OUTSIDE_OF_ANY_SPAN", 1 as u64));
    //task_metrics().await;

    let app = Router::new().route("/", get(get_root));

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();

    global::shutdown_tracer_provider();
}

// `&'static str` becomes a `200 OK` with `content-type: text/plain; charset=utf-8`
#[instrument]
async fn get_root() -> &'static str {
    metric!(Metric::new(metric_constants::GET_ROOT_REQUEST_COUNT, 1 as u64));
    "foo"
}

// async fn task_metrics() {
//     // construct a metrics taskmonitor
//     let metrics_monitor = tokio_metrics::TaskMonitor::new();

//     // print task metrics every 500ms
//     {
//         let metrics_monitor = metrics_monitor.clone();
//         tokio::spawn(async move {
//             for interval in metrics_monitor.intervals() {
//                 metric!(Metric::new("INSTRUMENTED_COUNT", interval.instrumented_count));
//                 metric!(Metric::new("DROPPED_COUNT", interval.dropped_count));
//                 metric!(Metric::new("FIRST_POLL_COUNT", interval.first_poll_count));
//                 //metric!(Metric::new("TOTAL_FIRST_POLL_DELAY", interval.total_first_poll_delay));
//                 metric!(Metric::new("TOTAL_IDLED_COUNT", interval.total_idled_count));
//                 //metric!(Metric::new("TOTAL_IDLED_DURATION", interval.total_idle_duration));
//                 metric!(Metric::new("TOTAL_SCHEDULED_COUNT", interval.total_scheduled_count));
//                 //metric!(Metric::new("TOTAL_SCHEDULED_DURATION", interval.total_scheduled_duration));
//                 metric!(Metric::new("TOTAL_POLL_COUNT", interval.total_poll_count));
//                 //metric!(Metric::new("TOTAL_POLL_DURATION", interval.total_poll_duration));
//                 metric!(Metric::new("TOTAL_FAST_POLL_COUNT", interval.total_fast_poll_count));
//                 //metric!(Metric::new("TOTAL_FAST_POLL_DURATION", interval.total_fast_poll_duration));
//                 metric!(Metric::new("TOTAL_SLOW_POLL_COUNT", interval.total_slow_poll_count));
//                 //metric!(Metric::new("TOTAL_SLOW_POLL_DURATION", interval.total_slow_poll_duration));
//                 // wait 500ms
//                 tokio::time::sleep(Duration::from_millis(5000)).await;
//             }
//         });
//     }

//     // instrument some tasks and await them
//     // note that the same taskmonitor can be used for multiple tasks
//     tokio::join![
//         metrics_monitor.instrument(do_work()),
//         metrics_monitor.instrument(do_work()),
//         metrics_monitor.instrument(do_work())
//     ];
// }

// async fn do_work() {
//     for _ in 0..1000 {
//         tokio::task::yield_now().await;
//         tokio::time::sleep(Duration::from_millis(100)).await;
//     }
// }
