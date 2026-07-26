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

const TOP_LIMIT_RANKED: usize = 10;
const TOP_LIMIT_SCORE: usize = 5;
const SCORE_MONTHLY_OFFSET: u16 = 2;
const SCORE_USER_TAIL_HEIGHT: u16 = 2;
const SCORE_MONTHLY_HEIGHT: u16 = 1 + TOP_LIMIT_SCORE as u16 + SCORE_USER_TAIL_HEIGHT;
const SCORE_ALL_TIME_OFFSET: u16 = SCORE_MONTHLY_OFFSET + SCORE_MONTHLY_HEIGHT;
const SCORE_ALL_TIME_GAP: u16 = 1;
const WIN_STREAK_GAP: u16 = 1;
const MORE_LEADERBOARDS_TITLE: &str = "More Leaderboards coming!";
const TOP_BOARD_BASE_SPANS: [u16; 2] = [31, 26];
// The span includes the 32-cell heading, its required trailing space, and the
// right-hand border. The heading itself supplies the required leading space.
const MORE_LEADERBOARDS_PANEL_SPAN: u16 = 34;
const REFERENCE_SCORE_SPAN: u16 = 110;
const REFERENCE_SCORE_PANEL_SPANS: [u16; 5] = [23, 22, 22, 22, 21];

pub(crate) fn draw(
    frame: &mut Frame,
    popup: Rect,
    hub_footer: Rect,
    data: &LeaderboardData,
    user_id: Uuid,
    is_admin: bool,
) {
    // The dense grid carries its own footer, cut short so a column divider can
    // run past the right of it. When the popup is too small to build a grid,
    // the hub's plain full-width footer stands in: without it the tab would
    // render as an empty modal with no key hints at all.
    let Some(mut grid) = DenseGrid::new(popup) else {
        super::ui::draw_footer(frame, hub_footer, is_admin);
        return;
    };
    fit_top_row_to_content(&mut grid, data, user_id);

    draw_top_row(frame, &grid, data, user_id);
    draw_score_row(frame, &grid, data, user_id);
    super::ui::draw_footer(frame, grid.footer_area(), is_admin);
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
        let top_offsets = top_divider_offsets(
            span,
            TOP_BOARD_BASE_SPANS.map(|panel_span| panel_span.saturating_sub(1)),
        );
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

    fn set_top_panel_content_widths(&mut self, content_widths: [usize; 2]) {
        let span = self.popup.width - 1;
        let content_widths = content_widths.map(|width| u16::try_from(width).unwrap_or(u16::MAX));
        self.top_dividers = top_divider_offsets(span, content_widths)
            .into_iter()
            .map(|offset| self.popup.x + offset)
            .collect();
    }
}

fn top_divider_offsets(span: u16, content_widths: [u16; 2]) -> Vec<u16> {
    let base_board_span = TOP_BOARD_BASE_SPANS.iter().sum::<u16>();
    let base_span = base_board_span + MORE_LEADERBOARDS_PANEL_SPAN;
    if span < base_span {
        return vec![span / 2];
    }

    let board_budget = span - MORE_LEADERBOARDS_PANEL_SPAN;
    let desired_spans: [u16; 2] = std::array::from_fn(|index| {
        content_widths[index]
            .saturating_add(1)
            .max(TOP_BOARD_BASE_SPANS[index])
    });
    let mut panel_spans = TOP_BOARD_BASE_SPANS;
    let mut remaining = board_budget.saturating_sub(base_board_span);
    while remaining > 0 {
        let mut grew = false;
        for (panel_span, desired_span) in panel_spans.iter_mut().zip(desired_spans) {
            if *panel_span < desired_span {
                *panel_span += 1;
                remaining -= 1;
                grew = true;
                if remaining == 0 {
                    break;
                }
            }
        }
        if !grew {
            break;
        }
    }

    // Space the boards could not use is shared across all three panels rather
    // than pooling in the placeholder, the way the score row shares its own
    // surplus. Two roomy boards beside a right-sized "coming soon" beats two
    // cramped ones beside a mostly empty half-screen.
    if remaining > 0 {
        let mut all_spans = [panel_spans[0], panel_spans[1], MORE_LEADERBOARDS_PANEL_SPAN];
        distribute_surplus(&mut all_spans, remaining);
        panel_spans = [all_spans[0], all_spans[1]];
    }

    vec![panel_spans[0], panel_spans.iter().sum()]
}

