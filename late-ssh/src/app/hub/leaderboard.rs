use late_core::models::leaderboard::{HighScoreEntry, LeaderboardData, RankedEntry};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::common::theme;

use super::state::HubTab;

const TOP_LIMIT_RANKED: usize = 10;
const TOP_LIMIT_SCORE: usize = 5;
const SCORE_MONTHLY_OFFSET: u16 = 2;
const SCORE_USER_TAIL_HEIGHT: u16 = 2;
const SCORE_MONTHLY_HEIGHT: u16 = 1 + TOP_LIMIT_SCORE as u16 + SCORE_USER_TAIL_HEIGHT;
const SCORE_ALL_TIME_OFFSET: u16 = SCORE_MONTHLY_OFFSET + SCORE_MONTHLY_HEIGHT;
const WIN_STREAK_GAP: u16 = 1;
const REFERENCE_SCORE_SPAN: u16 = 110;
const REFERENCE_SCORE_PANEL_SPANS: [u16; 5] = [23, 22, 22, 22, 21];

pub(crate) fn draw(
    frame: &mut Frame,
    popup: Rect,
    data: &LeaderboardData,
    user_id: Uuid,
    is_admin: bool,
) {
    let Some(grid) = DenseGrid::new(popup) else {
        return;
    };

    draw_top_row(frame, &grid, data, user_id);
    draw_score_row(frame, &grid, data, user_id);
    super::ui::draw_footer(frame, grid.footer_area(), HubTab::Leaderboard, is_admin);
    draw_grid(frame, &grid);
}

#[derive(Debug)]
struct DenseGrid {
    popup: Rect,
    content_top: u16,
    top_divider_y: u16,
    score_top: u16,
    lower_divider_y: u16,
    footer_y: u16,
    bottom_y: u16,
    top_dividers: Vec<u16>,
    score_dividers: [u16; 4],
}

impl DenseGrid {
    fn new(popup: Rect) -> Option<Self> {
        if popup.width < 12 || popup.height < 14 {
            return None;
        }

        let bottom_y = popup.bottom().saturating_sub(1);
        let footer_y = bottom_y.saturating_sub(1);
        let content_top = popup.y.saturating_add(4);
        if footer_y <= content_top.saturating_add(3) {
            return None;
        }
        // Bias the odd shared row toward the score grid. At the reference
        // height the top boards still fit exactly, while every monthly score
        // board gains room for five leaders plus the two-row current-user tail.
        let panel_rows = footer_y.saturating_sub(content_top).saturating_sub(2);
        let top_divider_y = content_top + panel_rows / 2;
        let score_top = top_divider_y.saturating_add(1);
        let lower_divider_y = footer_y.saturating_sub(1);

        let span = popup.width - 1;
        let top_offsets = if span >= 69 {
            vec![31, 57]
        } else {
            vec![span / 2]
        };
        let score_offsets = score_divider_offsets(span);

        Some(Self {
            popup,
            content_top,
            top_divider_y,
            score_top,
            lower_divider_y,
            footer_y,
            bottom_y,
            top_dividers: top_offsets
                .into_iter()
                .map(|offset| popup.x + offset)
                .collect(),
            score_dividers: score_offsets.map(|offset| popup.x + offset),
        })
    }

    fn top_panels(&self) -> Vec<Rect> {
        let mut boundaries = Vec::with_capacity(self.top_dividers.len() + 2);
        boundaries.push(self.popup.x);
        boundaries.extend(self.top_dividers.iter().copied());
        boundaries.push(self.popup.right() - 1);
        boundaries
            .windows(2)
            .map(|pair| {
                panel_between(
                    pair[0],
                    pair[1],
                    self.content_top,
                    self.top_divider_y - self.content_top,
                )
            })
            .collect()
    }

    fn score_panels(&self) -> [Rect; 5] {
        let boundaries = [
            self.popup.x,
            self.score_dividers[0],
            self.score_dividers[1],
            self.score_dividers[2],
            self.score_dividers[3],
            self.popup.right() - 1,
        ];
        std::array::from_fn(|index| {
            let bottom = if index < 3 {
                self.lower_divider_y
            } else {
                self.bottom_y
            };
            panel_between(
                boundaries[index],
                boundaries[index + 1],
                self.score_top,
                bottom.saturating_sub(self.score_top),
            )
        })
    }

