use super::span_url;

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
