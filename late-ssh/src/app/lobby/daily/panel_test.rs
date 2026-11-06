use super::*;

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn props_with(open_count: usize) -> DailyPanelProps {
    DailyPanelProps {
        open_count,
        lobby_glow: false,
        entry_count: 1,
        entry_cap: 4,
    }
}

fn feed(n: usize, state: DailyFeedState) -> Vec<DailyFeedRow> {
    (0..n)
        .map(|i| DailyFeedRow {
            game: "chess",
            who: format!("p{i}·q{i}"),
            state,
            age: format!("{i}m"),
        })
        .collect()
}

#[test]
fn compact_shape_is_the_feed_rows_and_one_footer() {
    let quiet = props_with(2);
    let texts: Vec<String> = daily_compact_lines(40, 6, &[], &quiet)
        .iter()
        .map(line_text)
        .collect();
    assert_eq!(
        texts.len(),
        3,
        "a quiet board is one note, the counts and the keys"
    );
    assert_eq!(texts[0].trim_end(), "no tables yet");
    assert_eq!(texts[1].trim_end(), "2 open · 1/4");
    assert_eq!(texts[2], "^G open  ` toggle");
    // A tile too narrow for both keys drops the last one whole.
    let narrow: Vec<String> = daily_compact_lines(12, 3, &[], &quiet)
        .iter()
        .map(line_text)
        .collect();
    assert_eq!(narrow[2], "^G open");

    let busy = props_with(0);
    let texts: Vec<String> = daily_compact_lines(40, 4, &feed(6, DailyFeedState::Playing), &busy)
        .iter()
        .map(line_text)
        .collect();
    assert_eq!(texts.len(), 4, "rows past the area drop, the footer stays");
    assert_eq!(texts[0], format!("● chess p0·q0{}0m", " ".repeat(40 - 15)));
    assert!(texts[1].starts_with("● chess p1·q1"));
    assert!(texts[2].starts_with("0 open"));
    assert_eq!(texts[3], "^G open  ` toggle");

    // An open challenge names the challenger alone behind the open marker;
    // a done table is the faint dot. Long names give way to the age.
    let open = DailyFeedRow {
        game: "backgammon",
        who: "someone_with_a_long_name".to_string(),
        state: DailyFeedState::Open,
        age: "3h".to_string(),
    };
    let line = line_text(&feed_line(21, &open));
    assert_eq!(line.chars().count(), 21, "{line:?}");
    assert!(line.starts_with("○ backgammon s"), "{line:?}");
    assert!(line.ends_with("… 3h"), "{line:?}");
    let done = line_text(&feed_line(21, &feed(1, DailyFeedState::Done)[0]));
    assert!(done.starts_with("· chess p0·q0"), "{done:?}");
}