    fn footer_area(&self) -> Rect {
        panel_between(self.popup.x, self.score_dividers[2], self.footer_y, 1)
    }
}

fn score_divider_offsets(span: u16) -> [u16; 4] {
    if span < REFERENCE_SCORE_SPAN {
        return [span / 5, span * 2 / 5, span * 3 / 5, span * 4 / 5];
    }

    // Each span includes its right-hand divider. Le Word keeps a 20-cell
    // interior at the reference width, leaving exactly the row formatter's
    // one trailing pad before the outer border. The five cells it formerly
    // left unused are shared across the first four games.
    let extra = span - REFERENCE_SCORE_SPAN;
    let shared = extra / 5;
    let remainder = extra % 5;
    let mut panel_spans = REFERENCE_SCORE_PANEL_SPANS;
    for (index, panel_span) in panel_spans.iter_mut().enumerate() {
        *panel_span += shared + u16::from(index < usize::from(remainder));
    }

    let mut offset = 0;
    std::array::from_fn(|index| {
        offset += panel_spans[index];
        offset
    })
}

fn panel_between(left: u16, right: u16, y: u16, height: u16) -> Rect {
    Rect::new(
        left.saturating_add(1),
        y,
        right.saturating_sub(left).saturating_sub(1),
        height,
    )
}

fn draw_top_row(frame: &mut Frame, grid: &DenseGrid, data: &LeaderboardData, user_id: Uuid) {
    let panels = grid.top_panels();
    if let Some(panel) = panels.first() {
        draw_ranked_panel(
            frame,
            *panel,
            user_id,
            RankedBoardView {
                title: "Top Chips",
                unit: "chips",
                entries: &data.monthly_chip_earners,
                empty: "no chip earnings yet this month",
                hints: &["monthly net chip delta", "shop spend ignored"],
                show_current_marker: true,
            },
        );
    }
    if let Some(panel) = panels.get(1) {
        draw_ranked_panel(
            frame,
            *panel,
            user_id,
            RankedBoardView {
                title: "Arcade Wins",
                unit: "pts",
                entries: &data.arcade_champions,
                empty: "no daily puzzle wins yet this month",
                hints: &["daily puzzle value:", "easy 1·medium 3·hard 5"],
                show_current_marker: false,
            },
        );
    }
    if let Some(panel) = panels.get(2) {
        render_line(
            frame,
            *panel,
            0,
            section_heading("More Leaderboards coming soon!"),
        );
    }
}

fn draw_score_row(frame: &mut Frame, grid: &DenseGrid, data: &LeaderboardData, user_id: Uuid) {
    let panels = grid.score_panels();
    draw_score_panel(
        frame,
        panels[0],
        "Lateris",
        &data.monthly_tetris_high_scores,
        high_scores_for(data, "Lateris"),
        user_id,
    );
    draw_score_panel(
        frame,
        panels[1],
        "2048",
        &data.monthly_2048_high_scores,
        high_scores_for(data, "2048"),
        user_id,
    );
    draw_score_panel(
        frame,
        panels[2],
        "Snake",
        &data.monthly_snake_high_scores,
        high_scores_for(data, "Snake"),
        user_id,
    );
    draw_score_panel(
        frame,
        panels[3],
        "Traffic",
        &data.monthly_traffic_high_scores,
        high_scores_for(data, "Traffic"),
        user_id,
    );
    draw_le_word_panel(frame, panels[4], data, user_id);
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
    hints: &'a [&'a str],
    show_current_marker: bool,
}

