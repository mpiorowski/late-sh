//! Rendering for the World Cup screen.
//!
//! Two compact, colorful views: an **Overview** (live/upcoming matches beside
//! group standings) and a **Bracket** (the knockout rounds). The active view
//! and its scroll come from [`State`]; the data comes from the service
//! snapshot. Everything degrades gracefully — an empty snapshot shows a
//! friendly placeholder rather than a blank screen.

use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::flags::flag_emoji;
use super::model::{BracketRound, Group, Match, Matchup, Qual, Winner, WorldCupSnapshot};
use super::state::{State, View};
use crate::app::common::theme;

/// Everything the screen needs to render, borrowed from `App` at draw time.
pub struct WorldCupView<'a> {
    pub snapshot: &'a WorldCupSnapshot,
    pub state: &'a State,
    /// Honour the account's flag-emoji tweak: `false` suppresses every flag on
    /// the screen for terminal/font stacks that render regional-indicator pairs
    /// as boxed letters (mirrors the chat/shop `show_flag_fallback` setting).
    pub show_flags: bool,
    /// Client is kitty, which splits regional-indicator flags in the overview's
    /// rightmost (bracket) column — that one column drops flags for kitty while
    /// keeping them everywhere else.
    pub terminal_is_kitty: bool,
}

const LIVE: Color = Color::Rgb(255, 92, 92);

