use tracing::Span;
use tracing_subscriber::{EnvFilter, Registry, layer::SubscriberExt, util::SubscriberInitExt};

// ---------------------------------------------------------------------------
// init_telemetry
// ---------------------------------------------------------------------------

#[cfg(feature = "otel")]
mod init {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    use anyhow::anyhow;
    use opentelemetry::global;
    use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter};
    use opentelemetry_sdk::{
        Resource,
        logs::SdkLoggerProvider,
        metrics::{PeriodicReader, SdkMeterProvider},
        propagation::TraceContextPropagator,
        trace::{Sampler, SdkTracerProvider},
    };

    #[must_use]
    pub struct TelemetryGuard {
        tracer_provider: SdkTracerProvider,
        logger_provider: SdkLoggerProvider,
        meter_provider: SdkMeterProvider,
        shutdown: AtomicBool,
    }

    impl TelemetryGuard {
        pub fn shutdown(&self) -> anyhow::Result<()> {
            if self.shutdown.swap(true, Ordering::SeqCst) {
                return Ok(());
            }

            let mut errors = Vec::new();

            if let Err(err) = self.tracer_provider.shutdown() {
                errors.push(format!("trace shutdown failed: {err}"));
            }
            if let Err(err) = self.logger_provider.shutdown() {
                errors.push(format!("log shutdown failed: {err}"));
            }
            if let Err(err) = self.meter_provider.shutdown() {
                errors.push(format!("metric shutdown failed: {err}"));
            }

            if errors.is_empty() {
                Ok(())
            } else {
                Err(anyhow!(errors.join("; ")))
            }
        }
    }

    impl Drop for TelemetryGuard {
        fn drop(&mut self) {
            if let Err(err) = self.shutdown() {
                eprintln!("telemetry shutdown failed: {err}");
            }
        }
    }

    pub fn init_telemetry(service_name: &str) -> anyhow::Result<Option<TelemetryGuard>> {
        let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_default();
        let enabled = !otlp_endpoint.is_empty();

        if !enabled {
            Registry::default()
                .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_file(true)
                        .with_line_number(true),
                )
                .try_init()?;
            return Ok(None);
        }

        global::set_text_map_propagator(TraceContextPropagator::new());

        let resource = Resource::builder()
            .with_service_name(service_name.to_string())
            .build();

        // 1. Tracer
        let span_exporter = SpanExporter::builder().with_tonic().build()?;
        let tracer_provider = SdkTracerProvider::builder()
            .with_batch_exporter(span_exporter)
            .with_sampler(Sampler::AlwaysOn)
            .with_resource(resource.clone())
            .build();
        global::set_tracer_provider(tracer_provider.clone());
        let tracer =
            opentelemetry::trace::TracerProvider::tracer(&tracer_provider, service_name.to_owned());

        // 2. Logger
        let log_exporter = LogExporter::builder().with_tonic().build()?;
        let logger_provider = SdkLoggerProvider::builder()
            .with_batch_exporter(log_exporter)
            .with_resource(resource.clone())
            .build();
        let log_layer = opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(
            &logger_provider,
        );

        // 3. Metrics
        let metric_exporter = MetricExporter::builder().with_tonic().build()?;
        let meter_provider = SdkMeterProvider::builder()
            .with_reader(PeriodicReader::builder(metric_exporter).build())
            .with_resource(resource)
            .build();
        global::set_meter_provider(meter_provider.clone());

        // Combine and set global subscriber
        Registry::default()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_file(true)
                    .with_line_number(true),
            )
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .with(log_layer)
            .try_init()?;

        Ok(Some(TelemetryGuard {
            tracer_provider,
            logger_provider,
            meter_provider,
            shutdown: AtomicBool::new(false),
        }))
    }
}

#[cfg(not(feature = "otel"))]
mod init {
    use super::*;

    #[must_use]
    pub struct TelemetryGuard;

    pub fn init_telemetry(_service_name: &str) -> anyhow::Result<Option<TelemetryGuard>> {
        Registry::default()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_file(true)
                    .with_line_number(true),
            )
            .try_init()?;
        Ok(None)
    }
}

