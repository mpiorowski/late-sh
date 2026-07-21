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
    let mut rows: Vec<RankedRow> = (1..=50).map(|n| row(n, &format!("u{n}"), 100)).collect();
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
