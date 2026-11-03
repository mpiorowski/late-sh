//! `Screen::Realm` renderer: one-line header (game, the season's clock,
//! points, when night falls), the active view, the order prompt over it
//! while one is being written, the world's chat in a right column while it
//! is up (in place of the map's rail), and the pinned key-hint footer.

use chrono::Utc;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::app::chat::ui::{EmbeddedRoomChatView, draw_embedded_room_chat};
use crate::app::common::theme;
use crate::app::files::terminal_image::TerminalImageFrame;

use super::log_ui::draw_log_view;
use super::map_ui::{RAIL_WIDTH, draw_map_view};
use super::overview_ui::draw_overview_view;
use super::results_ui::draw_results;
use super::state::{RealmState, RealmView, points_left};
use super::svc::{next_reset, reset_hour_label};
use super::targets_ui::draw_targets_view;

/// "day 12 of 28" on a game with a season clock.
fn season_label(state: &super::resolver::RealmGameState, today: i32) -> Option<String> {
    super::night::season_last_day(state).map(|_| {
        format!(
            "day {} of {}",
            super::night::season_day(state, today),
            state.ruleset.season_days
        )
    })
}

/// The world in the board header. A generated one has no name anybody knows,
/// so it says what it is instead: "Uncharted · 5 continents · 8 islands · 300
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

/// The world's chat as the board gets it from the app, which owns the chat
/// state the pane is drawn from.
pub(crate) enum RealmChatPane<'a> {
    /// Nothing to talk in yet: the board is loading, or the world was opened
    /// before rooms were born with it and the sweeper has not caught up.
    Absent,
    /// The room is there and put away; the frame title counts what was
    /// said since (`render.rs`, `F4 chat (N new)`).
    Hidden,
    /// Boxed: the view dwarfs the other two variants, and the pane is built
    /// once a frame.
    Shown(Box<EmbeddedRoomChatView<'a>>),
}

