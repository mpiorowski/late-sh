use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use tracing::{Event, Subscriber};
use tracing_log::log;
use tracing_subscriber::{
    EnvFilter, Layer, Registry,
    layer::{Context, SubscriberExt},
};

use super::{quiet_noisy_targets, span_url};

struct CountEvents(Arc<AtomicUsize>);

impl<S: Subscriber> Layer<S> for CountEvents {
    fn on_event(&self, _: &Event<'_>, _: Context<'_, S>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

/// Goes through the `log` bridge because that is how russh arrives: it logs
/// with the `log` crate, not `tracing`.
fn reaches_the_logs(target: &str, level: log::Level) -> bool {
    let events = Arc::new(AtomicUsize::new(0));
    let subscriber = Registry::default()
        .with(quiet_noisy_targets(EnvFilter::new("info")))
        .with(CountEvents(events.clone()));
    tracing::subscriber::with_default(subscriber, || {
        let record = log::Record::builder()
            .target(target)
            .level(level)
            .args(format_args!("unknown channel request"))
            .build();
        tracing_log::format_trace(&record).expect("dispatch the record");
    });
    events.load(Ordering::Relaxed) == 1
}

#[test]
fn only_russhs_channel_chatter_is_silenced() {
    assert!(!reaches_the_logs(
        "russh::server::encrypted",
        log::Level::Warn
    ));
    assert!(reaches_the_logs(
        "russh::server::encrypted",
        log::Level::Error
    ));
    assert!(reaches_the_logs("russh::server::session", log::Level::Warn));
    assert!(reaches_the_logs("late_ssh::ssh", log::Level::Info));
}

/// Outbound spans land in VictoriaTraces, readable by anyone with Grafana.
/// Google APIs take their key as `?key=`, so the query string must never
/// reach the span name or `url.full`.
#[test]
fn span_url_drops_the_query_so_api_keys_never_reach_traces() {
    let url = reqwest::Url::parse(
        "https://generativelanguage.googleapis.com/v1beta/models/m:generateContent?key=SECRET#frag",
    )
    .unwrap();

    assert_eq!(
        span_url(&url),
        "https://generativelanguage.googleapis.com/v1beta/models/m:generateContent"
    );
}