pub fn draw(frame: &mut Frame, area: Rect, view: WorldCupView) {
    if view.snapshot.is_empty() {
        draw_placeholder(frame, area, view.snapshot.stale);
        return;
    }

    let [bar_area, body_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    draw_phase_bar(frame, bar_area, &view);

    match view.state.view {
        View::Overview => draw_overview(
            frame,
            body_area,
            view.snapshot,
            view.state,
            view.show_flags,
            view.terminal_is_kitty,
        ),
        // The dedicated bracket has no panel to its left, so kitty renders its
        // flags fine — only `show_flags` gates them here.
        View::Bracket => draw_bracket(frame, body_area, view.snapshot, view.state, view.show_flags),
    }
}

fn draw_placeholder(frame: &mut Frame, area: Rect, stale: bool) {
    let msg = if stale {
        "World Cup data is currently unavailable — retrying…"
    } else {
        "Loading World Cup…"
    };
    let lines = vec![
        Line::from(Span::styled(
            "⚽ FIFA World Cup",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(msg, Style::default().fg(theme::TEXT_DIM()))),
    ];
    let para = Paragraph::new(lines).alignment(Alignment::Center);
    // Vertically center-ish by offsetting into the area.
    let [_, mid, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .areas(area);
    frame.render_widget(para, mid);
}

fn draw_phase_bar(frame: &mut Frame, area: Rect, view: &WorldCupView) {
    let snap = view.snapshot;
    let title = if snap.season.is_empty() {
        "⚽ FIFA World Cup".to_string()
    } else {
        format!("⚽ FIFA World Cup {}", snap.season)
    };

    let mut spans = vec![
        Span::styled(
            title,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("   ", Style::default()),
    ];

    spans.push(tab_span("Overview", view.state.view == View::Overview));
    spans.push(Span::styled(" / ", Style::default().fg(theme::TEXT_DIM())));
    spans.push(tab_span("Bracket", view.state.view == View::Bracket));
    spans.push(Span::styled(
        "  (Space)",
        Style::default().fg(theme::TEXT_FAINT()),
    ));

    if snap.stale {
        spans.push(Span::styled(
            "   ⚠ stale",
            Style::default().fg(theme::ERROR()),
        ));
    } else if let Some(label) = fresh_label(snap) {
        spans.push(Span::styled(
            format!("   updated {label}"),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn tab_span(label: &str, active: bool) -> Span<'static> {
    if active {
        Span::styled(
            label.to_string(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
    } else {
        Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM()))
    }
}

// ---- overview --------------------------------------------------------------

/// Below this width the overview is matches + groups only; at or above it
/// there's room to also park the knockout bracket on the right, so the user
/// gets both views at once without toggling.
const WIDE_OVERVIEW_MIN_WIDTH: u16 = 140;

/// Width reserved for the bracket column in the wide overview. It carries two
/// flag-emoji per tie, which render unreliably when crammed, so it gets a little
/// extra room over the bare content width.
const OVERVIEW_BRACKET_WIDTH: u16 = 36;

fn draw_overview(
    frame: &mut Frame,
    area: Rect,
    snap: &WorldCupSnapshot,
    state: &State,
    show_flags: bool,
    terminal_is_kitty: bool,
) {
    let show_bracket = area.width >= WIDE_OVERVIEW_MIN_WIDTH && !snap.bracket.is_empty();
    if show_bracket {
        let [left, mid, right] = Layout::horizontal([
            Constraint::Length(46),
            Constraint::Fill(1),
            Constraint::Length(OVERVIEW_BRACKET_WIDTH),
        ])
        .areas(area);
        draw_matches_panel(frame, left, snap, show_flags);
        draw_groups_panel(frame, mid, snap, state.overview_scroll, show_flags);
        // kitty splits flags here (rightmost column, downstream of the panels'
        // flags on shared rows), so it alone falls back to codes.
        draw_bracket(frame, right, snap, state, show_flags && !terminal_is_kitty);
    } else {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(44), Constraint::Fill(1)]).areas(area);
        draw_matches_panel(frame, left, snap, show_flags);
        draw_groups_panel(frame, right, snap, state.overview_scroll, show_flags);
    }
}

fn draw_matches_panel(frame: &mut Frame, area: Rect, snap: &WorldCupSnapshot, show_flags: bool) {
    let block = panel_block(" ⚽ Matches ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width;

    let mut lines: Vec<Line> = Vec::new();

    let live: Vec<&Match> = snap.live().collect();
    if !live.is_empty() {
        lines.push(section_header("● LIVE"));
        for m in live {
            lines.push(live_match_line(m, show_flags));
        }
        lines.push(Line::from(""));
    }

    let upcoming: Vec<&Match> = snap.upcoming().take(10).collect();
    if !upcoming.is_empty() {
        lines.push(section_header("UPCOMING"));
        // Group consecutive fixtures under a single day banner so each row
        // only needs the kick-off time, buying back horizontal room.
        let mut current_day: Option<String> = None;
        for m in upcoming {
            let day = kickoff_day(m);
            if day != current_day {
                if let Some(d) = &day {
                    lines.push(banner_line(d, width));
                }
                current_day = day;
            }
            lines.push(fill_bg(
                upcoming_match_line(m, show_flags),
                muted_row_bg(),
                width,
            ));
        }
        lines.push(Line::from(""));
    }

    let recent: Vec<&Match> = snap.recent_finished().take(6).collect();
    if !recent.is_empty() {
        lines.push(banner_line("RECENT RESULTS", width));
        for m in recent {
            lines.push(fill_bg(
                result_match_line(m, show_flags),
                muted_row_bg(),
                width,
            ));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matches to show.",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn live_match_line(m: &Match, show_flags: bool) -> Line<'static> {
    Line::from(vec![
        Span::styled("🔴 ", Style::default().fg(LIVE)),
        team_span(&m.home, false, show_flags),
        Span::styled(
            format!(" {} ", score_str(m)),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        team_span(&m.away, false, show_flags),
    ])
}

fn upcoming_match_line(m: &Match, show_flags: bool) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{:<6}", kickoff_time(m)),
            Style::default().fg(theme::AMBER_DIM()),
        ),
        team_span(&m.home, false, show_flags),
        Span::styled(" ᵥ ", Style::default().fg(theme::TEXT_DIM())),
        team_span(&m.away, false, show_flags),
    ])
}

/// A subtle fill one notch above the canvas, sitting under the day banner so
/// the fixtures grouped beneath it read as a single block. Derived by blending
/// the banner's highlight toward the canvas, so it tracks whatever palette is
/// active and is always quieter than the banner above it.
fn muted_row_bg() -> Color {
    blend(theme::BG_HIGHLIGHT(), theme::BG_CANVAS(), 0.55)
}

/// Linear blend `t` of the way from `a` to `b` (0.0 = `a`, 1.0 = `b`).
/// Falls back to `a` for non-RGB colors (the palette backgrounds are RGB).
fn blend(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
            Color::Rgb(mix(ar, br), mix(ag, bg), mix(ab, bb))
        }
        _ => a,
    }
}

/// Paint `bg` behind every span of `line` and extend it to the panel's right
/// edge. Trailing padding is over-provisioned and clipped by the paragraph, so
/// the fill is full-width regardless of double-width flag glyphs in `line`.
fn fill_bg(mut line: Line<'static>, bg: Color, width: u16) -> Line<'static> {
    for span in &mut line.spans {
        span.style = span.style.bg(bg);
    }
    line.spans.push(Span::styled(
        " ".repeat(width as usize),
        Style::default().bg(bg),
    ));
    line
}

/// A full-width banner with a subtle standoff background, e.g. "Jun 30" over a
/// day's fixtures or "RECENT RESULTS" over the results block.
fn banner_line(label: &str, width: u16) -> Line<'static> {
    let mut text = format!(" {label}");
    let w = width as usize;
    let len = text.chars().count();
    if len < w {
        text.push_str(&" ".repeat(w - len));
    }
    Line::from(Span::styled(
        text,
        Style::default()
            .fg(theme::AMBER())
            .bg(theme::BG_HIGHLIGHT())
            .add_modifier(Modifier::BOLD),
    ))
}

fn result_match_line(m: &Match, show_flags: bool) -> Line<'static> {
    let home_won = matches!((m.home_score, m.away_score), (Some(h), Some(a)) if h > a);
    let away_won = matches!((m.home_score, m.away_score), (Some(h), Some(a)) if a > h);
    Line::from(vec![
        team_span(&m.home, home_won, show_flags),
        Span::styled(
            format!(" {} ", score_str(m)),
            Style::default().fg(theme::TEXT()),
        ),
        team_span(&m.away, away_won, show_flags),
        Span::styled(
            format!(" {}", m.reason_short.as_deref().unwrap_or("FT")),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ])
}

fn team_span(name: &str, winner: bool, show_flags: bool) -> Span<'static> {
    let label = format!("{}{}", flag_prefix(name, show_flags), clip_name(name, 12));
    let mut style = Style::default().fg(theme::TEXT());
    if winner {
        style = style.fg(theme::TEXT_BRIGHT()).add_modifier(Modifier::BOLD);
    }
    Span::styled(label, style)
}

/// A team's flag emoji followed by a space, or an empty string when the viewer
/// has flag display turned off. Centralises the one place a flag prefixes a
/// label so every panel honours the tweak the same way.
fn flag_prefix(name: &str, show_flags: bool) -> String {
    if show_flags {
        format!("{} ", flag_emoji(name))
    } else {
        String::new()
    }
}

fn score_str(m: &Match) -> String {
    match (m.home_score, m.away_score) {
        (Some(h), Some(a)) => format!("{h}-{a}"),
        _ => "v".to_string(),
    }
}

fn kickoff_day(m: &Match) -> Option<String> {
    m.kickoff.map(|t| t.format("%b %d").to_string())
}

fn kickoff_time(m: &Match) -> String {
    m.kickoff
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_default()
}

// ---- groups ----------------------------------------------------------------

fn draw_groups_panel(
    frame: &mut Frame,
    area: Rect,
    snap: &WorldCupSnapshot,
    scroll: u16,
    show_flags: bool,
) {
    let block = panel_block(" 🏆 Groups ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if snap.groups.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "Group standings not available yet.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
            inner,
        );
        return;
    }

    // Two columns of groups so all twelve fit compactly.
    let [col_a, col_b] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Fill(1)]).areas(inner);
    let mid = snap.groups.len().div_ceil(2);
    let (left, right) = snap.groups.split_at(mid);

    frame.render_widget(
        Paragraph::new(groups_lines(left, show_flags)).scroll((scroll, 0)),
        col_a,
    );
    frame.render_widget(
        Paragraph::new(groups_lines(right, show_flags)).scroll((scroll, 0)),
        col_b,
    );
}