fn draw_ranked_panel(frame: &mut Frame, area: Rect, user_id: Uuid, view: RankedBoardView<'_>) {
    if area.is_empty() {
        return;
    }
    render_line(frame, area, 0, section_heading(view.title));
    for (index, hint) in view.hints.iter().enumerate() {
        render_line(
            frame,
            area,
            index as u16 + 1,
            Line::from(vec![
                Span::raw(" "),
                Span::styled((*hint).to_string(), Style::default().fg(theme::TEXT_DIM())),
            ]),
        );
    }

    let body_offset = 1_u16
        .saturating_add(view.hints.len() as u16)
        .saturating_add(1);
    if body_offset >= area.height {
        return;
    }
    let body = Rect::new(
        area.x,
        area.y + body_offset,
        area.width,
        area.height - body_offset,
    );
    if view.entries.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    view.empty.to_string(),
                    Style::default().fg(theme::TEXT_FAINT()),
                ),
            ])),
            body,
        );
        return;
    }

    let rows: Vec<RankedRow> = view.entries.iter().map(RankedRow::from).collect();
    let user_rank = rows.iter().position(|row| row.user_id == user_id);
    let lines = ranked_lines_from_rows(
        &rows,
        view.unit,
        user_id,
        user_rank,
        body.height as usize,
        body.width as usize,
        TOP_LIMIT_RANKED,
        view.show_current_marker,
        RowDensity::Top,
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
    if area.is_empty() {
        return;
    }
    render_line(frame, area, 0, section_heading(title));
    let monthly_rows: Vec<RankedRow> = monthly.iter().map(RankedRow::from).collect();
    let all_time_rows: Vec<RankedRow> = all_time.into_iter().map(RankedRow::from).collect();
    let content_width = score_card_content_width(
        usize::from(area.width),
        &[&monthly_rows, &all_time_rows],
        user_id,
    );

    if area.height > SCORE_MONTHLY_OFFSET {
        let monthly_area = monthly_score_area(area);
        draw_score_list(
            frame,
            monthly_area,
            "monthly",
            &monthly_rows,
            user_id,
            content_width,
        );
    }
    if area.height > SCORE_ALL_TIME_OFFSET {
        let all_time_area = Rect::new(
            area.x,
            area.y + SCORE_ALL_TIME_OFFSET,
            area.width,
            area.height - SCORE_ALL_TIME_OFFSET,
        );
        draw_score_list(
            frame,
            all_time_area,
            "all-time",
            &all_time_rows,
            user_id,
            content_width,
        );
    }
}

fn draw_le_word_panel(frame: &mut Frame, area: Rect, data: &LeaderboardData, user_id: Uuid) {
    if area.is_empty() {
        return;
    }
    render_line(frame, area, 0, section_heading("Le Word"));
    let monthly_rows: Vec<RankedRow> = data
        .monthly_le_word_wins
        .iter()
        .map(RankedRow::from)
        .collect();
    let all_time_rows: Vec<RankedRow> = data
        .all_time_le_word_wins
        .iter()
        .map(RankedRow::from)
        .collect();
    let win_streak_rows: Vec<RankedRow> = data
        .le_word_win_streaks
        .iter()
        .map(RankedRow::from)
        .collect();
    let monthly_area = monthly_score_area(area);
    let all_time_area =
        preferred_score_list_area(area, SCORE_ALL_TIME_OFFSET, &all_time_rows, user_id);
    let win_streak_area = win_streak_area(area, all_time_area.bottom(), win_streak_rows.len());
    let visible_win_streak_rows = win_streak_area
        .map(|section| usize::from(section.height.saturating_sub(1)))
        .unwrap_or_default();
    let content_width = score_card_content_width(
        usize::from(area.width),
        &[
            &monthly_rows,
            &all_time_rows,
            &win_streak_rows[..visible_win_streak_rows],
        ],
        user_id,
    );

    if area.height > SCORE_MONTHLY_OFFSET {
        draw_score_list(
            frame,
            monthly_area,
            "monthly",
            &monthly_rows,
            user_id,
            content_width,
        );
    }
    if area.height > SCORE_ALL_TIME_OFFSET {
        draw_score_list(
            frame,
            all_time_area,
            "all-time",
            &all_time_rows,
            user_id,
            content_width,
        );
    }
    if let Some(win_streak_area) = win_streak_area {
        draw_win_streak_list(
            frame,
            win_streak_area,
            &win_streak_rows,
            user_id,
            content_width,
        );
    }
}

fn monthly_score_area(area: Rect) -> Rect {
    Rect::new(
        area.x,
        area.y + SCORE_MONTHLY_OFFSET,
        area.width,
        area.height
            .saturating_sub(SCORE_MONTHLY_OFFSET)
            .min(SCORE_MONTHLY_HEIGHT),
    )
}

fn preferred_score_list_area(area: Rect, offset: u16, rows: &[RankedRow], user_id: Uuid) -> Rect {
    let preferred_height = preferred_score_list_height(rows, user_id);
    Rect::new(
        area.x,
        area.y + offset,
        area.width,
        area.height.saturating_sub(offset).min(preferred_height),
    )
}

