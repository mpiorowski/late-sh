use askama::Template;
use axum::{
    Router,
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
};
use serde::Serialize;

use crate::{AppState, error::AppError, metrics, pages::shared::now_playing};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(handler))
        .route("/now-playing", get(now_playing_handler))
        .route("/status", get(status_handler))
}

impl Page {
    fn active_page(&self) -> &str {
        "dashboard"
    }
}

#[derive(Serialize)]
struct DashboardData {
    username: String,
    viewer_count: i32,
    is_live: bool,
}

#[derive(Template)]
#[template(path = "pages/dashboard/page.html")]
struct Page {
    cpu_usage: u8,
    mem_usage: u8,
    timestamp: String,
    // Initial fields for the dashboard data
    username: String,
    viewer_count: i32,
    is_live: bool,
}

#[derive(Template)]
#[template(path = "pages/dashboard/partial_now_playing.html")]
struct NowPlayingPartial {
    username: String,
    viewer_count: i32,
    is_live: bool,
}

#[derive(Template)]
#[template(path = "pages/dashboard/partial_status.html")]
struct StatusPartial {
    cpu_usage: u8,
    mem_usage: u8,
    timestamp: String,
}

#[tracing::instrument]
pub async fn status_handler() -> Result<impl IntoResponse, AppError> {
    tracing::info!("Handling dashboard status request");
    // Simple pseudo-randomness for demo purposes (using time)
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let (cpu_usage, mem_usage) = generate_status_values(now);

    // Format simple timestamp HH:MM:SS
    let time_str = chrono::Local::now().format("%H:%M:%S").to_string();

    let partial = StatusPartial {
        cpu_usage,
        mem_usage,
        timestamp: time_str,
    };

    Ok(Html(partial.render()?))
}

#[tracing::instrument(skip_all)]
pub async fn now_playing_handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let partial = build_now_playing_partial(&state).await;
    Ok(Html(partial.render()?))
}

#[tracing::instrument(skip_all)]
pub async fn handler(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    tracing::info!("Handling dashboard request");
    metrics::record_page_view("dashboard", false);

    let partial = build_now_playing_partial(&state).await;
    let data = DashboardData {
        username: partial.username.clone(),
        viewer_count: partial.viewer_count,
        is_live: partial.is_live,
    };

    // Generate initial status values (same logic as status_handler)
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (cpu_usage, mem_usage) = generate_status_values(now);
    let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();

    let page = Page {
        cpu_usage,
        mem_usage,
        timestamp,
        username: data.username,
        viewer_count: data.viewer_count,
        is_live: data.is_live,
    };
    let body = page.render()?;

    Ok(Html(body))
}

async fn build_now_playing_partial(state: &AppState) -> NowPlayingPartial {
    let np = now_playing::fetch(state).await.unwrap_or_default();
    NowPlayingPartial {
        username: "Mat".to_string(),
        viewer_count: np.listeners_count.unwrap_or_default() as i32,
        is_live: true,
    }
}

fn generate_status_values(now_secs: u64) -> (u8, u8) {
    let cpu_usage = (now_secs % 40 + 20) as u8; // 20-60%
    let mem_usage = ((now_secs * 7) % 30 + 40) as u8; // 40-70%
    (cpu_usage, mem_usage)
}

#[cfg(test)]
mod tests {
    use super::generate_status_values;

    #[test]
    fn status_values_are_deterministic() {
        let first = generate_status_values(123456);
        let second = generate_status_values(123456);
        assert_eq!(first, second);
    }

    #[test]
    fn status_values_stay_in_expected_ranges() {
        for now in [0_u64, 1, 39, 40, 41, 999_999] {
            let (cpu, mem) = generate_status_values(now);
            assert!((20..=60).contains(&cpu));
            assert!((40..=70).contains(&mem));
        }
    }

    #[test]
    fn status_values_differ_across_inputs() {
        let a = generate_status_values(100);
        let b = generate_status_values(105);
        // Different inputs should produce different CPU values (mod 40 cycle)
        assert_ne!(a, b);
    }

    #[test]
    fn status_values_at_modulo_boundaries() {
        // CPU = now % 40 + 20, so at now=40 we wrap
        let (cpu_at_0, _) = generate_status_values(0);
        let (cpu_at_40, _) = generate_status_values(40);
        assert_eq!(cpu_at_0, cpu_at_40);
    }
}

#[cfg(test)]
mod dashboard_test;
