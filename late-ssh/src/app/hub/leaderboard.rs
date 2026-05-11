use late_core::models::leaderboard::{HighScoreEntry, LeaderboardData, RankedEntry};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::common::theme;

const TOP_LIMIT_RANKED: usize = 10;
const TOP_LIMIT_SCORE: usize = 5;

pub fn draw(frame: &mut Frame, area: Rect, data: &LeaderboardData, user_id: Uuid) {
    // The 124x40 modal gives us a body of ~33 rows. We split into two equal
    // rows of boards: chips/arcade up top (top 10 each), score games at the
    // bottom (monthly top 5 + all-time top 5 stacked vertically per game).
    let rows = Layout::vertical([
        Constraint::Percentage(50), // chips + arcade
        Constraint::Length(1),      // breathing
        Constraint::Min(15),        // score games
    ])
    .split(area);

    draw_top_row(frame, rows[0], data, user_id);
    draw_score_row(frame, rows[2], data, user_id);
}

fn draw_top_row(frame: &mut Frame, area: Rect, data: &LeaderboardData, user_id: Uuid) {
    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
    draw_ranked_panel(
        frame,
        columns[0],
        user_id,
        RankedBoardView {
            title: "Top Chips",
            unit: "chips",
            entries: &data.monthly_chip_earners,
            empty: "no chip earnings yet this month",
            hint: "from daily puzzles · poker/blackjack pots",
        },
    );
    draw_ranked_panel(
        frame,
        columns[1],
        user_id,
        RankedBoardView {
            title: "Arcade Wins",
            unit: "pts",
            entries: &data.arcade_champions,
            empty: "no daily puzzle wins yet this month",
            hint: "daily puzzles · easy 1 · medium 3 · hard 5",
        },
    );
}

fn draw_score_row(frame: &mut Frame, area: Rect, data: &LeaderboardData, user_id: Uuid) {
    let columns = Layout::horizontal([
        Constraint::Percentage(34),
        Constraint::Percentage(33),
        Constraint::Percentage(33),
    ])
    .split(area);
    draw_score_panel(
        frame,
        columns[0],
        "Tetris",
        &data.monthly_tetris_high_scores,
        high_scores_for(data, "Tetris"),
        user_id,
    );
    draw_score_panel(
        frame,
        columns[1],
        "2048",
        &data.monthly_2048_high_scores,
        high_scores_for(data, "2048"),
        user_id,
    );
    draw_score_panel(
        frame,
        columns[2],
        "Snake",
        &data.monthly_snake_high_scores,
        high_scores_for(data, "Snake"),
        user_id,
    );
}

fn high_scores_for<'a>(data: &'a LeaderboardData, game: &str) -> Vec<&'a HighScoreEntry> {
    data.high_scores
        .iter()
        .filter(|entry| entry.game == game)
        .collect()
}

struct RankedBoardView<'a> {
    title: &'a str,
    unit: &'a str,
    entries: &'a [RankedEntry],
    empty: &'a str,
    hint: &'a str,
}

fn draw_ranked_panel(frame: &mut Frame, area: Rect, user_id: Uuid, view: RankedBoardView<'_>) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let sections = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // hint
        Constraint::Length(1), // breathing
        Constraint::Min(1),    // entries
    ])
    .split(area);

    frame.render_widget(Paragraph::new(section_heading(view.title)), sections[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(view.hint.to_string(), Style::default().fg(theme::TEXT_DIM())),
        ])),
        sections[1],
    );

    let body = sections[3];
    let width = body.width as usize;
    if view.entries.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    view.empty.to_string(),
                    Style::default().fg(theme::TEXT_FAINT()),
                ),
            ])),
            body,
        );
        return;
    }

    let user_rank = view
        .entries
        .iter()
        .position(|entry| entry.user_id == user_id);
    let rows: Vec<RankedRow> = view
        .entries
        .iter()
        .map(|entry| RankedRow {
            rank: entry.rank,
            username: entry.username.clone(),
            value: entry.value,
            user_id: entry.user_id,
        })
        .collect();
    let lines = ranked_lines_from_rows(
        &rows,
        view.unit,
        user_id,
        user_rank,
        body.height as usize,
        width,
        TOP_LIMIT_RANKED,
    );
    frame.render_widget(Paragraph::new(lines), body);
}