fn preferred_score_list_height(rows: &[RankedRow], user_id: Uuid) -> u16 {
    let visible_leaders = rows.len().min(TOP_LIMIT_SCORE) as u16;
    let user_tail = rows
        .iter()
        .position(|row| row.user_id == user_id)
        .is_some_and(|index| index >= TOP_LIMIT_SCORE);
    let body_height = if rows.is_empty() {
        1
    } else {
        visible_leaders + u16::from(user_tail) * SCORE_USER_TAIL_HEIGHT
    };
    1 + body_height
}

fn win_streak_area(area: Rect, all_time_bottom: u16, row_count: usize) -> Option<Rect> {
    if row_count == 0 {
        return None;
    }

    let available_top = all_time_bottom.saturating_add(WIN_STREAK_GAP);
    let available_height = area.bottom().saturating_sub(available_top);
    if available_height < 2 {
        return None;
    }

    let row_height = (row_count.min(TOP_LIMIT_SCORE) as u16).min(available_height - 1);
    let section_height = 1 + row_height;
    Some(Rect::new(
        area.x,
        area.bottom() - section_height,
        area.width,
        section_height,
    ))
}

fn draw_win_streak_list(
    frame: &mut Frame,
    area: Rect,
    rows: &[RankedRow],
    user_id: Uuid,
    content_width: usize,
) {
    render_line(frame, area, 0, subsection_heading("win streak"));
    for (index, row) in rows
        .iter()
        .take(usize::from(area.height.saturating_sub(1)))
        .enumerate()
    {
        render_line(
            frame,
            area,
            index as u16 + 1,
            ranked_line(
                row,
                "",
                row.user_id == user_id,
                content_width,
                RowDensity::Score,
            ),
        );
    }
}

fn draw_score_list(
    frame: &mut Frame,
    area: Rect,
    sub_title: &str,
    rows: &[RankedRow],
    user_id: Uuid,
    content_width: usize,
) {
    if area.is_empty() {
        return;
    }
    let mut lines = vec![subsection_heading(sub_title)];
    let body_room = usize::from(area.height).saturating_sub(1);
    if body_room > 0 {
        if rows.is_empty() {
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    "no scores yet".to_string(),
                    Style::default().fg(theme::TEXT_FAINT()),
                ),
            ]));
        } else {
            let user_rank = rows.iter().position(|row| row.user_id == user_id);
            lines.extend(ranked_lines_from_rows(
                rows,
                "",
                user_id,
                user_rank,
                body_room,
                content_width,
                TOP_LIMIT_SCORE,
                true,
                RowDensity::Score,
            ));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn score_card_content_width(
    available_width: usize,
    boards: &[&[RankedRow]],
    user_id: Uuid,
) -> usize {
    let natural_width = boards
        .iter()
        .flat_map(|rows| score_card_rows(rows, user_id))
        .map(score_row_natural_width)
        .max()
        .unwrap_or(available_width);
    available_width.min(natural_width)
}

fn score_card_rows(rows: &[RankedRow], user_id: Uuid) -> impl Iterator<Item = &RankedRow> {
    let user_tail = rows
        .iter()
        .position(|row| row.user_id == user_id)
        .filter(|index| *index >= TOP_LIMIT_SCORE)
        .map(|index| &rows[index]);
    rows.iter().take(TOP_LIMIT_SCORE).chain(user_tail)
}

fn score_row_natural_width(row: &RankedRow) -> usize {
    let lead_width = 1;
    let rank_width = format!("#{} ", row.rank).chars().count();
    let username_width = row.username.chars().count();
    let name_score_gap = 2;
    let value_width = format_number(row.value).chars().count();
    let right_pad = 1;

    lead_width + rank_width + username_width + name_score_gap + value_width + right_pad
}

fn render_line(frame: &mut Frame, area: Rect, offset: u16, line: Line<'static>) {
    if offset < area.height {
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(area.x, area.y + offset, area.width, 1),
        );
    }
}

#[derive(Clone)]
struct RankedRow {
    rank: i64,
    username: String,
    value: i64,
    user_id: Uuid,
}

impl From<&RankedEntry> for RankedRow {
    fn from(entry: &RankedEntry) -> Self {
        Self {
            rank: entry.rank,
            username: entry.username.clone(),
            value: entry.value,
            user_id: entry.user_id,
        }
    }
}

impl From<&HighScoreEntry> for RankedRow {
    fn from(entry: &HighScoreEntry) -> Self {
        Self {
            rank: entry.rank,
            username: entry.username.clone(),
            value: i64::from(entry.score),
            user_id: entry.user_id,
        }
    }
}

