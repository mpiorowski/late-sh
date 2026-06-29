#[cfg(feature = "otel")]
mod inner {
    use std::sync::OnceLock;

    use opentelemetry::{KeyValue, global, metrics::Counter};

    fn meter() -> opentelemetry::metrics::Meter {
        global::meter("late-web")
    }

    fn page_views_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_web_page_views_total")
                .with_description("Total rendered late-web page views")
                .build()
        })
    }

    fn now_playing_fetch_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_web_now_playing_fetch_total")
                .with_description("Total late-web now-playing backend fetches by result")
                .build()
        })
    }

    pub fn record_page_view(page: &str, has_token: bool) {
        page_views_total().add(
            1,
            &[
                KeyValue::new("page", page.to_string()),
                KeyValue::new("has_token", has_token),
            ],
        );
    }

    pub fn record_now_playing_fetch(result: &str) {
        now_playing_fetch_total().add(1, &[KeyValue::new("result", result.to_string())]);
    }
}

#[cfg(not(feature = "otel"))]
mod inner {
    pub fn record_page_view(_page: &str, _has_token: bool) {}
    pub fn record_now_playing_fetch(_result: &str) {}
}

pub use inner::*;
