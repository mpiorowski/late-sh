//! Traffic rendering. Reads geometry, lane configs, scenery, shoulders, and
//! dividers from the active stage on each frame so the picture updates
//! immediately when a stage transition occurs.
//!
//! Layout (left → right):
//! `[minimap] [gap] [left grass] [road] [right grass] [gap] [stats]`

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthChar;

use super::state::{Config, Phase, State, TrafficDir, TrafficScreen, hash3};
use super::theme;
use super::track::{Lanes, Stage, Track};
use super::tracks::ALL_TRACKS;
use crate::app::arcade::ui::{
    GameBottomBar, centered_rect, draw_game_frame, draw_game_overlay, keys_line, status_line,
};
use crate::app::common::theme as app_theme;

const PLAYER_FG: Color = Color::Cyan;
const SAME_DIR_CAR_FG: Color = Color::Rgb(200, 200, 200);
const ONCOMING_CAR_FG: Color = Color::Rgb(230, 60, 60);
const BORDER_FG: Color = Color::Rgb(80, 80, 80);

const FADE_ROWS: u16 = 4;
const FADE_FACTORS: [f32; 4] = [0.80, 0.50, 0.25, 0.06];

const MINI_BG: Color = Color::Rgb(14, 14, 14);
const MINI_BORDER: Color = Color::Rgb(55, 55, 55);
const MINI_DIVIDER: Color = Color::Rgb(70, 58, 20);
const MINI_SAME: Color = Color::Rgb(85, 85, 85);
const MINI_ONCOMING: Color = Color::Rgb(130, 50, 50);
const MINI_PLAYER: Color = Color::Rgb(0, 100, 100);
const MINI_OBSTACLE_SIMPLE: Color = Color::Rgb(200, 200, 0);
const MINI_OBSTACLE_CRASH: Color = Color::Rgb(200, 0, 0);

// ─── Public entry ────────────────────────────────────────────────────────────

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    match state.screen {
        TrafficScreen::Picker => draw_picker(frame, area, state, show_bottom_bar),
        TrafficScreen::Racing => draw_race(frame, area, state, show_bottom_bar),
    }
}

// ─── Picker ──────────────────────────────────────────────────────────────────