#[derive(Clone, Copy)]
enum RowDensity {
    Top,
    Score,
}

/// Shows the first `top_limit` rows. If the current user is ranked below that,
/// append an ellipsis plus their row, reducing the visible top rows as needed.
#[allow(clippy::too_many_arguments)]
fn ranked_lines_from_rows(
    rows: &[RankedRow],
    unit: &str,
    user_id: Uuid,
    user_index: Option<usize>,
    height: usize,
    width: usize,
    top_limit: usize,
    show_current_marker: bool,
    density: RowDensity,
) -> Vec<Line<'static>> {
    if rows.is_empty() || height == 0 {
        return Vec::new();
    }

    let needs_user_tail = user_index.is_some_and(|index| index >= top_limit);
    let tail_cost = if needs_user_tail { 2 } else { 0 };
    let top_count = top_limit
        .min(rows.len())
        .min(height.saturating_sub(tail_cost));

    let mut lines = Vec::with_capacity(height);
    for row in rows.iter().take(top_count) {
        lines.push(ranked_line(
            row,
            unit,
            show_current_marker && row.user_id == user_id,
            width,
            density,
        ));
    }

    if needs_user_tail && lines.len() + 2 <= height {
        lines.push(divider_line(width));
        if let Some(index) = user_index {
            let row = &rows[index];
            lines.push(ranked_line_with_marker_space(
                row,
                unit,
                show_current_marker,
                width,
                density,
                !show_current_marker,
            ));
        }
    }
    lines
}

fn divider_line(width: usize) -> Line<'static> {
    let dots = " …";
    Line::from(vec![
        Span::styled(dots.to_string(), Style::default().fg(theme::TEXT_FAINT())),
        Span::raw(" ".repeat(width.saturating_sub(dots.chars().count()))),
    ])
}

fn ranked_line(
    row: &RankedRow,
    unit: &str,
    is_current_user: bool,
    width: usize,
    density: RowDensity,
) -> Line<'static> {
    ranked_line_with_marker_space(row, unit, is_current_user, width, density, false)
}

fn ranked_line_with_marker_space(
    row: &RankedRow,
    unit: &str,
    is_current_user: bool,
    width: usize,
    density: RowDensity,
    reserve_hidden_marker: bool,
) -> Line<'static> {
    let (lead, rank_text) = match (density, is_current_user, reserve_hidden_marker) {
        (RowDensity::Top, false, true) => (" ".to_string(), format!("#{:<4}", row.rank)),
        (RowDensity::Top, true, _) => ("› ".to_string(), format!("#{} ", row.rank)),
        (RowDensity::Top, false, false) => (" ".to_string(), format!("#{:<3}", row.rank)),
        (RowDensity::Score, true, _) => ("›".to_string(), format!("#{} ", row.rank)),
        (RowDensity::Score, false, _) => (" ".to_string(), format!("#{} ", row.rank)),
    };
    let value_text = if unit.is_empty() {
        format_number(row.value)
    } else {
        format!("{} {unit}", format_number(row.value))
    };

    let selected = Style::default()
        .bg(theme::BG_SELECTION())
        .add_modifier(Modifier::BOLD);
    let lead_style = if is_current_user {
        selected.fg(theme::AMBER_GLOW())
    } else if row.rank == 1 {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let rank_style = if is_current_user {
        selected.fg(theme::AMBER_GLOW())
    } else if row.rank == 1 {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let name_style = if is_current_user {
        selected.fg(theme::TEXT_BRIGHT())
    } else if row.rank == 1 {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let value_style = if is_current_user {
        selected.fg(theme::SUCCESS())
    } else {
        Style::default().fg(theme::SUCCESS())
    };
    let fill_style = if is_current_user {
        Style::default().bg(theme::BG_SELECTION())
    } else {
        Style::default()
    };

    let gutter = 1;
    let right_pad = 1;
    let fixed = lead.chars().count()
        + rank_text.chars().count()
        + gutter
        + value_text.chars().count()
        + right_pad;
    let name_room = width.saturating_sub(fixed);
    let username = truncate(&row.username, name_room);
    let name_pad = name_room.saturating_sub(username.chars().count());

    Line::from(vec![
        Span::styled(lead, lead_style),
        Span::styled(rank_text, rank_style),
        Span::styled(username, name_style),
        Span::styled(" ".repeat(name_pad + gutter), fill_style),
        Span::styled(value_text, value_style),
        Span::styled(" ".repeat(right_pad), fill_style),
    ])
}

fn section_heading(title: &str) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled(" ── ", dim),
        Span::styled(title.to_string(), accent),
        Span::styled(" ──", dim),
    ])
}