fn score_divider_offsets(span: u16) -> [u16; 4] {
    if span < REFERENCE_SCORE_SPAN {
        return [span / 5, span * 2 / 5, span * 3 / 5, span * 4 / 5];
    }

    // Each span includes its right-hand divider. Le Word keeps a 20-cell
    // interior at the reference width, leaving exactly the row formatter's
    // one trailing pad before the outer border. The five cells it formerly
    // left unused are shared across the first four games.
    let mut panel_spans = REFERENCE_SCORE_PANEL_SPANS;
    distribute_surplus(&mut panel_spans, span - REFERENCE_SCORE_SPAN);

    let mut offset = 0;
    std::array::from_fn(|index| {
        offset += panel_spans[index];
        offset
    })
}

fn distribute_surplus<const N: usize>(panel_spans: &mut [u16; N], extra: u16) {
    debug_assert!(N > 0);
    let panel_count = N as u16;
    let shared = extra / panel_count;
    let remainder = extra % panel_count;
    for (index, panel_span) in panel_spans.iter_mut().enumerate() {
        *panel_span += shared + u16::from(index < usize::from(remainder));
    }
}

fn panel_between(left: u16, right: u16, y: u16, height: u16) -> Rect {
    Rect::new(
        left.saturating_add(1),
        y,
        right.saturating_sub(left).saturating_sub(1),
        height,
    )
}

/// One definition per top board, shared by the width pass and the draw pass.
/// Measuring has to see the same headings, hints and empty-state text that get
/// printed, or a board sizes itself to rows it may not even have.
fn top_board_views(data: &LeaderboardData) -> [RankedBoardView<'_>; 2] {
    [
        RankedBoardView {
            title: "Top Chips",
            unit: "chips",
            entries: &data.monthly_chip_earners,
            empty: "no chip earnings yet this month",
            hints: &["monthly net chip delta", "shop spend ignored"],
            show_current_marker: true,
        },
        RankedBoardView {
            title: "Arcade Wins",
            unit: "pts",
            entries: &data.arcade_champions,
            empty: "no daily puzzle wins yet this month",
            hints: &["daily puzzle value:", "easy 1·medium 3·hard 5"],
            show_current_marker: false,
        },
    ]
}

fn fit_top_row_to_content(grid: &mut DenseGrid, data: &LeaderboardData, user_id: Uuid) {
    let panel_height = grid.top_divider_y.saturating_sub(grid.content_top);
    let body_height = usize::from(panel_height.saturating_sub(ranked_panel_body_offset(2)));
    let views = top_board_views(data);
    let content_widths =
        std::array::from_fn(|index| top_board_natural_width(&views[index], user_id, body_height));
    grid.set_top_panel_content_widths(content_widths);
}

fn top_board_natural_width(view: &RankedBoardView<'_>, user_id: Uuid, height: usize) -> usize {
    let rows: Vec<RankedRow> = view.entries.iter().map(RankedRow::from).collect();
    let plan = top_row_plan(&rows, view.unit, user_id, height, view.show_current_marker);
    let widest_row = card_natural_width(&rows, plan).unwrap_or(0);

    // The heading, the hints and the empty-state line are printed too, each
    // with a leading space already in the text and one trailing cell of pad.
    let heading = section_heading_width(view.title) + 1;
    let widest_note = view
        .hints
        .iter()
        .copied()
        .chain(rows.is_empty().then_some(view.empty))
        .map(|note| note.chars().count() + 2)
        .max()
        .unwrap_or(0);

    widest_row.max(heading).max(widest_note)
}