fn draw_picker(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let bottom = GameBottomBar {
        status: status_line(vec![
            (
                "tracks",
                format!("{}", ALL_TRACKS.len()),
                app_theme::TEXT_BRIGHT(),
            ),
            (
                "best",
                if state.best_score > 0 {
                    format_score(state.best_score)
                } else {
                    "—".to_string()
                },
                app_theme::AMBER_GLOW(),
            ),
        ]),
        keys: keys_line(vec![("j/k", "select"), ("Enter", "drive"), ("Esc", "exit")]),
        tip: Some(crate::app::arcade::ui::tip_line(
            "Pick a track. Each track has multiple stages with different roads, themes, and hazards.",
        )),
    };
    let content_area = draw_game_frame(
        frame,
        area,
        "Shit I'm Late — pick a track",
        bottom,
        show_bottom_bar,
    );

    if content_area.height < 6 || content_area.width < 40 {
        return;
    }

    let list_w = (content_area.width as u32 * 35 / 100).max(20) as u16;
    let list_area = Rect {
        x: content_area.x + 2,
        y: content_area.y + 1,
        width: list_w,
        height: content_area.height.saturating_sub(2),
    };
    let detail_area = Rect {
        x: content_area.x + 2 + list_w + 2,
        y: content_area.y + 1,
        width: content_area.width.saturating_sub(list_w + 6),
        height: content_area.height.saturating_sub(2),
    };

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(ALL_TRACKS.len());
    for (i, t) in ALL_TRACKS.iter().enumerate() {
        let selected = i == state.picker_selected_idx;
        let prefix = if selected { " ▶ " } else { "   " };
        let style = if selected {
            Style::default()
                .fg(app_theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app_theme::TEXT_BRIGHT())
        };
        lines.push(Line::from(vec![
            Span::styled(prefix.to_string(), style),
            Span::styled(t.name.to_string(), style),
        ]));
        lines.push(Line::from(Span::styled(
            format!("       by {}", t.author),
            Style::default().fg(app_theme::TEXT_DIM()),
        )));
        lines.push(Line::from(""));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), list_area);

    if let Some(track) = ALL_TRACKS.get(state.picker_selected_idx).copied() {
        let mut det: Vec<Line<'static>> = Vec::new();
        det.push(Line::from(Span::styled(
            track.name.to_string(),
            Style::default()
                .fg(app_theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )));
        det.push(Line::from(Span::styled(
            format!("by {}", track.author),
            Style::default().fg(app_theme::TEXT_DIM()),
        )));
        det.push(Line::from(""));
        det.push(Line::from(Span::styled(
            track.description.to_string(),
            Style::default().fg(app_theme::TEXT_BRIGHT()),
        )));
        det.push(Line::from(""));
        det.push(Line::from(Span::styled(
            format!(
                "{} stages — total {:.0} km",
                track.stages.len(),
                track.total_distance_km()
            ),
            Style::default().fg(app_theme::SUCCESS()),
        )));
        det.push(Line::from(""));
        for (i, stage) in track.stages.iter().enumerate() {
            det.push(Line::from(vec![
                Span::styled(
                    format!(" {}. {} ", i + 1, stage.icon),
                    Style::default().fg(app_theme::TEXT_DIM()),
                ),
                Span::styled(
                    stage.name.to_string(),
                    Style::default().fg(app_theme::TEXT_BRIGHT()),
                ),
                Span::styled(
                    format!("  {} {:.0} km", stage.theme.icon(), stage.distance_km),
                    Style::default().fg(app_theme::TEXT_DIM()),
                ),
            ]));
        }
        if let Some(best) = state.best_scores.get(track.name).copied() {
            det.push(Line::from(""));
            det.push(Line::from(vec![
                Span::styled(
                    " Best ".to_string(),
                    Style::default().fg(app_theme::TEXT_DIM()),
                ),
                Span::styled(
                    format_score(best),
                    Style::default()
                        .fg(app_theme::AMBER_GLOW())
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
        frame.render_widget(Paragraph::new(det).wrap(Wrap { trim: false }), detail_area);
    }
}

// ─── Race ─────────────────────────────────────────────────────────────────────

struct StageGeom {
    road_width: u16,
    lane_starts: Vec<u16>,
    total_lanes: usize,
    incoming: usize,
    grass_left_w: u16,
    grass_right_w: u16,
}

fn compute_geom(stage: &Stage, road_x: u16) -> StageGeom {
    let lanes = &stage.road.lanes;
    let total = lanes.total();
    let lane_starts: Vec<u16> = (0..total)
        .map(|i| road_x + 1 + (i as u16) * (Config::LANE_WIDTH + 1))
        .collect();
    let road_width = 1 + (total as u16) * Config::LANE_WIDTH + total.saturating_sub(1) as u16 + 1;
    StageGeom {
        road_width,
        lane_starts,
        total_lanes: total,
        incoming: lanes.incoming.len(),
        grass_left_w: stage.road.sceneries.left.width as u16,
        grass_right_w: stage.road.sceneries.right.width as u16,
    }
}

fn draw_race(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let track_name = state.track().map(|t| t.name).unwrap_or("(no track)");
    let bottom = GameBottomBar {
        status: status_line(vec![
            (
                "speed",
                format!("{:.0} km/h", state.player_speed_kmh),
                speed_color(state.player_speed_kmh),
            ),
            ("score", format_score(state.score), app_theme::AMBER_GLOW()),
            (
                "progress",
                format!("{:.1}%", state.progress_pct()),
                app_theme::SUCCESS(),
            ),
        ]),
        keys: keys_line(vec![
            ("w/up", "accel"),
            ("s/dn", "brake"),
            ("spc", "handbrake"),
            ("a/d", "lane"),
            ("p", "pause"),
            ("r", "restart"),
            ("t", "tracks"),
            ("Esc", "exit"),
        ]),
        tip: Some(crate::app::arcade::ui::tip_line(
            "Stay on your lane. Each lane has its own min/max — overtaking on the wrong side is risky.",
        )),
    };
    let content_area = draw_game_frame(frame, area, track_name, bottom, show_bottom_bar);

    let Some(stage) = state.current_stage() else {
        return;
    };
    let Some(track) = state.track() else {
        return;
    };

    let min_w = Config::MIN_TERMINAL_WIDTH_FLOOR;
    if content_area.height < Config::MIN_TERMINAL_HEIGHT || content_area.width < min_w {
        frame.render_widget(
            Paragraph::new(format!(
                "Terminal too small — need at least {}×{} (currently: {}x{})",
                min_w,
                Config::MIN_TERMINAL_HEIGHT,
                content_area.width,
                content_area.height,
            ))
            .alignment(Alignment::Center)
            .style(Style::default().fg(app_theme::ERROR())),
            content_area,
        );
        return;
    }

    let geom_for_width = compute_geom(stage, 0);
    let mini_gap: u16 = 2;
    let stats_gap: u16 = 2;
    let stats_min: u16 = 30;
    let mini_w = mini_width(stage.road.lanes);
    let block_w = mini_w
        + mini_gap
        + geom_for_width.grass_left_w
        + geom_for_width.road_width
        + geom_for_width.grass_right_w
        + stats_gap
        + stats_min;
    let block_x = content_area.x + content_area.width.saturating_sub(block_w) / 2;
    let mini_x = block_x;
    let grass_left_x = block_x + mini_w + mini_gap;
    let road_x = grass_left_x + geom_for_width.grass_left_w;
    let road_y = content_area.y + content_area.height.saturating_sub(Config::VISIBLE_ROWS) / 2;
    let road_height = content_area.height.min(Config::VISIBLE_ROWS);
    let grass_right_x_end = road_x + geom_for_width.road_width + geom_for_width.grass_right_w;
    let stats_x = grass_right_x_end + stats_gap;
    let stats_w = (content_area.x + content_area.width).saturating_sub(stats_x);

    let road_area = Rect {
        x: road_x,
        y: road_y,
        width: geom_for_width.road_width,
        height: road_height,
    };
    let geom = compute_geom(stage, road_x);

    let minimap_rows = (Config::MINIMAP_RANGE_M
        / (Config::CAR_HEIGHT_ROWS as f32 * Config::METERS_PER_ROW)) as u16;
    let mini_area = Rect {
        x: mini_x,
        y: road_y,
        width: mini_w,
        height: minimap_rows.min(road_height),
    };
    let stats_area = Rect {
        x: stats_x,
        y: road_y,
        width: stats_w,
        height: road_height,
    };

    draw_minimap(frame, mini_area, state, stage);
    draw_grass(
        frame,
        road_area,
        grass_left_x,
        grass_right_x_end,
        state,
        stage,
        state.scenery_seed,
    );
    draw_road(frame, road_area, state, track, stage, &geom);
    draw_lane_speed_labels(frame, road_area, stage, &geom);
    draw_stats(frame, stats_area, state, track, stage);

    match &state.phase {
        Phase::Dead => {
            draw_game_overlay(
                frame,
                road_area,
                "CRASH!",
                "Press r to restart",
                app_theme::ERROR(),
            );
        }
        Phase::Crashed => {
            let sub = format!("{} lives left · any key", state.lives);
            draw_game_overlay(frame, road_area, "YOU CRASHED!", &sub, app_theme::ERROR());
        }
        Phase::Finished { elapsed_s, score } => {
            let mins = (*elapsed_s as u32) / 60;
            let secs = (*elapsed_s as u32) % 60;
            let time_str = format!("{}:{:02}", mins, secs);
            let score_str = format!("Score  {}", format_score(*score));
            draw_finish_overlay(
                frame,
                road_area,
                &time_str,
                &score_str,
                app_theme::SUCCESS(),
            );
        }
        Phase::Playing if state.is_paused => {
            draw_game_overlay(
                frame,
                road_area,
                "PAUSED",
                "Press p to resume",
                app_theme::AMBER(),
            );
        }
        _ => {}
    }
}

// ─── Minimap ─────────────────────────────────────────────────────────────────

fn mini_width(lanes: Lanes) -> u16 {
    1 + lanes.incoming.len() as u16 + 1 + lanes.outgoing.len() as u16 + 1
}

fn mini_lane_offset(stage: &Stage, lane_idx: usize) -> u16 {
    let in_n = stage.road.lanes.incoming.len();
    let base = 1 + lane_idx as u16;
    if lane_idx >= in_n { base + 1 } else { base }
}

fn draw_minimap(frame: &mut Frame, area: Rect, state: &State, stage: &Stage) {
    let scale_m = Config::CAR_HEIGHT_ROWS as f32 * Config::METERS_PER_ROW;
    let rows = ((Config::MINIMAP_RANGE_M / scale_m) as u16).min(area.height);
    let buf = frame.buffer_mut();
    let in_n = stage.road.lanes.incoming.len() as u16;
    let mini_w = mini_width(stage.road.lanes);
    let divider_x = area.x + 1 + in_n;
    let right_border_x = area.x + mini_w - 1;

    for mr in 0..rows {
        let sy = area.y + mr;
        for x in area.x..area.x + mini_w {
            if let Some(c) = buf.cell_mut((x, sy)) {
                if x == area.x || x == right_border_x {
                    c.set_symbol("│").set_fg(MINI_BORDER).set_bg(MINI_BG);
                } else if x == divider_x {
                    c.set_symbol("·").set_fg(MINI_DIVIDER).set_bg(MINI_BG);
                } else {
                    c.set_symbol(" ").set_fg(MINI_BG).set_bg(MINI_BG);
                }
            }
        }
    }

    for car in &state.ai_cars {
        let ahead_m = car.pos_m - state.player_pos_m;
        if ahead_m < 0.0 {
            continue;
        }
        let idx = (ahead_m / scale_m) as u16;
        if idx >= rows {
            continue;
        }
        let sy = area.y + (rows - 1) - idx;
        let x = area.x + mini_lane_offset(stage, car.lane_idx);
        let fg = match car.direction {
            TrafficDir::Same => MINI_SAME,
            TrafficDir::Oncoming => MINI_ONCOMING,
        };
        if let Some(c) = buf.cell_mut((x, sy)) {
            c.set_symbol("▪").set_fg(fg).set_bg(MINI_BG);
        }
    }

    for obs in &state.obstacles {
        let ahead_m = obs.pos_m - state.player_pos_m;
        if ahead_m < 0.0 {
            continue;
        }
        let idx = (ahead_m / scale_m) as u16;
        if idx >= rows {
            continue;
        }
        let sy = area.y + (rows - 1) - idx;
        let x = area.x + mini_lane_offset(stage, obs.lane_idx);
        if let Some(c) = buf.cell_mut((x, sy)) {
            let fg = if obs.crash {
                MINI_OBSTACLE_CRASH
            } else {
                MINI_OBSTACLE_SIMPLE
            };
            c.set_symbol("!").set_fg(fg).set_bg(MINI_BG);
        }
    }

    let x = area.x + mini_lane_offset(stage, state.player_lane_idx);
    if let Some(c) = buf.cell_mut((x, area.y + rows.saturating_sub(1))) {
        c.set_symbol("▲").set_fg(MINI_PLAYER).set_bg(MINI_BG);
    }
}

// ─── Grass / shoulders ───────────────────────────────────────────────────────

fn darken(c: Color, factor: f32) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * factor) as u8,
            (g as f32 * factor) as u8,
            (b as f32 * factor) as u8,
        ),
        other => other,
    }
}

fn draw_grass(
    frame: &mut Frame,
    road_area: Rect,
    grass_left_x: u16,
    grass_right_end_x: u16,
    state: &State,
    stage: &Stage,
    scenery_seed: u64,
) {
    let buf = frame.buffer_mut();
    let track_base = (state.player_pos_m / Config::METERS_PER_ROW) as i32;
    let left_bg = (stage.road.sceneries.left.background.bg)(stage.theme);
    let right_bg = (stage.road.sceneries.right.background.bg)(stage.theme);
    let road_right = road_area.x + road_area.width;

    for r in 0..Config::VISIBLE_ROWS {
        let screen_y = road_area.y + r;
        if screen_y >= road_area.y + road_area.height {
            break;
        }
        let ri = r as i32;
        let track_row = track_base - (ri - Config::PLAYER_TOP_ROW as i32);

        let bottom_fade_start = road_area.height.saturating_sub(FADE_ROWS);
        let fade: Option<f32> = if r < FADE_ROWS {
            Some(FADE_FACTORS[(FADE_ROWS - 1 - r) as usize])
        } else if r >= bottom_fade_start {
            Some(FADE_FACTORS[(r - bottom_fade_start) as usize])
        } else {
            None
        };
        let lbg = fade.map_or(left_bg, |f| darken(left_bg, f));
        let rbg = fade.map_or(right_bg, |f| darken(right_bg, f));

        for x in grass_left_x..road_area.x {
            if let Some(c) = buf.cell_mut((x, screen_y)) {
                c.set_symbol(" ").set_bg(lbg);
            }
        }
        for x in road_right..grass_right_end_x {
            if let Some(c) = buf.cell_mut((x, screen_y)) {
                c.set_symbol(" ").set_bg(rbg);
            }
        }

        draw_scenery_side(
            buf,
            stage.road.sceneries.left.objects,
            grass_left_x..road_area.x,
            screen_y,
            track_row,
            stage.theme,
            lbg,
            stage.road.sceneries.left.width,
            stage.road.shoulders.left.len() as u16,
            true,
            fade,
            scenery_seed,
        );
        draw_scenery_side(
            buf,
            stage.road.sceneries.right.objects,
            road_right..grass_right_end_x,
            screen_y,
            track_row,
            stage.theme,
            rbg,
            stage.road.sceneries.right.width,
            stage.road.shoulders.right.len() as u16,
            false,
            fade,
            scenery_seed,
        );

        draw_shoulders_side(
            buf,
            stage.road.shoulders.left,
            road_area.x,
            screen_y,
            track_row,
            stage.theme,
            lbg,
            true,
            fade,
        );
        draw_shoulders_side(
            buf,
            stage.road.shoulders.right,
            road_right - 1,
            screen_y,
            track_row,
            stage.theme,
            rbg,
            false,
            fade,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_scenery_side(
    buf: &mut ratatui::buffer::Buffer,
    objects: &[super::track::Object],
    x_range: std::ops::Range<u16>,
    screen_y: u16,
    track_row: i32,
    theme_id: super::track::Theme,
    fallback_bg: Color,
    band_width: u8,
    shoulder_width: u16,
    left_side: bool,
    fade: Option<f32>,
    run_salt: u64,
) {
    if objects.is_empty() {
        return;
    }

    const STRIDE: u16 = 4;
    const MAX_HEIGHT: i64 = 6;
    const ROW_STRIDE: i64 = 7;
    const PLACEMENT_GATE: u64 = 2;

    let band_w = band_width as u16;
    if band_w <= shoulder_width {
        return;
    }
    let inner_count = band_w - shoulder_width;

    for d in 0..inner_count {
        if d % STRIDE != 1 {
            continue;
        }
        let col_salt = hash3(run_salt as i64, d as i64, if left_side { 0 } else { 1 }) as i64;
        for h in 0..MAX_HEIGHT {
            let anchor_row = track_row as i64 - h;
            if (anchor_row + col_salt).rem_euclid(ROW_STRIDE) != 0 {
                continue;
            }
            let seed = hash3(
                anchor_row ^ run_salt as i64,
                d as i64,
                if left_side { 0 } else { 1 },
            );
            if !seed.is_multiple_of(PLACEMENT_GATE) {
                continue;
            }
            let obj = pick_object(objects, seed);
            let sprite = (obj.style.sprite)(theme_id);
            let sprite_h = sprite.glyphs.len() as i64;
            if h >= sprite_h {
                break;
            }
            let sprite_w = sprite.width as u16;
            if d + sprite_w > inner_count {
                break;
            }
            let row_idx = (sprite_h - 1 - h) as usize;
            let row_glyphs = sprite.glyphs[row_idx];
            let bottom_row = h == 0;
            let trunk_fg = theme::trunk_color(theme_id);
            for (w_idx, glyph) in row_glyphs.iter().enumerate() {
                if *glyph == " " {
                    continue;
                }
                let xx = if left_side {
                    x_range.start.saturating_add(d + w_idx as u16)
                } else {
                    x_range.end.saturating_sub(d + sprite_w - w_idx as u16)
                };
                if xx < x_range.start || xx >= x_range.end {
                    continue;
                }
                let raw_fg = if bottom_row && obj.style.has_trunk {
                    trunk_fg
                } else {
                    sprite.fg
                };
                let fg = fade.map_or(raw_fg, |f| darken(raw_fg, f));
                if let Some(c) = buf.cell_mut((xx, screen_y)) {
                    c.set_symbol(glyph).set_fg(fg).set_bg(fallback_bg);
                }
            }
            break;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_shoulders_side(
    buf: &mut ratatui::buffer::Buffer,
    shoulders: &[super::track::Shoulder],
    base_x: u16,
    screen_y: u16,
    track_row: i32,
    theme_id: super::track::Theme,
    fallback_bg: Color,
    left_side: bool,
    fade: Option<f32>,
) {
    for (i, sh) in shoulders.iter().enumerate() {
        let x = if left_side {
            base_x.saturating_sub(i as u16 + 1)
        } else {
            base_x + i as u16 + 1
        };
        let cell = (sh.style.cell)(theme_id, track_row, sh.repeat, fallback_bg);
        let fg = fade.map_or(cell.fg, |f| darken(cell.fg, f));
        let bg = fade.map_or(cell.bg, |f| darken(cell.bg, f));
        if let Some(c) = buf.cell_mut((x, screen_y)) {
            c.set_symbol(cell.sym).set_fg(fg).set_bg(bg);
        }
    }
}

fn pick_object(objects: &[super::track::Object], seed: u64) -> &super::track::Object {
    let total: f32 = objects.iter().map(|o| o.incidence).sum();
    if total <= 0.0 {
        return &objects[0];
    }
    let r = ((seed % 10_000) as f32 / 10_000.0) * total;
    let mut acc = 0.0;
    for o in objects {
        acc += o.incidence;
        if r < acc {
            return o;
        }
    }
    &objects[objects.len() - 1]
}

// ─── Road ────────────────────────────────────────────────────────────────────

fn lane_separator_x(geom: &StageGeom, lane_idx: usize) -> Option<u16> {
    if lane_idx + 1 >= geom.total_lanes {
        return None;
    }
    Some(geom.lane_starts[lane_idx] + Config::LANE_WIDTH)
}

fn player_body_x_start(geom: &StageGeom, lane_f: f32) -> u16 {
    let total = geom.total_lanes as i32;
    let floor_i = (lane_f.floor() as i32).clamp(0, (total - 1).max(0));
    let frac = (lane_f - floor_i as f32).clamp(0.0, 1.0);
    let a = geom.lane_starts[floor_i as usize] as f32 + 1.0;
    let next_i = (floor_i + 1).min(total - 1).max(0);
    let b = geom.lane_starts[next_i as usize] as f32 + 1.0;
    (a + (b - a) * frac).round() as u16
}

fn is_car_col(col: u16) -> bool {
    (1..=3).contains(&col)
}

const SEP_LINE_FG: Color = Color::Rgb(160, 160, 160);
const SEP_LABEL_FG: Color = Color::Rgb(240, 220, 140);
const SEP_BG: Color = Color::Rgb(10, 10, 10);

fn draw_road(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    track: &Track,
    stage: &Stage,
    geom: &StageGeom,
) {
    let buf = frame.buffer_mut();
    let lanes = stage.road.lanes;
    let theme_id = stage.theme;
    let player_track_row = (state.player_pos_m / Config::METERS_PER_ROW) as i32;

    // Pre-compute which screen rows contain a stage separator and which stage
    // starts there. Stage 0 separator is at track pos 0 (pre-stage is before that).
    let scale = state.distance_scale();
    let mut sep_at: Vec<(i32, usize)> = Vec::with_capacity(track.stages.len());
    let mut sep_pos_m = 0.0f32;
    for (idx, stg) in track.stages.iter().enumerate() {
        let sep_row = state.track_to_screen_row(sep_pos_m);
        sep_at.push((sep_row, idx));
        sep_pos_m += stg.distance_km * 1000.0 * scale;
    }

    for r in 0..Config::VISIBLE_ROWS {
        let screen_y = area.y + r;
        if screen_y >= area.y + area.height {
            break;
        }
        let ri = r as i32;
        let track_row = player_track_row - (ri - Config::PLAYER_TOP_ROW as i32);

        let bottom_fade_start = area.height.saturating_sub(FADE_ROWS);
        let fade_idx: Option<usize> = if r < FADE_ROWS {
            Some((FADE_ROWS - 1 - r) as usize)
        } else if r >= bottom_fade_start {
            Some((r - bottom_fade_start) as usize)
        } else {
            None
        };

        if let Some(cell) = buf.cell_mut((area.x, screen_y)) {
            cell.set_symbol("│")
                .set_fg(BORDER_FG)
                .set_bg(Color::Rgb(0, 0, 0));
        }

        for lane_idx in 0..geom.total_lanes {
            let lane_cfg = match lanes.get(lane_idx) {
                Some(l) => l,
                None => continue,
            };
            let bg = (lane_cfg.style.bg)(theme_id);
            let lane_x_start = geom.lane_starts[lane_idx];
            let car_hit = state.ai_cars.iter().find(|c| {
                c.lane_idx == lane_idx
                    && ri >= car_top_row(state, c)
                    && ri < car_top_row(state, c) + c.height_rows as i32
            });
            let obstacle_hit = state
                .obstacles
                .iter()
                .find(|o| o.lane_idx == lane_idx && state.track_to_screen_row(o.pos_m) == ri);

            for col in 0..Config::LANE_WIDTH {
                let screen_x = lane_x_start + col;
                if let Some(cell) = buf.cell_mut((screen_x, screen_y)) {
                    if let Some(car) = car_hit
                        && is_car_col(col)
                    {
                        let fg = match car.direction {
                            TrafficDir::Same => SAME_DIR_CAR_FG,
                            TrafficDir::Oncoming => ONCOMING_CAR_FG,
                        };
                        cell.set_symbol("█").set_fg(fg).set_bg(bg);
                    } else if let Some(obs) = obstacle_hit {
                        let (glyphs, fg) = obs.style.glyphs;
                        if is_car_col(col) {
                            let body_col = (col as i32 - 1).clamp(0, 2) as usize;
                            cell.set_symbol(glyphs[body_col]).set_fg(fg).set_bg(bg);
                        } else {
                            let cell_v = (lane_cfg.style.cell)(theme_id, track_row, col);
                            cell.set_symbol(cell_v.sym)
                                .set_fg(cell_v.fg)
                                .set_bg(cell_v.bg);
                        }
                    } else {
                        let cell_v = (lane_cfg.style.cell)(theme_id, track_row, col);
                        cell.set_symbol(cell_v.sym)
                            .set_fg(cell_v.fg)
                            .set_bg(cell_v.bg);
                    }
                }
            }
        }

        for lane_idx in 0..geom.total_lanes {
            let Some(sep_x) = lane_separator_x(geom, lane_idx) else {
                continue;
            };
            let next_dir_same = (lane_idx + 1 < geom.incoming) || (lane_idx >= geom.incoming);
            let div_style = if next_dir_same {
                stage.road.aspect.dividers.lane
            } else {
                stage.road.aspect.dividers.primary
            };
            let nb_bg = match lanes.get(lane_idx) {
                Some(l) => (l.style.bg)(theme_id),
                None => Color::Rgb(0, 0, 0),
            };
            let cell_v = (div_style.cell)(track_row, nb_bg);
            if let Some(cell) = buf.cell_mut((sep_x, screen_y)) {
                cell.set_symbol(cell_v.sym)
                    .set_fg(cell_v.fg)
                    .set_bg(cell_v.bg);
            }
        }

        let right_border_x = area.x + geom.road_width - 1;
        if let Some(cell) = buf.cell_mut((right_border_x, screen_y)) {
            cell.set_symbol("│")
                .set_fg(BORDER_FG)
                .set_bg(Color::Rgb(0, 0, 0));
        }

        // Stage separator: overwrite this row with a horizontal rule + stage label.
        if let Some(&(_, sep_stage_idx)) = sep_at.iter().find(|&&(row, _)| row == ri) {
            let sep_stage = &track.stages[sep_stage_idx];
            let label: String = format!(" {} {} ", sep_stage.icon, sep_stage.name);
            let label_w: usize = label
                .chars()
                .map(|c| UnicodeWidthChar::width(c).unwrap_or(1))
                .sum();
            let inner_w = (geom.road_width as usize).saturating_sub(2);
            let pad_left = inner_w.saturating_sub(label_w) / 2;
            let mut x_inner = 0usize;
            let mut label_chars = label.chars();
            let mut in_label_idx = 0usize;
            while x_inner < inner_w {
                let screen_x = area.x + 1 + x_inner as u16;
                let in_label = x_inner >= pad_left && in_label_idx < label_w;
                if in_label {
                    if let Some(ch) = label_chars.next() {
                        let w = UnicodeWidthChar::width(ch).unwrap_or(1);
                        let mut tmp = [0u8; 4];
                        let sym = ch.encode_utf8(&mut tmp);
                        if let Some(cell) = buf.cell_mut((screen_x, screen_y)) {
                            cell.set_symbol(sym).set_fg(SEP_LABEL_FG).set_bg(SEP_BG);
                        }
                        if w == 2
                            && let Some(cell) = buf.cell_mut((screen_x + 1, screen_y))
                        {
                            cell.set_symbol(" ").set_fg(SEP_LABEL_FG).set_bg(SEP_BG);
                        }
                        in_label_idx += w;
                        x_inner += w;
                    } else {
                        if let Some(cell) = buf.cell_mut((screen_x, screen_y)) {
                            cell.set_symbol("─").set_fg(SEP_LINE_FG).set_bg(SEP_BG);
                        }
                        x_inner += 1;
                    }
                } else {
                    if let Some(cell) = buf.cell_mut((screen_x, screen_y)) {
                        cell.set_symbol("─").set_fg(SEP_LINE_FG).set_bg(SEP_BG);
                    }
                    x_inner += 1;
                }
            }
        }

        let p_top = Config::PLAYER_TOP_ROW as i32;
        let p_h = Config::CAR_HEIGHT_ROWS as i32;
        if ri >= p_top && ri < p_top + p_h && state.player_visible() {
            let body_x = player_body_x_start(geom, state.player_lane_display);
            let bg = lanes
                .get(state.player_lane_idx)
                .map(|l| (l.style.bg)(theme_id))
                .unwrap_or(Color::Rgb(0, 0, 0));
            for col in 0..3 {
                if let Some(cell) = buf.cell_mut((body_x + col, screen_y)) {
                    cell.set_symbol("█").set_fg(PLAYER_FG).set_bg(bg);
                }
            }
        }

        if let Some(fi) = fade_idx {
            let factor = FADE_FACTORS[fi];
            for x in 0..geom.road_width {
                if let Some(cell) = buf.cell_mut((area.x + x, screen_y)) {
                    let new_bg = darken(cell.bg, factor);
                    let new_fg = darken(cell.fg, factor);
                    cell.set_bg(new_bg).set_fg(new_fg);
                }
            }
        }
    }
}

fn car_top_row(state: &State, c: &super::state::AiCar) -> i32 {
    let center = state.track_to_screen_row(c.pos_m);
    center - (c.height_rows as i32) / 2
}

// ─── Stats panel ─────────────────────────────────────────────────────────────

fn draw_stats(frame: &mut Frame, area: Rect, state: &State, track: &Track, stage: &Stage) {
    if area.width < 18 || area.height < 8 {
        return;
    }

    let displayed_km = state.displayed_km_total();
    let total_km = state.track_total_km();
    let stage_km = state.displayed_km_stage();
    let stage_total_km = state.current_stage_km();

    let dim = |s: &'static str| Span::styled(s, Style::default().fg(app_theme::TEXT_DIM()));
    let bright = |s: String| Span::styled(s, Style::default().fg(app_theme::TEXT_BRIGHT()));

    let mut lines: Vec<Line<'static>> = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(" {}", track.name),
            Style::default()
                .fg(app_theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("   by {}", track.author),
            Style::default().fg(app_theme::TEXT_DIM()),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!(" {} ", stage.icon),
                Style::default().fg(app_theme::TEXT_BRIGHT()),
            ),
            Span::styled(
                stage.name.to_string(),
                Style::default()
                    .fg(app_theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!(" {} ", stage.theme.icon()),
                Style::default().fg(app_theme::TEXT_BRIGHT()),
            ),
            dim("Scenery: "),
            bright(stage.theme.name().to_string()),
        ]),
        Line::from(Span::styled(
            format!("   {}", stage.description),
            Style::default().fg(app_theme::TEXT_DIM()),
        )),
        Line::from(""),
        Line::from(vec![
            dim(" Speed "),
            Span::styled(
                format!("{:.0} km/h", state.player_speed_kmh),
                Style::default()
                    .fg(speed_color(state.player_speed_kmh))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    if let Some(lane) = stage.road.lanes.get(state.player_lane_idx) {
        lines.push(Line::from(vec![
            dim("   lane min/max "),
            bright(format!(
                "{:.0}/{:.0}",
                lane.own_min_speed, lane.own_max_speed
            )),
        ]));
    }
    let max_lives = state.track().map(|t| t.lives).unwrap_or(state.lives);
    let lost = max_lives.saturating_sub(state.lives);
    lines.push(Line::from(vec![
        dim(" Lives "),
        Span::styled(
            "♥ ".repeat(state.lives as usize),
            Style::default()
                .fg(app_theme::ERROR())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "♡ ".repeat(lost as usize),
            Style::default().fg(app_theme::TEXT_DIM()),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        dim(" Stage "),
        bright(format!("{:.1} / {:.0} km", stage_km, stage_total_km)),
    ]));
    lines.push(Line::from(vec![
        dim(" Track "),
        bright(format!("{:.1} / {:.0} km", displayed_km, total_km)),
    ]));
    lines.push(Line::from(vec![
        dim(" Time "),
        bright(state.elapsed_formatted()),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        dim(" Score "),
        Span::styled(
            format_score(state.score),
            Style::default()
                .fg(app_theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    if let Some(best) = state.best_scores.get(track.name).copied() {
        lines.push(Line::from(vec![
            dim(" Best "),
            Span::styled(
                format_score(best),
                Style::default().fg(app_theme::SUCCESS()),
            ),
        ]));
    }

    lines.push(Line::from(""));
    let bar_width = (area.width as usize).saturating_sub(4).min(20);
    let pct = state.progress_pct();
    let filled = ((pct / 100.0) * bar_width as f32) as usize;
    let empty = bar_width.saturating_sub(filled);
    lines.push(Line::from(Span::styled(
        format!(" [{}{}] {:.0}%", "█".repeat(filled), "░".repeat(empty), pct),
        Style::default().fg(app_theme::AMBER()),
    )));

    let km_left = (stage_total_km - stage_km).max(0.0);
    lines.push(Line::from(vec![
        dim(" Now  "),
        bright(format!("{} ({:.1} km left)", stage.name, km_left)),
    ]));
    if let Some(next) = track.stages.get(state.current_stage_idx + 1) {
        lines.push(Line::from(vec![
            dim(" Next  "),
            Span::styled(
                next.name.to_string(),
                Style::default().fg(app_theme::TEXT_DIM()),
            ),
        ]));
    }

    if !state.recent_effects.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(dim(" Recent")));
        for eff in &state.recent_effects {
            lines.push(Line::from(Span::styled(
                format!("   {}", eff.label),
                Style::default().fg(app_theme::ERROR()),
            )));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn speed_color(kmh: f32) -> Color {
    if kmh >= 150.0 {
        Color::Red
    } else if kmh >= 100.0 {
        Color::Yellow
    } else if kmh >= 50.0 {
        app_theme::SUCCESS()
    } else {
        app_theme::TEXT_DIM()
    }
}

fn draw_lane_speed_labels(frame: &mut Frame, area: Rect, stage: &Stage, geom: &StageGeom) {
    let screen_y = area.y + area.height.saturating_sub(1);
    if screen_y < area.y {
        return;
    }
    let buf = frame.buffer_mut();
    for lane_idx in 0..geom.total_lanes {
        let Some(lane) = stage.road.lanes.get(lane_idx) else {
            continue;
        };
        let speed = lane.own_max_speed as u16;
        let label = format!("{:>3}", speed);
        let lane_x = geom.lane_starts[lane_idx];
        for (i, ch) in label.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((lane_x + 1 + i as u16, screen_y)) {
                let mut tmp = [0u8; 4];
                cell.set_symbol(ch.encode_utf8(&mut tmp))
                    .set_fg(Color::Rgb(160, 160, 100))
                    .set_bg(Color::Rgb(0, 0, 0));
            }
        }
    }
}

fn draw_finish_overlay(
    frame: &mut Frame,
    area: Rect,
    time_str: &str,
    score_str: &str,
    color: Color,
) {
    let overlay_area = centered_rect(area, 36.min(area.width), 5.min(area.height));
    let overlay = Paragraph::new(vec![
        Line::from(Span::styled(
            " FINISHED! ",
            Style::default()
                .bg(color)
                .fg(ratatui::style::Color::Reset)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            time_str.to_string(),
            Style::default().fg(app_theme::TEXT_DIM()),
        )),
        Line::from(Span::styled(
            score_str.to_string(),
            Style::default().fg(app_theme::TEXT_DIM()),
        )),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color)),
    );
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(overlay, overlay_area);
}

fn format_score(s: i64) -> String {
    let s = s.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}