fn draw_score_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    monthly: &[HighScoreEntry],
    all_time: Vec<&HighScoreEntry>,
    user_id: Uuid,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let sections = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // hint
        Constraint::Length(1), // breathing
        Constraint::Length(6), // monthly: subtitle + up to 5 rows
        Constraint::Length(1), // breathing
        Constraint::Min(6),    // all-time: subtitle + up to 5 rows
    ])
    .split(area);

    frame.render_widget(Paragraph::new(section_heading(title)), sections[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "monthly resets the 1st · all-time persists",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        sections[1],
    );

    draw_score_list(
        frame,
        sections[3],
        "monthly",
        monthly.iter().collect(),
        user_id,
    );
    draw_score_list(frame, sections[5], "all-time", all_time, user_id);
}

fn draw_score_list(
    frame: &mut Frame,
    area: Rect,
    sub_title: &str,
    entries: Vec<&HighScoreEntry>,
    user_id: Uuid,
) {
    if area.height == 0 {
        return;
    }
    let width = area.width as usize;

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(area.height as usize);
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            sub_title.to_string(),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    let body_room = (area.height as usize).saturating_sub(1);
    if body_room == 0 {
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    if entries.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "no scores yet".to_string(),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ]));
    } else {
        let rows: Vec<RankedRow> = entries
            .iter()
            .map(|entry| RankedRow {
                rank: entry.rank,
                username: entry.username.clone(),
                value: i64::from(entry.score),
                user_id: entry.user_id,
            })
            .collect();
        let user_rank = rows.iter().position(|row| row.user_id == user_id);
        lines.extend(ranked_lines_from_rows(
            &rows,
            "",
            user_id,
            user_rank,
            body_room,
            width,
            TOP_LIMIT_SCORE,
        ));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

#[derive(Clone)]
struct RankedRow {
    rank: i64,
    username: String,
    value: i64,
    user_id: Uuid,
}

/// Layout note: shows the first `top_limit` rows. If the current user is
/// ranked below that, append a `…` divider plus their row so they always
/// see where they stand without losing the top of the board. The function
/// reduces `top_count` automatically when the height budget can't fit both
/// the full top list and the user tail.
fn ranked_lines_from_rows(
    rows: &[RankedRow],
    unit: &str,
    user_id: Uuid,
    user_index: Option<usize>,
    height: usize,
    width: usize,
    top_limit: usize,
) -> Vec<Line<'static>> {
    if rows.is_empty() || height == 0 {
        return Vec::new();
    }

    let needs_user_tail = match user_index {
        Some(idx) => idx >= top_limit,
        None => false,
    };
    let tail_cost = if needs_user_tail { 2 } else { 0 };
    let top_count = top_limit.min(rows.len()).min(height.saturating_sub(tail_cost));

    let mut lines = Vec::with_capacity(height);
    for row in rows.iter().take(top_count) {
        let is_user = row.user_id == user_id;
        lines.push(ranked_line(row, unit, is_user, width));
    }

    if needs_user_tail && lines.len() + 2 <= height {
        lines.push(divider_line(width));
        if let Some(idx) = user_index {
            let row = &rows[idx];
            lines.push(ranked_line(row, unit, true, width));
        }
    }

    lines
}

fn divider_line(width: usize) -> Line<'static> {
    let dots = "  …";
    let pad = width.saturating_sub(dots.chars().count());
    Line::from(vec![
        Span::styled(
            dots.to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::raw(" ".repeat(pad)),
    ])
}

