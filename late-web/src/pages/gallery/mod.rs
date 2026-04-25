use std::collections::HashMap;

use anyhow::Context;
use askama::Template;
use axum::{
    Router,
    extract::{Query, State},
    response::{Html, IntoResponse},
    routing::get,
};
use dartboard_core::{Canvas, CellValue, Pos};
use late_core::models::artboard::{Snapshot, SnapshotSummary};
use serde::{Deserialize, Serialize};

use crate::{AppState, error::AppError, metrics};

const DAILY_PREFIX: &str = "daily:";
const MONTHLY_PREFIX: &str = "monthly:";

pub fn router() -> Router<AppState> {
    Router::new().route("/gallery", get(handler))
}

#[derive(Deserialize)]
struct GalleryQuery {
    key: Option<String>,
}

#[derive(Template)]
#[template(path = "pages/gallery/page.html")]
struct Page {
    live_items: Vec<SnapshotNavItem>,
    daily_items: Vec<SnapshotNavItem>,
    monthly_items: Vec<SnapshotNavItem>,
    show_live_empty: bool,
    show_daily_empty: bool,
    show_monthly_empty: bool,
    has_selected: bool,
    selected_title: String,
    selected_updated: String,
    selected_cell_count: usize,
    selected_author_count: usize,
    snapshot_json: String,
}

#[derive(Debug)]
struct SnapshotNavItem {
    key: String,
    label: String,
    meta: String,
    active: bool,
}

#[derive(Debug, Default, Deserialize)]
struct GalleryProvenance {
    cells: Vec<(Pos, String)>,
}

#[derive(Serialize)]
struct GallerySnapshotData {
    key: String,
    width: usize,
    height: usize,
    cells: Vec<GalleryCell>,
    authors: Vec<String>,
}

#[derive(Serialize)]
struct GalleryCell(usize, usize, String, usize, Option<String>, Option<usize>);

#[tracing::instrument(skip_all)]
async fn handler(
    State(state): State<AppState>,
    Query(query): Query<GalleryQuery>,
) -> Result<impl IntoResponse, AppError> {
    metrics::record_page_view("gallery", false);

    let client = state
        .db
        .get()
        .await
        .context("failed to get db client for gallery")?;
    let live = Snapshot::find_summary_by_board_key(&client, Snapshot::MAIN_BOARD_KEY)
        .await
        .context("failed to load live artboard snapshot")?;
    let daily = Snapshot::list_summaries_by_board_key_prefix(&client, DAILY_PREFIX)
        .await
        .context("failed to load daily artboard snapshots")?;
    let monthly = Snapshot::list_summaries_by_board_key_prefix(&client, MONTHLY_PREFIX)
        .await
        .context("failed to load monthly artboard snapshots")?;

    let requested_key = query.key;
    let default_key = live
        .as_ref()
        .map(|snapshot| snapshot.board_key.clone())
        .or_else(|| daily.first().map(|snapshot| snapshot.board_key.clone()))
        .or_else(|| monthly.first().map(|snapshot| snapshot.board_key.clone()));
    let selected_key = requested_key.or(default_key);
    let selected = match selected_key.as_deref() {
        Some(key) => Snapshot::find_by_board_key(&client, key)
            .await
            .with_context(|| format!("failed to load artboard snapshot {key}"))?,
        None => None,
    };

    let live_items: Vec<SnapshotNavItem> = live
        .iter()
        .map(|snapshot| nav_item(snapshot, selected_key.as_deref()))
        .collect();
    let daily_items: Vec<SnapshotNavItem> = daily
        .iter()
        .map(|snapshot| nav_item(snapshot, selected_key.as_deref()))
        .collect();
    let monthly_items: Vec<SnapshotNavItem> = monthly
        .iter()
        .map(|snapshot| nav_item(snapshot, selected_key.as_deref()))
        .collect();

    let show_live_empty = live_items.is_empty();
    let show_daily_empty = daily_items.is_empty();
    let show_monthly_empty = monthly_items.is_empty();
    let mut page = Page {
        live_items,
        daily_items,
        monthly_items,
        show_live_empty,
        show_daily_empty,
        show_monthly_empty,
        has_selected: false,
        selected_title: "No snapshot selected".to_string(),
        selected_updated: String::new(),
        selected_cell_count: 0,
        selected_author_count: 0,
        snapshot_json: "null".to_string(),
    };

    if let Some(snapshot) = selected {
        let selected = build_selected_snapshot(snapshot)?;
        page.has_selected = true;
        page.selected_title = selected.title;
        page.selected_updated = selected.updated;
        page.selected_cell_count = selected.cell_count;
        page.selected_author_count = selected.author_count;
        page.snapshot_json = selected.snapshot_json;
    }

    Ok(Html(page.render()?))
}