fn draw_top_row(frame: &mut Frame, grid: &DenseGrid, data: &LeaderboardData, user_id: Uuid) {
    let panels = grid.top_panels();
    for (panel, view) in panels.iter().zip(top_board_views(data)) {
        draw_ranked_panel(frame, *panel, user_id, view);
    }
    if let Some(panel) = panels.get(2) {
        render_line(frame, *panel, 0, section_heading(MORE_LEADERBOARDS_TITLE));
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

fn ranked_panel_body_offset(hint_count: usize) -> u16 {
    1_u16.saturating_add(hint_count as u16).saturating_add(1)
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

    let body_offset = ranked_panel_body_offset(view.hints.len());
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
    let plan = top_row_plan(
        &rows,
        view.unit,
        user_id,
        usize::from(body.height),
        view.show_current_marker,
    );
    let content_width = card_content_width(usize::from(body.width), &rows, plan);
    let lines = ranked_lines_from_rows(&rows, plan, content_width);
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
        score_card_rows(&monthly_rows, user_id).chain(score_card_rows(&all_time_rows, user_id)),
    );
    let all_time_offset = score_all_time_offset(area, &all_time_rows, user_id, 0);

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
    if area.height > all_time_offset {
        let all_time_area = Rect::new(
            area.x,
            area.y + all_time_offset,
            area.width,
            area.height - all_time_offset,
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
    let all_time_offset = score_all_time_offset(
        area,
        &all_time_rows,
        user_id,
        preferred_win_streak_tail_height(win_streak_rows.len()),
    );
    let all_time_area = preferred_score_list_area(area, all_time_offset, &all_time_rows, user_id);
    let streak_area = win_streak_area(area, all_time_area.bottom(), win_streak_rows.len());
    // The streak section lives on leftover rows, so unlike the two boards above
    // it there is no reserved tail: whatever height it got decides how many
    // leaders it trades away to show your own streak.
    let streak_plan = streak_area.map(|section| {
        score_row_plan(
            &win_streak_rows,
            user_id,
            usize::from(section.height.saturating_sub(1)),
        )
    });
    let content_width = score_card_content_width(
        usize::from(area.width),
        score_card_rows(&monthly_rows, user_id)
            .chain(score_card_rows(&all_time_rows, user_id))
            .chain(
                streak_plan
                    .into_iter()
                    .flat_map(|plan| planned_rows(&win_streak_rows, plan)),
            ),
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
    if area.height > all_time_offset {
        draw_score_list(
            frame,
            all_time_area,
            "all-time",
            &all_time_rows,
            user_id,
            content_width,
        );
    }
    if let (Some(section), Some(plan)) = (streak_area, streak_plan) {
        draw_win_streak_list(frame, section, &win_streak_rows, plan, content_width);
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

fn score_all_time_offset(
    area: Rect,
    rows: &[RankedRow],
    user_id: Uuid,
    trailing_height: u16,
) -> u16 {
    let padded_offset = SCORE_ALL_TIME_OFFSET + SCORE_ALL_TIME_GAP;
    let padded_height = padded_offset
        .saturating_add(preferred_score_list_height(rows, user_id))
        .saturating_add(trailing_height);
    if padded_height <= area.height {
        padded_offset
    } else {
        SCORE_ALL_TIME_OFFSET
    }
}

fn preferred_win_streak_tail_height(row_count: usize) -> u16 {
    if row_count == 0 {
        0
    } else {
        WIN_STREAK_GAP + 1 + row_count.min(TOP_LIMIT_SCORE) as u16
    }
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
    Some(Rect::new(area.x, available_top, area.width, section_height))
}

fn draw_win_streak_list(
    frame: &mut Frame,
    area: Rect,
    rows: &[RankedRow],
    plan: RowPlan<'_>,
    content_width: usize,
) {
    let mut lines = vec![subsection_heading("win streak")];
    lines.extend(ranked_lines_from_rows(rows, plan, content_width));
    frame.render_widget(Paragraph::new(lines), area);
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
            let plan = score_row_plan(rows, user_id, body_room);
            lines.extend(ranked_lines_from_rows(rows, plan, content_width));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Right-align the values at the widest line the card will actually print, so
/// a rank nobody can see never pushes the score column out. Callers pass the
/// rows they are about to draw.
fn score_card_content_width<'a>(
    available_width: usize,
    rows: impl Iterator<Item = &'a RankedRow>,
) -> usize {
    let natural_width = rows
        .map(score_row_natural_width)
        .max()
        .unwrap_or(available_width);
    available_width.min(natural_width)
}

/// The rows a full-height score subsection shows: the top five, plus the
/// viewer's own row when they rank below it.
fn score_card_rows(rows: &[RankedRow], user_id: Uuid) -> impl Iterator<Item = &RankedRow> {
    let user_tail = rows
        .iter()
        .position(|row| row.user_id == user_id)
        .filter(|index| *index >= TOP_LIMIT_SCORE)
        .map(|index| &rows[index]);
    rows.iter().take(TOP_LIMIT_SCORE).chain(user_tail)
}

/// The rows a height-constrained plan will fit, tail included when it survives.
fn planned_rows<'a>(
    rows: &'a [RankedRow],
    plan: RowPlan<'_>,
) -> impl Iterator<Item = &'a RankedRow> {
    let (top_count, include_user_tail) = ranked_display_plan(rows.len(), plan);
    let user_tail = include_user_tail
        .then(|| plan.user_index.map(|index| &rows[index]))
        .flatten();
    rows.iter().take(top_count).chain(user_tail)
}

fn score_row_natural_width(row: &RankedRow) -> usize {
    ranked_row_natural_width(row, "", false, RowDensity::Score, false)
}

fn card_content_width(available_width: usize, rows: &[RankedRow], plan: RowPlan<'_>) -> usize {
    available_width.min(card_natural_width(rows, plan).unwrap_or(available_width))
}

fn card_natural_width(rows: &[RankedRow], plan: RowPlan<'_>) -> Option<usize> {
    let (top_count, include_user_tail) = ranked_display_plan(rows.len(), plan);
    let top_widths = rows.iter().take(top_count).map(|row| {
        ranked_row_natural_width(
            row,
            plan.unit,
            plan.show_current_marker && row.user_id == plan.user_id,
            plan.density,
            false,
        )
    });
    let tail_width = include_user_tail
        .then(|| {
            plan.user_index.map(|index| {
                ranked_row_natural_width(
                    &rows[index],
                    plan.unit,
                    plan.show_current_marker,
                    plan.density,
                    !plan.show_current_marker,
                )
            })
        })
        .flatten();

    top_widths.chain(tail_width).max()
}

fn ranked_row_natural_width(
    row: &RankedRow,
    unit: &str,
    is_current_user: bool,
    density: RowDensity,
    reserve_hidden_marker: bool,
) -> usize {
    let (lead, rank_text) =
        ranked_prefix(row.rank, density, is_current_user, reserve_hidden_marker);
    let value_text = ranked_value_text(row.value, unit);
    let name_score_gap = 2;
    let right_pad = 1;

    lead.chars().count()
        + rank_text.chars().count()
        + row.username.chars().count()
        + name_score_gap
        + value_text.chars().count()
        + right_pad
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

impl RowDensity {
    /// How deep the leader slice goes before the current-user tail takes over.
    /// A property of the density, not of the caller: the roomy top boards show
    /// ten, the stacked score subsections five.
    fn top_limit(self) -> usize {
        match self {
            RowDensity::Top => TOP_LIMIT_RANKED,
            RowDensity::Score => TOP_LIMIT_SCORE,
        }
    }
}

/// Everything one board needs to place its rows: which entry is the viewer's,
/// how much vertical room the body has, and how a row is spelled at this
/// density. Width stays out of it, because the width is what the natural-width
/// pass is computing.
#[derive(Clone, Copy)]
struct RowPlan<'a> {
    unit: &'a str,
    user_id: Uuid,
    user_index: Option<usize>,
    height: usize,
    show_current_marker: bool,
    density: RowDensity,
}

fn top_row_plan<'a>(
    rows: &[RankedRow],
    unit: &'a str,
    user_id: Uuid,
    height: usize,
    show_current_marker: bool,
) -> RowPlan<'a> {
    RowPlan {
        unit,
        user_id,
        user_index: rows.iter().position(|row| row.user_id == user_id),
        height,
        show_current_marker,
        density: RowDensity::Top,
    }
}

fn score_row_plan(rows: &[RankedRow], user_id: Uuid, height: usize) -> RowPlan<'static> {
    RowPlan {
        unit: "",
        user_id,
        user_index: rows.iter().position(|row| row.user_id == user_id),
        height,
        show_current_marker: true,
        density: RowDensity::Score,
    }
}

/// Shows the first `density.top_limit()` rows. If the current user is ranked
/// below that, append an ellipsis plus their row, reducing the visible top rows
/// as needed.
fn ranked_lines_from_rows(
    rows: &[RankedRow],
    plan: RowPlan<'_>,
    width: usize,
) -> Vec<Line<'static>> {
    if rows.is_empty() || plan.height == 0 {
        return Vec::new();
    }

    let (top_count, include_user_tail) = ranked_display_plan(rows.len(), plan);

    let mut lines = Vec::with_capacity(plan.height);
    for row in rows.iter().take(top_count) {
        lines.push(ranked_line(
            row,
            plan.unit,
            plan.show_current_marker && row.user_id == plan.user_id,
            width,
            plan.density,
        ));
    }

    if include_user_tail {
        lines.push(divider_line(width));
        if let Some(index) = plan.user_index {
            let row = &rows[index];
            lines.push(ranked_line_with_marker_space(
                row,
                plan.unit,
                plan.show_current_marker,
                width,
                plan.density,
                !plan.show_current_marker,
            ));
        }
    }
    lines
}