pub use init::{TelemetryGuard, init_telemetry};

// ---------------------------------------------------------------------------
// HTTP telemetry middleware (axum)
// ---------------------------------------------------------------------------

use axum::{
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use reqwest::{RequestBuilder, Response as ReqwestResponse};
use tracing::{Instrument, field};

pub async fn http_telemetry_middleware(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .unwrap_or(path.as_str())
        .to_owned();

    #[cfg(feature = "otel")]
    let parent_cx = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&opentelemetry_http::HeaderExtractor(req.headers()))
    });

    let span = tracing::info_span!(
        "http.server.request",
        "otel.name" = field::Empty,
        "otel.kind" = "server",
        "otel.status_code" = field::Empty,
        "http.request.method" = %method,
        "url.path" = %path,
        "http.route" = %route,
        "http.response.status_code" = field::Empty,
        "error.type" = field::Empty,
    );
    span.record("otel.name", field::display(format!("{method} {route}")));

    #[cfg(feature = "otel")]
    {
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        let _ = span.set_parent(parent_cx);
    }

    let response = next.run(req).instrument(span.clone()).await;
    record_response_status(&span, response.status().as_u16());
    response
}

// ---------------------------------------------------------------------------
// Span helpers (always available — pure tracing, no OT deps)
// ---------------------------------------------------------------------------

fn record_response_status(span: &Span, status_code: u16) {
    span.record("http.response.status_code", field::display(status_code));

    if status_code >= 500 {
        record_error(span, status_code);
    }
}

fn record_error(span: &Span, error_type: impl std::fmt::Display) {
    span.record("otel.status_code", "ERROR");
    span.record("error.type", field::display(error_type));
}

pub fn mark_span_error(span: &Span, error_type: impl std::fmt::Display) {
    record_error(span, error_type);
}

pub fn mark_current_span_error(error_type: impl std::fmt::Display) {
    record_error(&Span::current(), error_type);
}

#[macro_export]
macro_rules! error_span {
    ($error_type:expr, $($arg:tt)*) => {{
        $crate::telemetry::mark_current_span_error($error_type);
        tracing::error!(error.type = $error_type, $($arg)*);
    }};
}

// ---------------------------------------------------------------------------
// TracedExt — HTTP client with span propagation
// ---------------------------------------------------------------------------

pub trait TracedExt {
    fn send_traced(
        self,
    ) -> impl std::future::Future<Output = reqwest::Result<ReqwestResponse>> + Send;
}

impl TracedExt for RequestBuilder {
    async fn send_traced(self) -> reqwest::Result<ReqwestResponse> {
        let (client, request) = self.build_split();
        #[cfg(feature = "otel")]
        let mut request = request?;
        #[cfg(not(feature = "otel"))]
        let request = request?;
        let method = request.method().clone();
        let url = request.url().clone();
        let server_address = url.host_str().unwrap_or_default().to_owned();
        let span = tracing::info_span!(
            "http.client.request",
            "otel.name" = field::display(format!("{method} {url}")),
            "otel.kind" = "client",
            "otel.status_code" = field::Empty,
            "http.request.method" = %method,
            "http.response.status_code" = field::Empty,
            "url.full" = %url,
            "server.address" = %server_address,
            "error.type" = field::Empty,
        );

        #[cfg(feature = "otel")]
        {
            use tracing_opentelemetry::OpenTelemetrySpanExt;
            let cx = span.context();
            opentelemetry::global::get_text_map_propagator(|propagator| {
                propagator.inject_context(
                    &cx,
                    &mut opentelemetry_http::HeaderInjector(request.headers_mut()),
                );
            });
        }

        let response = async { client.execute(request).await }
            .instrument(span.clone())
            .await;

        match response {
            Ok(response) => {
                record_response_status(&span, response.status().as_u16());
                response.error_for_status()
            }
            Err(err) => {
                record_error(&span, "transport");
                Err(err)
            }
        }
    }
}