struct SelectedSnapshot {
    title: String,
    updated: String,
    cell_count: usize,
    author_count: usize,
    snapshot_json: String,
}

fn build_selected_snapshot(snapshot: Snapshot) -> anyhow::Result<SelectedSnapshot> {
    let canvas: Canvas =
        serde_json::from_value(snapshot.canvas).context("failed to decode artboard canvas")?;
    let provenance: GalleryProvenance = serde_json::from_value(snapshot.provenance)
        .context("failed to decode artboard provenance")?;
    let data = snapshot_data(&snapshot.board_key, &canvas, &provenance);
    let cell_count = data.cells.len();
    let author_count = data.authors.len();
    let snapshot_json = serde_json::to_string(&data)
        .context("failed to serialize gallery snapshot data")?
        .replace("</", "<\\/");

    Ok(SelectedSnapshot {
        title: snapshot_title(&snapshot.board_key),
        updated: snapshot.updated.format("%Y-%m-%d %H:%M UTC").to_string(),
        cell_count,
        author_count,
        snapshot_json,
    })
}

fn snapshot_data(
    board_key: &str,
    canvas: &Canvas,
    provenance: &GalleryProvenance,
) -> GallerySnapshotData {
    let authors = provenance.author_map();
    let mut author_names = Vec::new();
    let mut author_indices = HashMap::new();
    let mut cells = Vec::new();
    for (pos, cell) in canvas.iter() {
        let ch = match cell {
            CellValue::Narrow(ch) | CellValue::Wide(ch) => *ch,
            CellValue::WideCont => continue,
        };
        let Some(glyph) = canvas.glyph_at(*pos) else {
            continue;
        };
        let author = authors.get(pos).map(|username| {
            if let Some(index) = author_indices.get(username) {
                *index
            } else {
                let index = author_names.len();
                author_names.push(username.clone());
                author_indices.insert(username.clone(), index);
                index
            }
        });
        cells.push(GalleryCell(
            pos.x,
            pos.y,
            ch.to_string(),
            glyph.width,
            glyph.fg.map(rgb_hex),
            author,
        ));
    }
    cells.sort_by_key(|cell| (cell.1, cell.0));

    GallerySnapshotData {
        key: board_key.to_string(),
        width: canvas.width,
        height: canvas.height,
        cells,
        authors: author_names,
    }
}

impl GalleryProvenance {
    fn author_map(&self) -> HashMap<Pos, String> {
        self.cells.iter().cloned().collect()
    }
}

fn nav_item(snapshot: &SnapshotSummary, selected_key: Option<&str>) -> SnapshotNavItem {
    SnapshotNavItem {
        key: snapshot.board_key.clone(),
        label: snapshot_label(&snapshot.board_key),
        meta: snapshot.updated.format("%Y-%m-%d %H:%M UTC").to_string(),
        active: Some(snapshot.board_key.as_str()) == selected_key,
    }
}

fn snapshot_title(key: &str) -> String {
    match key {
        Snapshot::MAIN_BOARD_KEY => "Live / latest saved".to_string(),
        _ if key.starts_with(DAILY_PREFIX) => {
            format!("Daily {}", key.trim_start_matches(DAILY_PREFIX))
        }
        _ if key.starts_with(MONTHLY_PREFIX) => {
            format!("Monthly {}", key.trim_start_matches(MONTHLY_PREFIX))
        }
        _ => key.to_string(),
    }
}

fn snapshot_label(key: &str) -> String {
    match key {
        Snapshot::MAIN_BOARD_KEY => "Live".to_string(),
        _ if key.starts_with(DAILY_PREFIX) => key.trim_start_matches(DAILY_PREFIX).to_string(),
        _ if key.starts_with(MONTHLY_PREFIX) => key.trim_start_matches(MONTHLY_PREFIX).to_string(),
        _ => key.to_string(),
    }
}

fn rgb_hex(color: dartboard_core::RgbColor) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

#[cfg(test)]
mod tests {
    use super::{snapshot_label, snapshot_title};

    #[test]
    fn snapshot_labels_are_human_readable() {
        assert_eq!(snapshot_label("main"), "Live");
        assert_eq!(snapshot_label("daily:2026-04-24"), "2026-04-24");
        assert_eq!(snapshot_label("monthly:2026-04"), "2026-04");
    }

    #[test]
    fn snapshot_titles_include_kind() {
        assert_eq!(snapshot_title("main"), "Live / latest saved");
        assert_eq!(snapshot_title("daily:2026-04-24"), "Daily 2026-04-24");
        assert_eq!(snapshot_title("monthly:2026-04"), "Monthly 2026-04");
    }
}