fn ranked_line(row: &RankedRow, unit: &str, is_current_user: bool, width: usize) -> Line<'static> {
    let marker = if is_current_user { "›" } else { " " };
    let rank_text = format!("#{:<3}", row.rank);
    let value_text = if unit.is_empty() {
        format_number(row.value)
    } else {
        format!("{} {}", format_number(row.value), unit)
    };

    let prefix = format!(" {marker} ");
    let prefix_style = if is_current_user {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else if row.rank == 1 {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let rank_style = if is_current_user {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else if row.rank == 1 {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let name_style = if is_current_user {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else if row.rank == 1 {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let value_style = if is_current_user {
        Style::default()
            .fg(theme::SUCCESS())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::SUCCESS())
    };
    let trailing_style = if is_current_user {
        Style::default().bg(theme::BG_SELECTION())
    } else {
        Style::default()
    };

    let prefix_w = prefix.chars().count();
    let rank_w = rank_text.chars().count();
    let value_w = value_text.chars().count();
    let gutter = 1;
    let used_fixed = prefix_w + rank_w + gutter + value_w + gutter;
    let name_room = width.saturating_sub(used_fixed).max(3);
    let truncated = truncate(&row.username, name_room);
    let name_pad = name_room.saturating_sub(truncated.chars().count());

    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(rank_text, rank_style),
        Span::styled(" ", trailing_style),
        Span::styled(truncated, name_style),
        Span::styled(" ".repeat(name_pad + gutter), trailing_style),
        Span::styled(value_text, value_style),
    ])
}

fn section_heading(title: &str) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled("  ── ", dim),
        Span::styled(title.to_string(), accent),
        Span::styled(" ──", dim),
    ])
}

fn truncate(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }
    if max_chars <= 1 {
        return value.chars().take(max_chars).collect();
    }
    let mut out: String = value.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

fn format_number(value: i64) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let abs = value.unsigned_abs();
    let digits = abs.to_string();
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    format!("{sign}{out}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(rank: i64, name: &str, value: i64) -> RankedRow {
        RankedRow {
            rank,
            username: name.to_string(),
            value,
            user_id: Uuid::nil(),
        }
    }

    fn user_row(rank: i64, name: &str, value: i64, id: Uuid) -> RankedRow {
        RankedRow {
            rank,
            username: name.to_string(),
            value,
            user_id: id,
        }
    }

    #[test]
    fn top_visible_users_render_top_only() {
        let me = Uuid::now_v7();
        let rows = vec![
            user_row(1, "alice", 1000, me),
            row(2, "bob", 800),
            row(3, "carol", 600),
        ];
        let lines = ranked_lines_from_rows(&rows, "chips", me, Some(0), 12, 40, 10);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn deep_rank_appends_divider_and_user() {
        let me = Uuid::now_v7();
        let mut rows: Vec<RankedRow> = (1..=50)
            .map(|n| row(n, &format!("u{n}"), 1000 - n * 10))
            .collect();
        rows.push(user_row(51, "me", 100, me));
        let lines = ranked_lines_from_rows(&rows, "chips", me, Some(50), 14, 40, 10);
        // 10 top rows + divider + me
        assert_eq!(lines.len(), 12);
    }

    #[test]
    fn no_user_no_tail() {
        let nobody = Uuid::now_v7();
        let rows: Vec<RankedRow> = (1..=12).map(|n| row(n, &format!("u{n}"), 100)).collect();
        let lines = ranked_lines_from_rows(&rows, "chips", nobody, None, 12, 40, 10);
        assert_eq!(lines.len(), 10);
    }

    #[test]
    fn tight_budget_keeps_tail_visible() {
        // Even a 3-row budget reserves room for divider + you so a low-rank
        // user always sees where they stand.
        let me = Uuid::now_v7();
        let mut rows: Vec<RankedRow> = (1..=50)
            .map(|n| row(n, &format!("u{n}"), 100))
            .collect();
        rows.push(user_row(51, "me", 1, me));
        let lines = ranked_lines_from_rows(&rows, "chips", me, Some(50), 3, 40, 10);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn score_panel_top_five_fits_six_row_budget() {
        let me = Uuid::now_v7();
        let rows: Vec<RankedRow> = (1..=8)
            .map(|n| {
                if n == 7 {
                    user_row(n, "me", 100, me)
                } else {
                    row(n, &format!("u{n}"), 1000 - n * 10)
                }
            })
            .collect();
        // Budget 5 entries; user at index 6 (rank 7) is outside top 5 → tail.
        let lines = ranked_lines_from_rows(&rows, "", me, Some(6), 5, 30, 5);
        // 3 top + divider + me = 5
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn format_number_thousands() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(12_345_678), "12,345,678");
        assert_eq!(format_number(-1_234), "-1,234");
    }

    #[test]
    fn truncate_uses_ellipsis() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
        assert_eq!(truncate("abc", 3), "abc");
    }
}