/// The chat column's width: a third of the screen, never narrower than the
/// rail it stands in for, never so wide the map turns into a strip.
fn chat_width(body_width: u16) -> u16 {
    (body_width / 3).clamp(RAIL_WIDTH, 64)
}

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    realm: &RealmState,
    chat: RealmChatPane<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    let Some(board) = realm.board.as_ref() else {
        return;
    };
    if area.height < 4 {
        return;
    }
    let header = Rect::new(area.x, area.y, area.width, 1);
    let full_body = Rect::new(area.x, area.y + 1, area.width, area.height - 2);
    let footer = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
    // The chat takes the right column on every view; on the map it stands
    // where the rail was. Too narrow for both, and the chat has the body:
    // it was asked for, the map was not.
    let (body, chat_area, chat_view) = match chat {
        RealmChatPane::Shown(view) => {
            let width = chat_width(full_body.width);
            match full_body.width > width + 40 {
                true => (
                    Rect::new(
                        full_body.x,
                        full_body.y,
                        full_body.width - width - 1,
                        full_body.height,
                    ),
                    Some(Rect::new(
                        full_body.x + full_body.width - width,
                        full_body.y,
                        width,
                        full_body.height,
                    )),
                    Some(view),
                ),
                false => (Rect::default(), Some(full_body), Some(view)),
            }
        }
        RealmChatPane::Absent | RealmChatPane::Hidden => (full_body, None, None),
    };
    let rail = chat_area.is_none();
    // The map sets it again if it is drawn; a frame where the chat has the
    // whole body must not leave the last map rect taking the clicks.
    board.map_geometry.set(None);

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
                    // The clock that matters is the night: it resolves the
                    // orders and brings the points back.
                    let clock = season_label(state, detail.today())
                        .map(|label| format!("{label} · "))
                        .unwrap_or_default();
                    let points = points_left(board, realm.user_id)
                        .map(|(left, cap)| format!("{left}/{cap} pts left"))
                        .unwrap_or_default();
                    let secs = (next_reset(Utc::now(), detail.reset_hour_utc) - Utc::now())
                        .num_seconds()
                        .max(0);
                    spans.push(Span::styled(
                        format!(
                            "{clock}{points} · night falls in {:02}:{:02}:{:02} ({})",
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
                    let clock = season_label(state, detail.today())
                        .map(|label| format!(" · {label}"))
                        .unwrap_or_default();
                    spans.push(Span::styled(
                        format!(
                            "{}{clock} · {} players · {}/{} claimed",
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
    } else if board.detail.is_some() && !body.is_empty() {
        match board.view {
            RealmView::Map => draw_map_view(frame, body, realm, rail),
            RealmView::Targets => draw_targets_view(frame, body, realm),
            RealmView::Orders => super::orders_ui::draw_orders_view(frame, body, realm),
            RealmView::Overview => draw_overview_view(frame, body, realm),
            RealmView::Log => draw_log_view(frame, body, realm),
            RealmView::History => super::history_ui::draw(frame, body, realm),
        }
    }

    if let (Some(chat_area), Some(view)) = (chat_area, chat_view) {
        // The column between the view and the chat gets a rule, so the
        // talk does not read as part of the map it sits against.
        if chat_area.x > full_body.x {
            let rule = Rect::new(chat_area.x - 1, chat_area.y, 1, chat_area.height);
            frame.render_widget(
                Paragraph::new(
                    (0..rule.height)
                        .map(|_| {
                            Line::from(Span::styled("│", Style::default().fg(theme::BORDER())))
                        })
                        .collect::<Vec<_>>(),
                ),
                rule,
            );
        }
        draw_embedded_room_chat(frame, chat_area, *view, terminal_images);
    }

    // The order being written goes over whatever view it was opened from,
    // where the map's rail stands.
    if board.prompt.is_some() && !body.is_empty() {
        super::orders_ui::draw_prompt(frame, body, realm);
    }

    // The result goes over the top of everything: it is the one thing on
    // this screen that someone might have been waiting weeks for.
    if board.results_open {
        draw_results(frame, area, realm);
    }
    // And the way in goes over the map, once a visitor has asked for it.
    if let Some(draft) = realm.join_draft.as_ref() {
        super::join_ui::draw_join_overlay(frame, area, draft);
    }

    // Footer.
    let history_hints = match board.replay {
        Some(_) => "p play/hold · . , step the night · esc stop · [ ] walk the days".to_string(),
        None if realm.replay_available() => {
            "p play the night · [ ] or ←/→ walk the days · g/G first/last · tab view · q back"
                .to_string()
        }
        None => "[ ] or ←/→ walk the days · g/G first/last · tab view · q back".to_string(),
    };
    let hints = if board.prompt.is_some() {
        "+/- or 1-9 size it · enter place · esc cancel".to_string()
    } else if role.acts() {
        // The orders lead on every view: what is left to order is the
        // number a player acts on.
        let orders = points_left(board, realm.user_id)
            .map(|(left, _)| format!("o orders ({left} pts left) · "))
            .unwrap_or_default();
        let keys = match board.view {
            RealmView::Map => {
                "arrows/drag pan · +/- zoom · a take · f dig in · n next of yours · tab view · q back"
                    .to_string()
            }
            RealmView::Targets => {
                "j/k move · enter/a take · f dig in · g show on map · tab view · q back".to_string()
            }
            RealmView::Orders => {
                "j/k move · enter resize · x withdraw · g show on map · tab view · q back"
                    .to_string()
            }
            RealmView::Overview => {
                "j/k walk your land · g show on map · tab view · q back".to_string()
            }
            RealmView::Log => "j/k scroll · tab view · q back".to_string(),
            RealmView::History => history_hints,
        };
        format!("{orders}{keys}")
    } else if board
        .detail
        .as_ref()
        .is_some_and(|d| d.row_status == "finished")
    {
        "r results · arrows/drag pan · tab view · q back".to_string()
    } else {
        // Nothing here acts, so nothing here offers to, except the way in
        // while the gates are open.
        match board.view {
            RealmView::Map if realm.can_join() => {
                "j take your spot · arrows/drag pan · +/- zoom · tab view · q back".to_string()
            }
            RealmView::Map => {
                "watching · arrows/drag pan · +/- zoom · tab view · q back".to_string()
            }
            RealmView::Targets => {
                "watching · j/k move · g show on map · tab view · q back".to_string()
            }
            RealmView::Orders => {
                "watching · the night's fights · the log has the rest · tab view · q back"
                    .to_string()
            }
            RealmView::Overview | RealmView::Log => {
                "watching · j/k scroll · tab view · q back".to_string()
            }
            RealmView::History => history_hints,
        }
    };
    let faint = Style::default().fg(theme::TEXT_FAINT());
    // While the chat is up its keys lead, so a narrow footer cuts the
    // view's keys first. Put away, the frame title names F4 and counts what
    // is new, as on every game with a chat.
    let mut footer_spans = Vec::new();
    if realm.shown_chat_room_id().is_some() {
        footer_spans.push(Span::styled("F4 hide chat · i type · ", faint));
    }
    footer_spans.push(Span::styled(hints, faint));
    frame.render_widget(
        Paragraph::new(fit_line(footer_spans, footer.width as usize)),
        footer,
    );
}

/// `text` cut to `width` characters, an ellipsis standing in for the rest.
/// Every realm renderer cuts names and labels the same way.
pub(crate) fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let keep: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{keep}\u{2026}")
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
