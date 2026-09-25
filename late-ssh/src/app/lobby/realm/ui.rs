//! `Screen::Realm` renderer: one-line header (game, points, next refill),
//! the active view (map/targets/overview/log), and the pinned key-hint
//! footer.

use chrono::Utc;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::app::common::theme;

use super::log_ui::draw_log_view;
use super::map_ui::draw_map_view;
use super::overview_ui::draw_overview_view;
use super::results_ui::draw_results;
use super::state::{RealmState, RealmView, points_left};
use super::svc::{next_reset, reset_hour_label};
use super::targets_ui::draw_targets_view;

/// The world in the board header. A generated one has no name anybody knows,
/// so it says what it is instead: "Uncharted · 5 continents · 8 islands · 160
/// lands" tells a player what they are looking at before they have panned
/// across it.
fn map_label(detail: &super::state::RealmDetail) -> String {
    let Some(map) = detail.map() else {
        return "unknown map".to_string();
    };
    match detail.state.ruleset.map_spec.as_ref() {
        Some(spec) => format!("{} · {}", map.display_name, spec.summary()),
        None => map.display_name.clone(),
    }
}

/// Trim a line of spans to `width` display columns, marking the cut with an
/// ellipsis. Used where a row has exactly one line to say everything in.
fn fit_line(spans: Vec<Span<'static>>, width: usize) -> Line<'static> {
    use unicode_width::UnicodeWidthStr;
    let total: usize = spans.iter().map(|s| s.content.width()).sum();
    if total <= width || width == 0 {
        return Line::from(spans);
    }
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for span in spans {
        let span_width = span.content.width();
        if used + span_width <= width.saturating_sub(1) {
            used += span_width;
            out.push(span);
            continue;
        }
        // The span that overruns is cut mid-way, char by char so wide
        // glyphs are not split.
        let room = width.saturating_sub(used + 1);
        let mut text = String::new();
        for ch in span.content.chars() {
            let ch_width = ch.to_string().width();
            if text.width() + ch_width > room {
                break;
            }
            text.push(ch);
        }
        text.push('…');
        out.push(Span::styled(text, span.style));
        break;
    }
    Line::from(out)
}

pub fn draw(frame: &mut Frame, area: Rect, realm: &RealmState) {
    let Some(board) = realm.board.as_ref() else {
        return;
    };
    if area.height < 4 {
        return;
    }
    let header = Rect::new(area.x, area.y, area.width, 1);
    let body = Rect::new(area.x, area.y + 1, area.width, area.height - 2);
    let footer = Rect::new(area.x, area.y + area.height - 1, area.width, 1);

    // Header.
    let dim = Style::default().fg(theme::TEXT_DIM());
    let bright = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let role = realm.viewer_role();
    let mut spans: Vec<Span> = Vec::new();
    match board.detail.as_ref() {
        Some(detail) => {
            let state = &detail.state;
            spans.push(Span::styled(format!("{} ", detail.name), bright));
            spans.push(Span::styled(
                format!(
                    "· {} {} · {} · ",
                    state.ruleset.pace_name,
                    state.ruleset.display_name,
                    map_label(detail)
                ),
                Style::default().fg(theme::TEXT_DIM()),
            ));
            match detail.row_status.as_str() {
                // Frozen while it musters: no points to report and no refill
                // to wait for, only the clock everyone is waiting on.
                "active" if detail.muster_left().is_some() => {
                    let left = detail.muster_left().unwrap_or_default();
                    spans.push(Span::styled(
                        format!(
                            "mustering · opens in {} · {} players · look around",
                            super::svc::countdown_label(left),
                            state.players.len()
                        ),
                        bright,
                    ));
                }
                "active" if role.acts() => {
                    // Actions land immediately, so the clock that matters is
                    // when this game's points come back.
                    let points = points_left(board, realm.user_id)
                        .map(|(left, cap)| format!("{left}/{cap} points"))
                        .unwrap_or_default();
                    let secs = (next_reset(Utc::now(), detail.reset_hour_utc) - Utc::now())
                        .num_seconds()
                        .max(0);
                    spans.push(Span::styled(
                        format!(
                            "{points} · refill in {:02}:{:02}:{:02} ({})",
                            secs / 3600,
                            (secs % 3600) / 60,
                            secs % 60,
                            reset_hour_label(detail.reset_hour_utc, realm.viewer_tz)
                        ),
                        dim,
                    ));
                }
                // A visitor has no points and no refill to wait for; tell
                // them what they are looking at instead.
                "active" => {
                    let map_size = detail.map().map(|m| m.territories.len()).unwrap_or(0);
                    spans.push(Span::styled(
                        format!(
                            "{} · {} players · {}/{} claimed",
                            role.label(),
                            state.players.len(),
                            state.ownership.len(),
                            map_size
                        ),
                        dim,
                    ));
                }
                "finished" => {
                    let winner = detail
                        .winner_user_id
                        .and_then(|w| state.player(w))
                        .map(|p| p.username.clone());
                    spans.push(Span::styled(
                        match winner {
                            Some(name) => format!("finished — {name} rules the realm"),
                            None => "finished — the realm dissolved".to_string(),
                        },
                        bright,
                    ));
                }
                other => {
                    spans.push(Span::styled(other.to_string(), dim));
                }
            }
        }
        None => {
            spans.push(Span::styled("Realm · loading…", dim));
        }
    }
    // A one-row header cannot wrap, so it is trimmed to the space instead of
    // being clipped mid-word by the terminal.
    frame.render_widget(
        Paragraph::new(fit_line(spans, header.width as usize)),
        header,
    );

    // Body.
    if let Some(error) = board.load_error.as_ref() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("could not load the game: {error}"),
                Style::default().fg(theme::ERROR()),
            )))
            .wrap(Wrap { trim: false }),
            body,
        );
    } else if board.detail.is_some() {
        match board.view {
            RealmView::Map => draw_map_view(frame, body, realm),
            RealmView::Targets => draw_targets_view(frame, body, realm),
            RealmView::Overview => draw_overview_view(frame, body, realm),
            RealmView::Log => draw_log_view(frame, body, realm),
            RealmView::History => super::history_ui::draw(frame, body, realm),
        }
    }

    // The result goes over the top of everything: it is the one thing on
    // this screen that someone might have been waiting weeks for.
    if board.results_open {
        draw_results(frame, area, realm);
    }

    // Footer.
    let hints = if role.acts() {
        match board.view {
            RealmView::Map => {
                "arrows/drag pan · +/- zoom · a attack · f dig in · n next of yours · tab view · q back"
                    .to_string()
            }
            RealmView::Targets => {
                "j/k move · enter/a attack · f dig in · g show on map · tab view · q back".to_string()
            }
            RealmView::Overview => {
                "j/k walk your land · g show on map · tab view · q back".to_string()
            }
            RealmView::Log => "j/k scroll · tab view · q back".to_string(),
            RealmView::History => {
                "[ ] or ←/→ walk the days · g/G first/last · tab view · q back".to_string()
            }
        }
    } else if board
        .detail
        .as_ref()
        .is_some_and(|d| d.row_status == "finished")
    {
        "r results · arrows/drag pan · tab view · q back".to_string()
    } else {
        // Nothing here acts, so nothing here offers to.
        match board.view {
            RealmView::Map => {
                "watching · arrows/drag pan · +/- zoom · tab view · q back".to_string()
            }
            RealmView::Targets => {
                "watching · j/k move · g show on map · tab view · q back".to_string()
            }
            RealmView::Overview | RealmView::Log => {
                "watching · j/k scroll · tab view · q back".to_string()
            }
            RealmView::History => {
                "[ ] or ←/→ walk the days · g/G first/last · tab view · q back".to_string()
            }
        }
    };
    frame.render_widget(
        Paragraph::new(fit_line(
            vec![Span::styled(
                hints,
                Style::default().fg(theme::TEXT_FAINT()),
            )],
            footer.width as usize,
        )),
        footer,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(parts: &[&str]) -> Vec<Span<'static>> {
        parts.iter().map(|p| Span::raw((*p).to_string())).collect()
    }

    fn rendered(line: &Line<'static>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn a_line_that_fits_is_left_alone() {
        let line = fit_line(spans(&["EU 2 ", "· Slow Standard"]), 40);
        assert_eq!(rendered(&line), "EU 2 · Slow Standard");
    }

    #[test]
    fn a_line_that_overruns_is_cut_with_an_ellipsis() {
        let line = fit_line(
            spans(&["EU 2 ", "· Slow Standard · Earth · 4/7 points"]),
            20,
        );
        let text = rendered(&line);
        assert!(text.chars().count() <= 20, "{text:?} is still too wide");
        assert!(text.ends_with('…'), "{text:?} should show it was cut");
        assert!(text.starts_with("EU 2 "), "the front survives: {text:?}");
    }

    #[test]
    fn cutting_never_splits_a_wide_glyph() {
        // Box-drawing and CJK are two columns each; the cut has to land
        // between them, not inside one.
        let line = fit_line(spans(&["日本語のテキスト"]), 7);
        let text = rendered(&line);
        assert!(text.ends_with('…'));
        assert!(
            unicode_width::UnicodeWidthStr::width(text.as_str()) <= 7,
            "{text:?} overflows its 7 columns"
        );
    }

    #[test]
    fn a_pane_with_no_room_does_not_panic() {
        // Terminals can hand us a zero-width area mid-resize.
        let line = fit_line(spans(&["anything"]), 0);
        assert_eq!(rendered(&line), "anything", "nothing to trim against");
        let line = fit_line(spans(&["anything at all"]), 1);
        assert!(unicode_width::UnicodeWidthStr::width(rendered(&line).as_str()) <= 1);
    }
}
