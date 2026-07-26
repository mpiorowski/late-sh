use super::*;
use ratatui::{Terminal, backend::TestBackend, layout::Size};

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

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn top_plan(
    unit: &'static str,
    me: Uuid,
    user_index: Option<usize>,
    height: usize,
    show_current_marker: bool,
) -> RowPlan<'static> {
    RowPlan {
        unit,
        user_id: me,
        user_index,
        height,
        show_current_marker,
        density: RowDensity::Top,
    }
}

fn score_plan(me: Uuid, user_index: Option<usize>, height: usize) -> RowPlan<'static> {
    RowPlan {
        unit: "",
        user_id: me,
        user_index,
        height,
        show_current_marker: true,
        density: RowDensity::Score,
    }
}

/// Draw the leaderboard over a whole `width x height` screen, taking the hub's
/// footer row where the shell would put it, and return what landed.
fn drawn_leaderboard(width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    let data = LeaderboardData::default();
    terminal
        .draw(|frame| {
            let popup = frame.area();
            let hub_footer = Rect::new(popup.x + 1, popup.bottom() - 2, popup.width - 2, 1);
            draw(frame, popup, hub_footer, &data, Uuid::nil(), false);
        })
        .expect("draw");

    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

/// The dense grid carries its own footer, so a popup too small to build one
/// used to render the tab with no key hints at all.
#[test]
fn key_hints_survive_a_popup_too_small_for_the_grid() {
    let reference = drawn_leaderboard(111, 41);
    assert!(
        reference.contains("Esc/q close"),
        "reference popup lost its footer:\n{reference}"
    );

    assert!(DenseGrid::new(Rect::new(0, 0, 80, 13)).is_none());
    let short = drawn_leaderboard(80, 13);
    assert!(
        short.contains("Esc/q close"),
        "gridless popup lost its footer:\n{short}"
    );
}

/// The width pass used to measure ranked rows only, so a board with no rows
/// fell back to its base span and clipped its own empty-state line ("no chip
/// earnings yet this mon") while the placeholder beside it sat half empty.
#[test]
fn empty_top_boards_are_wide_enough_for_their_own_copy() {
    let data = LeaderboardData::default();
    for view in &top_board_views(&data) {
        let natural = top_board_natural_width(view, Uuid::nil(), 12);
        let title = view.title;

        let heading = section_heading_width(title) + 1;
        assert!(
            natural >= heading,
            "{title} clips its heading: {natural} < {heading}"
        );

        let empty = view.empty.chars().count() + 2;
        assert!(
            natural >= empty,
            "{title} clips its empty state: {natural} < {empty}"
        );

        for hint in view.hints {
            let hint_width = hint.chars().count() + 2;
            assert!(
                natural >= hint_width,
                "{title} clips hint {hint:?}: {natural} < {hint_width}"
            );
        }
    }

    // And end to end, the panel really does print the whole line.
    let wide = drawn_leaderboard(160, 44);
    assert!(
        wide.contains("no chip earnings yet this month"),
        "Top Chips clipped its empty state:\n{wide}"
    );
    assert!(
        wide.contains("no daily puzzle wins yet this month"),
        "Arcade Wins clipped its empty state:\n{wide}"
    );
}

#[test]
fn reference_grid_matches_target_cells() {
    let grid = DenseGrid::new(Rect::new(0, 0, 111, 41)).unwrap();
    assert_eq!(grid.content_top, 4);
    assert_eq!(grid.top_divider_y, 20);
    assert_eq!(grid.score_top, 21);
    assert_eq!(grid.lower_divider_y, 38);
    assert_eq!(grid.footer_y, 39);
    assert_eq!(grid.bottom_y, 40);
    assert_eq!(grid.top_dividers, vec![38, 70]);
    assert_eq!(grid.score_dividers, [23, 45, 67, 89]);
    assert_eq!(
        grid.top_panels()
            .iter()
            .map(|panel| Size::new(panel.width, panel.height))
            .collect::<Vec<Size>>(),
        vec![Size::new(37, 16), Size::new(31, 16), Size::new(39, 16)]
    );
    assert_eq!(
        grid.score_panels()
            .map(|panel| Size::new(panel.width, panel.height)),
        [
            Size::new(22, 17),
            Size::new(21, 17),
            Size::new(21, 17),
            Size::new(21, 19),
            Size::new(20, 19),
        ]
    );
    assert_eq!(grid.footer_area(), Rect::new(1, 39, 66, 1));
}

#[test]
fn monthly_score_area_reserves_top_five_and_current_user_tail() {
    let area = Rect::new(1, 21, 22, 17);

    assert_eq!(monthly_score_area(area), Rect::new(1, 23, 22, 8));
    assert_eq!(area.y + SCORE_ALL_TIME_OFFSET, 31);

    let me = Uuid::now_v7();
    let mut rows: Vec<RankedRow> = (1..=5)
        .map(|rank| row(rank, &format!("u{rank}"), 100 - rank))
        .collect();
    rows.push(user_row(169, "me", 10, me));
    let lines = ranked_lines_from_rows(
        &rows,
        score_plan(me, Some(5), usize::from(SCORE_MONTHLY_HEIGHT - 1)),
        22,
    );

    assert_eq!(lines.len(), 7);
    assert!(line_text(&lines[4]).contains("#5 "));
    assert!(line_text(&lines[5]).contains('…'));
    assert!(line_text(&lines[6]).starts_with("›#169 me"));
}

#[test]
fn all_time_gap_uses_only_height_left_after_preferred_content() {
    let me = Uuid::now_v7();
    let mut rows: Vec<RankedRow> = (1..=5)
        .map(|rank| row(rank, &format!("u{rank}"), 100 - rank))
        .collect();

    assert_eq!(
        score_all_time_offset(Rect::new(0, 0, 20, 16), &rows, me, 0),
        SCORE_ALL_TIME_OFFSET
    );
    assert_eq!(
        score_all_time_offset(Rect::new(0, 0, 20, 17), &rows, me, 0),
        SCORE_ALL_TIME_OFFSET + SCORE_ALL_TIME_GAP
    );

    rows.push(user_row(99, "me", 1, me));
    assert_eq!(
        score_all_time_offset(Rect::new(0, 0, 20, 18), &rows, me, 0),
        SCORE_ALL_TIME_OFFSET
    );
    assert_eq!(
        score_all_time_offset(Rect::new(0, 0, 20, 19), &rows, me, 0),
        SCORE_ALL_TIME_OFFSET + SCORE_ALL_TIME_GAP
    );
}

#[test]
fn win_streak_follows_all_time_and_uses_only_remaining_space() {
    let me = Uuid::now_v7();
    let all_time_rows: Vec<RankedRow> = (1..=5)
        .map(|rank| row(rank, &format!("u{rank}"), 100 - rank))
        .collect();
    let reference = Rect::new(90, 21, 20, 19);
    let streak_tail_height = preferred_win_streak_tail_height(5);
    let all_time_offset = score_all_time_offset(reference, &all_time_rows, me, streak_tail_height);
    let all_time = preferred_score_list_area(reference, all_time_offset, &all_time_rows, me);

    assert_eq!(all_time_offset, SCORE_ALL_TIME_OFFSET);
    assert_eq!(all_time, Rect::new(90, 31, 20, 6));
    assert_eq!(
        win_streak_area(reference, all_time.bottom(), 5),
        Some(Rect::new(90, 38, 20, 2))
    );

    let pressured = Rect::new(90, 21, 20, 18);
    assert_eq!(win_streak_area(pressured, all_time.bottom(), 5), None);

    let tall = Rect::new(90, 21, 20, 24);
    let tall_all_time_offset = score_all_time_offset(tall, &all_time_rows, me, streak_tail_height);
    let tall_all_time = preferred_score_list_area(tall, tall_all_time_offset, &all_time_rows, me);
    assert_eq!(
        tall_all_time_offset,
        SCORE_ALL_TIME_OFFSET + SCORE_ALL_TIME_GAP
    );
    assert_eq!(
        win_streak_area(tall, tall_all_time.bottom(), 5),
        Some(Rect::new(90, 39, 20, 6))
    );

    let mut all_time_with_user_tail = all_time_rows;
    all_time_with_user_tail.push(user_row(99, "me", 1, me));
    let prioritized_all_time = preferred_score_list_area(
        reference,
        SCORE_ALL_TIME_OFFSET,
        &all_time_with_user_tail,
        me,
    );
    assert_eq!(prioritized_all_time.height, 8);
    assert_eq!(
        win_streak_area(reference, prioritized_all_time.bottom(), 5),
        None
    );
}

#[test]
fn wide_grid_shares_surplus_width_across_all_score_panels() {
    let grid = DenseGrid::new(Rect::new(0, 0, 144, 51)).unwrap();

    assert_eq!(grid.score_dividers, [30, 59, 88, 116]);
    assert_eq!(
        grid.score_panels().map(|panel| panel.width),
        [29, 28, 28, 27, 26]
    );
    assert!(
        grid.score_panels()
            .into_iter()
            .zip([22, 21, 21, 21, 20])
            .all(|(panel, reference_width)| panel.width > reference_width)
    );
}

#[test]
fn top_board_borders_follow_content_and_share_the_surplus() {
    let mut reference = DenseGrid::new(Rect::new(0, 0, 111, 41)).unwrap();
    reference.set_top_panel_content_widths([34, 29]);
    let mut wide = DenseGrid::new(Rect::new(0, 0, 144, 51)).unwrap();
    wide.set_top_panel_content_widths([34, 29]);

    // Both boards clear the 35 and 30 spans their content asked for, then the
    // leftover is split three ways instead of landing in the placeholder.
    assert_eq!(reference.top_dividers, vec![39, 73]);
    assert_eq!(wide.top_dividers, vec![50, 95]);
    assert_eq!(
        reference
            .top_panels()
            .iter()
            .map(|panel| panel.width)
            .collect::<Vec<_>>(),
        vec![38, 33, 36]
    );
    assert_eq!(
        wide.top_panels()
            .iter()
            .map(|panel| panel.width)
            .collect::<Vec<_>>(),
        vec![49, 44, 47]
    );

    let heading = line_text(&section_heading(MORE_LEADERBOARDS_TITLE));
    assert_eq!(heading, " ── More Leaderboards coming! ──");
    assert_eq!(heading.chars().next(), Some(' '));
    assert!(reference.top_panels()[2].width as usize - heading.chars().count() >= 1);

    let mut constrained = DenseGrid::new(Rect::new(0, 0, 92, 41)).unwrap();
    constrained.set_top_panel_content_widths([34, 29]);
    assert_eq!(constrained.top_dividers, vec![31, 57]);
    assert_eq!(
        constrained
            .top_panels()
            .iter()
            .map(|panel| panel.width)
            .collect::<Vec<_>>(),
        vec![30, 25, 33]
    );

    let too_narrow = DenseGrid::new(Rect::new(0, 0, 91, 41)).unwrap();
    assert_eq!(too_narrow.top_panels().len(), 2);
}

#[test]
fn compact_rows_match_reference_spacing() {
    assert_eq!(
        line_text(&ranked_line(
            &row(1, "lemoncmd", 52_575),
            "chips",
            false,
            30,
            RowDensity::Top,
        )),
        " #1  lemoncmd    52,575 chips "
    );
    assert_eq!(
        line_text(&ranked_line(
            &row(1, "beforeuall", 2_205_888),
            "",
            false,
            20,
            RowDensity::Score,
        )),
        " #1 befo… 2,205,888 "
    );
    assert_eq!(
        line_text(&ranked_line(
            &row(1, "beforeuall", 2_205_888),
            "",
            false,
            26,
            RowDensity::Score,
        )),
        " #1 beforeuall  2,205,888 "
    );
    assert_eq!(
        line_text(&ranked_line(
            &row(1, "mevanlc", 123_212),
            "",
            true,
            20,
            RowDensity::Score,
        )),
        "›#1 mevanlc 123,212 "
    );
    assert_eq!(
        line_text(&ranked_line(
            &row(1, "odd", 15),
            "",
            false,
            20,
            RowDensity::Score,
        )),
        " #1 odd          15 "
    );
    assert!(
        !line_text(&ranked_line(
            &row(230, "mevanlc", 940),
            "",
            true,
            27,
            RowDensity::Score,
        ))
        .contains('…')
    );
}

#[test]
fn score_card_width_stops_at_widest_visible_line_item() {
    let me = Uuid::now_v7();
    let mut rows = vec![
        row(1, "beforeuall", 2_205_888),
        row(2, "laksuall", 882_734),
        row(3, "0x53", 336_718),
        row(4, "damax", 256_874),
        row(5, "Inkk_is_here", 202_959),
    ];

    assert_eq!(score_row_natural_width(&rows[0]), 26);
    assert_eq!(score_card_content_width(40, score_card_rows(&rows, me)), 26);
    assert_eq!(score_card_content_width(20, score_card_rows(&rows, me)), 20);

    rows.push(user_row(230, "long_current_name", 940, me));
    assert_eq!(score_card_content_width(40, score_card_rows(&rows, me)), 29);

    // A section with room for two rows prints two leaders and no tail, so it
    // must not widen for the deep row it never shows.
    let cramped = score_plan(me, Some(5), 2);
    assert_eq!(
        score_card_content_width(40, planned_rows(&rows, cramped)),
        26
    );
}

#[test]
fn deep_ranked_streak_replaces_leaders_instead_of_hiding_you() {
    let me = Uuid::now_v7();
    let mut rows: Vec<RankedRow> = (1..=20)
        .map(|rank| row(rank, &format!("u{rank}"), 30 - rank))
        .collect();
    rows.push(user_row(21, "me", 2, me));

    // Five rows of room: three leaders, the ellipsis, then your streak.
    let lines = ranked_lines_from_rows(&rows, score_plan(me, Some(20), 5), 20);
    assert_eq!(lines.len(), 5);
    assert!(line_text(&lines[2]).contains("#3 "));
    assert!(line_text(&lines[3]).contains('…'));
    assert!(line_text(&lines[4]).starts_with("›#21 me"));

    // Two rows leave no leader above the ellipsis, so the tail is dropped and
    // the space goes back to the leaders rather than showing you alone.
    let cramped = ranked_lines_from_rows(&rows, score_plan(me, Some(20), 2), 20);
    assert_eq!(cramped.len(), 2);
    assert!(line_text(&cramped[0]).contains("#1 "));
    assert!(line_text(&cramped[1]).contains("#2 "));
}

#[test]
fn top_card_width_stops_at_two_space_name_value_gap() {
    let nobody = Uuid::now_v7();
    let rows = vec![
        row(1, "lb_lemoncmd", 64_100),
        row(2, "lb_mars", 63_200),
        row(3, "lb_astersystem", 62_300),
    ];

    let plan = top_plan("chips", nobody, None, 12, true);
    let content_width = card_content_width(60, &rows, plan);
    assert_eq!(content_width, 34);
    let lines = ranked_lines_from_rows(&rows, plan, content_width);
    assert_eq!(line_text(&lines[0]), " #1  lb_lemoncmd     64,100 chips ");
    assert_eq!(line_text(&lines[2]), " #3  lb_astersystem  62,300 chips ");

    let compact_width = card_content_width(30, &rows, plan);
    assert_eq!(compact_width, 30);
    assert!(line_text(&ranked_lines_from_rows(&rows, plan, compact_width)[2]).contains('…'));
}

#[test]
fn top_card_natural_width_includes_visible_current_user_tail() {
    let me = Uuid::now_v7();
    let mut rows: Vec<RankedRow> = (1..=10)
        .map(|rank| row(rank, &format!("u{rank}"), 20_000 - rank))
        .collect();
    rows.push(user_row(51, "long_current_name", 12_500, me));

    assert_eq!(
        card_content_width(60, &rows, top_plan("chips", me, Some(10), 12, true)),
        38
    );
    assert_eq!(
        card_content_width(60, &rows, top_plan("chips", me, Some(10), 12, false)),
        38
    );
}

#[test]
fn top_visible_users_render_top_only() {
    let me = Uuid::now_v7();
    let rows = vec![
        user_row(1, "alice", 1000, me),
        row(2, "bob", 800),
        row(3, "carol", 600),
    ];
    let lines = ranked_lines_from_rows(&rows, top_plan("chips", me, Some(0), 12, true), 40);
    assert_eq!(lines.len(), 3);
}

#[test]
fn deep_rank_appends_divider_and_user() {
    let me = Uuid::now_v7();
    let mut rows: Vec<RankedRow> = (1..=50)
        .map(|rank| row(rank, &format!("u{rank}"), 1000 - rank * 10))
        .collect();
    rows.push(user_row(51, "me", 100, me));
    let lines = ranked_lines_from_rows(&rows, top_plan("chips", me, Some(50), 14, true), 40);
    assert_eq!(lines.len(), 12);

    let unmarked = ranked_lines_from_rows(&rows, top_plan("chips", me, Some(50), 14, false), 40);
    assert!(line_text(unmarked.last().unwrap()).starts_with(" #51  me"));
}

#[test]
fn no_user_no_tail() {
    let nobody = Uuid::now_v7();
    let rows: Vec<RankedRow> = (1..=12)
        .map(|rank| row(rank, &format!("u{rank}"), 100))
        .collect();
    let lines = ranked_lines_from_rows(&rows, top_plan("chips", nobody, None, 12, true), 40);
    assert_eq!(lines.len(), 10);
}

#[test]
fn tight_budget_keeps_tail_visible() {
    let me = Uuid::now_v7();
    let mut rows: Vec<RankedRow> = (1..=50)
        .map(|rank| row(rank, &format!("u{rank}"), 100))
        .collect();
    rows.push(user_row(51, "me", 1, me));
    let lines = ranked_lines_from_rows(&rows, top_plan("chips", me, Some(50), 3, true), 40);
    assert_eq!(lines.len(), 3);
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