fn groups_lines(groups: &[Group], show_flags: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for g in groups {
        lines.push(Line::from(Span::styled(
            format!("Group {}", g.letter),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )));
        for (i, row) in g.rows.iter().enumerate() {
            lines.push(group_row_line(i + 1, row, show_flags));
        }
        lines.push(Line::from(""));
    }
    lines
}

fn group_row_line(pos: usize, row: &super::model::TeamRow, show_flags: bool) -> Line<'static> {
    let name = clip_name(&row.name, 12);
    let name_style = match row.qual {
        Qual::Direct => Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD),
        Qual::Playoff => Style::default().fg(theme::AMBER()),
        Qual::None => Style::default().fg(theme::TEXT_DIM()),
    };
    Line::from(vec![
        Span::styled(format!("{pos} "), Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(flag_prefix(&row.name, show_flags), Style::default()),
        Span::styled(format!("{name:<13} "), name_style),
        Span::styled(
            format!("{}pt", row.points),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:+}", row.goal_diff),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ])
}

// ---- bracket ---------------------------------------------------------------

fn draw_bracket(
    frame: &mut Frame,
    area: Rect,
    snap: &WorldCupSnapshot,
    state: &State,
    show_flags: bool,
) {
    let block = panel_block(" 🏆 Knockout Bracket ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if snap.bracket.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "The knockout bracket isn't set yet — check back after the group stage.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
            inner,
        );
        return;
    }

    let mut lines = Vec::new();
    for round in &snap.bracket {
        lines.extend(round_lines(round, show_flags));
    }
    frame.render_widget(
        Paragraph::new(lines).scroll((state.bracket_scroll, 0)),
        inner,
    );
}

fn round_lines(round: &BracketRound, show_flags: bool) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        format!("── {} ──", round.label),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))];
    for mu in &round.matchups {
        lines.push(matchup_line(mu, show_flags));
    }
    lines.push(Line::from(""));
    lines
}