fn subsection_heading(title: &str) -> Line<'static> {
    let style = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled(" ─ ", style),
        Span::styled(title.to_string(), style),
        Span::styled(" ─", style),
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
    let digits = value.unsigned_abs().to_string();
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 && (bytes.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*byte as char);
    }
    format!("{sign}{out}")
}

fn draw_grid(frame: &mut Frame, grid: &DenseGrid) {
    let dim = Style::default().fg(theme::BORDER_DIM());
    let active = Style::default().fg(theme::BORDER_ACTIVE());

    for x in &grid.top_dividers {
        draw_vertical(frame, *x, grid.content_top, grid.top_divider_y, dim);
    }
    for (index, x) in grid.score_dividers.iter().enumerate() {
        let end = if index < 2 {
            grid.lower_divider_y
        } else {
            grid.bottom_y
        };
        draw_vertical(frame, *x, grid.score_top, end, dim);
    }

    let top_rule = junction_rule(
        grid.popup.width,
        &grid
            .top_dividers
            .iter()
            .map(|x| x - grid.popup.x)
            .collect::<Vec<_>>(),
        &grid
            .score_dividers
            .iter()
            .map(|x| x - grid.popup.x)
            .collect::<Vec<_>>(),
    );
    frame.render_widget(
        Paragraph::new(Span::styled(top_rule, dim)),
        Rect::new(grid.popup.x, grid.top_divider_y, grid.popup.width, 1),
    );
    draw_cell(frame, grid.popup.x, grid.top_divider_y, "├", active);
    draw_cell(
        frame,
        grid.popup.right() - 1,
        grid.top_divider_y,
        "┤",
        active,
    );

    let lower_end = grid.score_dividers[2];
    let lower_rule = lower_junction_rule(
        lower_end - grid.popup.x + 1,
        &[
            grid.score_dividers[0] - grid.popup.x,
            grid.score_dividers[1] - grid.popup.x,
        ],
    );
    frame.render_widget(
        Paragraph::new(Span::styled(lower_rule, dim)),
        Rect::new(
            grid.popup.x,
            grid.lower_divider_y,
            lower_end - grid.popup.x + 1,
            1,
        ),
    );
    draw_cell(frame, grid.popup.x, grid.lower_divider_y, "├", active);

    draw_cell(frame, grid.score_dividers[2], grid.bottom_y, "┴", active);
    draw_cell(frame, grid.score_dividers[3], grid.bottom_y, "┴", active);
}

fn draw_vertical(frame: &mut Frame, x: u16, start_y: u16, end_y: u16, style: Style) {
    if end_y <= start_y {
        return;
    }
    let lines: Vec<Line<'static>> = (start_y..end_y)
        .map(|_| Line::from(Span::styled("│", style)))
        .collect();
    frame.render_widget(
        Paragraph::new(lines),
        Rect::new(x, start_y, 1, end_y - start_y),
    );
}

fn draw_cell(frame: &mut Frame, x: u16, y: u16, symbol: &'static str, style: Style) {
    frame.render_widget(
        Paragraph::new(Span::styled(symbol, style)),
        Rect::new(x, y, 1, 1),
    );
}

fn junction_rule(width: u16, up: &[u16], down: &[u16]) -> String {
    let mut rule = String::with_capacity(width as usize);
    for x in 0..width {
        let symbol = if x == 0 {
            '├'
        } else if x + 1 == width {
            '┤'
        } else if up.contains(&x) && down.contains(&x) {
            '┼'
        } else if up.contains(&x) {
            '┴'
        } else if down.contains(&x) {
            '┬'
        } else {
            '─'
        };
        rule.push(symbol);
    }
    rule
}

fn lower_junction_rule(width: u16, up: &[u16]) -> String {
    let mut rule = String::with_capacity(width as usize);
    for x in 0..width {
        let symbol = if x == 0 {
            '├'
        } else if x + 1 == width {
            '┤'
        } else if up.contains(&x) {
            '┴'
        } else {
            '─'
        };
        rule.push(symbol);
    }
    rule
}

#[cfg(test)]
#[path = "leaderboard_test.rs"]
mod leaderboard_test;