fn ranked_display_plan(row_count: usize, plan: RowPlan<'_>) -> (usize, bool) {
    let top_limit = plan.density.top_limit();
    let needs_user_tail = plan.user_index.is_some_and(|index| index >= top_limit);
    let leaders = |reserved: usize| {
        top_limit
            .min(row_count)
            .min(plan.height.saturating_sub(reserved))
    };

    // The tail costs an ellipsis row plus your own. It only earns them if a
    // leader still fits above it: a divider hanging under the heading with
    // nothing before it reads as a broken list, not as "you are further down".
    let with_tail = leaders(2);
    if needs_user_tail && with_tail >= 1 && with_tail + 2 <= plan.height {
        (with_tail, true)
    } else {
        (leaders(0), false)
    }
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
    let (lead, rank_text) =
        ranked_prefix(row.rank, density, is_current_user, reserve_hidden_marker);
    let value_text = ranked_value_text(row.value, unit);

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

fn ranked_prefix(
    rank: i64,
    density: RowDensity,
    is_current_user: bool,
    reserve_hidden_marker: bool,
) -> (String, String) {
    match (density, is_current_user, reserve_hidden_marker) {
        (RowDensity::Top, false, true) => (" ".to_string(), format!("#{rank:<4}")),
        (RowDensity::Top, true, _) => ("› ".to_string(), format!("#{rank} ")),
        (RowDensity::Top, false, false) => (" ".to_string(), format!("#{rank:<3}")),
        (RowDensity::Score, true, _) => ("›".to_string(), format!("#{rank} ")),
        (RowDensity::Score, false, _) => (" ".to_string(), format!("#{rank} ")),
    }
}

fn ranked_value_text(value: i64, unit: &str) -> String {
    if unit.is_empty() {
        format_number(value)
    } else {
        format!("{} {unit}", format_number(value))
    }
}

/// Rendered width of [`section_heading`], leading space included.
fn section_heading_width(title: &str) -> usize {
    " ── ".chars().count() + title.chars().count() + " ──".chars().count()
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
