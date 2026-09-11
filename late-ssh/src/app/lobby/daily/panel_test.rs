use super::*;

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn props_with(matches: Vec<DailyPanelMatchRow>, open_count: usize) -> DailyPanelProps {
    DailyPanelProps {
        matches,
        open_count,
        lobby_glow: false,
        entry_count: 1,
        entry_cap: 4,
    }
}

#[test]
fn panel_height_is_stable_across_states() {
    let empty = props_with(Vec::new(), 0);
    let busy = props_with(
        (0..6)
            .map(|i| DailyPanelMatchRow {
                opponent: format!("player{i}"),
                status: if i == 0 {
                    DailyPanelRowStatus::YourTurn
                } else {
                    DailyPanelRowStatus::Waiting
                },
            })
            .collect(),
        3,
    );
    for props in [&empty, &busy] {
        let lines = daily_panel_lines(21, props);
        assert_eq!(lines.len(), DAILY_PANEL_HEIGHT as usize);
    }
}

#[test]
fn compact_shape_is_the_live_rows_and_one_footer() {
    let quiet = props_with(Vec::new(), 2);
    let texts: Vec<String> = daily_compact_lines(40, 6, &quiet)
        .iter()
        .map(line_text)
        .collect();
    assert_eq!(texts.len(), 2, "a quiet lobby is one note and the footer");
    assert_eq!(texts[0].trim_end(), "  no games running");
    assert_eq!(texts[1].trim_end(), "  2 open · 1/4");

    let busy = props_with(
        (0..6)
            .map(|i| DailyPanelMatchRow {
                opponent: format!("player{i}"),
                status: DailyPanelRowStatus::Waiting,
            })
            .collect(),
        0,
    );
    let texts: Vec<String> = daily_compact_lines(40, 4, &busy)
        .iter()
        .map(line_text)
        .collect();
    assert_eq!(texts.len(), 4, "rows past the area drop, the footer stays");
    assert!(texts[0].starts_with("  player0"));
    assert!(texts[2].starts_with("  player2"));
    assert!(texts[3].starts_with("  0 open"));

    let one = props_with(
        vec![DailyPanelMatchRow {
            opponent: "mira".to_string(),
            status: DailyPanelRowStatus::YourTurn,
        }],
        0,
    );
    let texts: Vec<String> = daily_compact_lines(40, 6, &one)
        .iter()
        .map(line_text)
        .collect();
    assert_eq!(texts.len(), 2, "no empty slots pad a single game");
    assert!(texts[0].starts_with("► mira"));
}

#[test]
fn empty_slots_render_dashes() {
    let props = props_with(
        vec![DailyPanelMatchRow {
            opponent: "mira".to_string(),
            status: DailyPanelRowStatus::YourTurn,
        }],
        0,
    );
    let texts: Vec<String> = daily_panel_lines(21, &props)
        .iter()
        .map(line_text)
        .collect();
    assert!(texts[0].starts_with("► mira"));
    assert!(texts[0].trim_end().ends_with("your turn"));
    assert_eq!(texts[1].trim_end(), "  ─");
    assert_eq!(texts[2].trim_end(), "  ─");
    assert_eq!(texts[3].trim_end(), "  ─");
    assert_eq!(texts[4].trim_end(), "0 open · 1/4");
}

#[test]
fn outcome_rows_announce_results() {
    let props = props_with(
        vec![
            DailyPanelMatchRow {
                opponent: "mira".to_string(),
                status: DailyPanelRowStatus::Won,
            },
            DailyPanelMatchRow {
                opponent: "c0ld".to_string(),
                status: DailyPanelRowStatus::Lost,
            },
            DailyPanelMatchRow {
                opponent: "kal".to_string(),
                status: DailyPanelRowStatus::Draw,
            },
        ],
        0,
    );
    let texts: Vec<String> = daily_panel_lines(21, &props)
        .iter()
        .map(line_text)
        .collect();
    assert!(texts[0].starts_with("► mira"));
    assert!(texts[0].trim_end().ends_with("you won"));
    assert!(texts[1].starts_with("► c0ld"));
    assert!(texts[1].trim_end().ends_with("you lost"));
    assert!(texts[2].starts_with("► kal"));
    assert!(texts[2].trim_end().ends_with("draw"));
}

#[test]
fn hints_line_shows_both_lobby_keys() {
    let props = props_with(Vec::new(), 0);
    let hint = line_text(&daily_panel_lines(21, &props)[5]);
    assert_eq!(hint.trim_end(), "ctrl+g · ` toggle");
}

#[test]
fn status_line_merges_open_count_and_entries() {
    let props = props_with(Vec::new(), 2);
    let texts: Vec<String> = daily_panel_lines(30, &props)
        .iter()
        .map(line_text)
        .collect();
    assert_eq!(texts[4].trim_end(), "2 open · 1/4");
}