fn matchup_line(mu: &Matchup, show_flags: bool) -> Line<'static> {
    if mu.tbd {
        return Line::from(Span::styled(
            format!(
                "  {} v {}",
                code_or(&mu.home_short),
                code_or(&mu.away_short)
            ),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    Line::from(vec![
        Span::styled("  ", Style::default()),
        bracket_team_span(
            &mu.home_name,
            &mu.home_short,
            mu.winner == Winner::Home,
            show_flags,
        ),
        Span::styled(
            format!(" {} ", bracket_score(mu)),
            Style::default().fg(theme::TEXT()),
        ),
        bracket_team_span(
            &mu.away_name,
            &mu.away_short,
            mu.winner == Winner::Away,
            show_flags,
        ),
    ])
}

// The flag emoji is built into the same span as the 3-letter code (one grapheme
// then a space then ASCII), matching the matches/groups panes that render flags
// reliably. The bracket column is widened a little (see `OVERVIEW_BRACKET_WIDTH`)
// so the two regional-indicator pairs per line aren't crammed.
fn bracket_team_span(name: &str, short: &str, winner: bool, show_flags: bool) -> Span<'static> {
    let label = format!("{}{}", flag_prefix(name, show_flags), code_or(short));
    let mut style = Style::default().fg(theme::TEXT());
    if winner {
        style = style.fg(theme::SUCCESS()).add_modifier(Modifier::BOLD);
    }
    Span::styled(label, style)
}

fn bracket_score(mu: &Matchup) -> String {
    match (mu.home_score, mu.away_score) {
        (Some(h), Some(a)) => format!("{h}-{a}"),
        _ => "v".to_string(),
    }
}

fn code_or(short: &str) -> String {
    if short.trim().is_empty() {
        "TBD".to_string()
    } else {
        short.to_string()
    }
}

// ---- shared helpers --------------------------------------------------------

fn panel_block(title: &'static str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()))
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ))
}

fn section_header(label: &str) -> Line<'static> {
    Line::from(Span::styled(
        label.to_string(),
        Style::default()
            .fg(theme::TEXT_DIM())
            .add_modifier(Modifier::BOLD),
    ))
}

fn fresh_label(snap: &WorldCupSnapshot) -> Option<String> {
    let fetched = snap.fetched_at?;
    let secs = (Utc::now() - fetched).num_seconds().max(0);
    Some(if secs < 60 {
        format!("{secs}s ago")
    } else {
        format!("{}m ago", secs / 60)
    })
}

/// Keep up to `max` characters of a team name, appending an ellipsis when it
/// was longer. A name of `max + 1` characters keeps all `max` and gains the
/// ellipsis (so the clipped form is `max + 1` columns wide).
fn clip_name(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max).collect();
        format!("{kept}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::worldcup::model::{Match, MatchStatus};

    #[test]
    fn score_str_shows_score_or_placeholder() {
        let finished = Match {
            home_score: Some(2),
            away_score: Some(1),
            status: MatchStatus::Finished,
            ..Default::default()
        };
        assert_eq!(score_str(&finished), "2-1");

        let upcoming = Match {
            status: MatchStatus::Upcoming,
            ..Default::default()
        };
        assert_eq!(score_str(&upcoming), "v");
    }

    #[test]
    fn clip_name_keeps_short_and_ellipsizes_long() {
        assert_eq!(clip_name("Spain", 12), "Spain");
        // Exactly 12 chars is kept whole; a 13th forces the ellipsis.
        assert_eq!(clip_name("Saudi Arabia", 12), "Saudi Arabia");
        assert_eq!(clip_name("Bosnia and Herzegovina", 12), "Bosnia and H…");
    }

    #[test]
    fn flag_prefix_honours_tweak() {
        assert_eq!(
            flag_prefix("Spain", true),
            format!("{} ", flag_emoji("Spain"))
        );
        assert_eq!(flag_prefix("Spain", false), "");
    }

    #[test]
    fn code_or_falls_back_to_tbd() {
        assert_eq!(code_or("GER"), "GER");
        assert_eq!(code_or("   "), "TBD");
    }

    #[test]
    fn kickoff_splits_into_day_and_time() {
        use chrono::TimeZone;
        let m = Match {
            kickoff: Some(Utc.with_ymd_and_hms(2026, 6, 30, 21, 0, 0).unwrap()),
            status: MatchStatus::Upcoming,
            ..Default::default()
        };
        assert_eq!(kickoff_day(&m), Some("Jun 30".to_string()));
        assert_eq!(kickoff_time(&m), "21:00");

        let tbd = Match::default();
        assert_eq!(kickoff_day(&tbd), None);
        assert_eq!(kickoff_time(&tbd), "");
    }

    #[test]
    fn banner_fills_to_width() {
        let line = banner_line("Jun 30", 20);
        assert_eq!(line.width(), 20);
    }
}
