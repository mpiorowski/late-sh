use super::*;
use crate::app::common::composer::build_composer_rows;
use late_core::models::chat_message_reaction::ChatMessageReactionSummary;
use ratatui::style::Color;

fn lines_to_strings(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn parse_news_payload_splits_marker_payload() {
    let body = "---NEWS--- Title || Summary line || https://example.com || .:-\\n+*#";
    let payload = parse_news_payload(body).expect("payload");
    assert_eq!(payload.title, "Title");
    assert_eq!(payload.summary, "Summary line");
    assert_eq!(payload.url, "https://example.com");
    assert_eq!(payload.ascii_art, ".:-\n+*#");
}

#[test]
fn parse_news_payload_requires_marker_at_start() {
    assert!(parse_news_payload("hello ---NEWS--- Fake || summary || url || ascii").is_none());
    assert!(parse_news_payload("  ---NEWS--- Title || Summary || url || ascii").is_some());
}

#[test]
fn parse_report_payload_requires_marker_at_start() {
    assert_eq!(
        parse_report_payload("---BUG--- the door ate my hat"),
        Some((ReportKind::Bug, "the door ate my hat"))
    );
    assert_eq!(
        parse_report_payload("  ---SUGGESTION--- more cats"),
        Some((ReportKind::Suggestion, "more cats"))
    );
    assert!(parse_report_payload("hello ---BUG--- fake").is_none());
    assert!(parse_report_payload("regular message").is_none());
}

#[test]
fn wrap_chat_entry_to_lines_renders_report_card() {
    let wrapped = wrap_chat_entry_to_lines(
        "---BUG--- the door ate my hat",
        "[now]",
        "mat",
        40,
        Style::default(),
        None,
        Style::default(),
        false,
        false,
        None,
        None,
        &[],
    );
    let lines = lines_to_strings(&wrapped.lines);
    assert_eq!(lines[0], " mat filed a bug [now]");
    assert!(lines[1].contains('─'), "{lines:?}");
    assert!(lines[2].contains("🐛 the door ate my hat"), "{lines:?}");
    assert!(lines.last().unwrap().contains('─'), "{lines:?}");
    assert_eq!(wrapped.header_line_index, None);
}

#[test]
fn wrap_chat_entry_to_lines_renders_action_message() {
    let body = crate::app::chat::action::encode_action_body("waves").expect("action");
    let wrapped = wrap_chat_entry_to_lines(
        &body,
        "[now]",
        "mat",
        80,
        Style::default(),
        None,
        Style::default(),
        false,
        false,
        None,
        None,
        &[],
    );
    assert_eq!(lines_to_strings(&wrapped.lines), vec![" * mat waves"]);
    assert_eq!(wrapped.header_line_index, None);
}

#[test]
fn format_news_ascii_art_for_display_limits_to_requested_rows() {
    let art = "abc\ndef\nghi\njkl";
    let lines = format_news_ascii_art_for_display(art, 2);
    assert_eq!(lines, vec!["abc".to_string(), "def".to_string()]);
}

#[test]
fn format_news_ascii_art_for_display_drops_blank_rows_and_trims_right_edge() {
    let art = "\n   \n  abc  \n\\n def\t \n";
    let lines = format_news_ascii_art_for_display(art, 6);
    assert_eq!(lines, vec!["  abc".to_string(), " def".to_string()]);
}

#[test]
fn format_news_ascii_art_for_display_allows_short_or_empty_art() {
    assert_eq!(
        format_news_ascii_art_for_display("one\n\n", 6),
        vec!["one".to_string()]
    );
    assert!(format_news_ascii_art_for_display("\n  \n", 6).is_empty());
    assert!(format_news_ascii_art_for_display("one", 0).is_empty());
}

#[test]
fn wrap_news_to_lines_renders_rules_with_ascii_left() {
    let lines = wrap_news_to_lines(
        "[1m]",
        "mat: ",
        120,
        Style::default(),
        NewsPayload {
            title: "Title".to_string(),
            summary: "• first bullet".to_string(),
            url: "https://example.com".to_string(),
            ascii_art: ".:-\n+*#".to_string(),
        },
    );
    assert!(lines.len() >= 4);
    let rendered = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    for row in lines_to_strings(&lines) {
        assert!(
            row.starts_with(' '),
            "custom card row lost left padding: {row:?}"
        );
    }
    assert!(rendered.contains("shared news"));
    assert!(!rendered.contains("┌"));
    assert!(!rendered.contains("┐"));
    assert!(!rendered.contains("└"));
    assert!(!rendered.contains("┘"));
    assert!(rendered.contains("──"));
    assert!(
        rendered
            .lines()
            .filter(|line| line.trim().chars().all(|ch| ch == '─'))
            .count()
            >= 2
    );
    assert!(rendered.contains(".:-"));
    assert!(rendered.contains(" │ "));
    assert!(rendered.contains("Title"));
    assert!(rendered.contains("first bullet"));
    assert!(rendered.contains("https://example.com"));
}

#[test]
fn wrap_news_to_lines_respects_terminal_cell_width() {
    let width = 58;
    let lines = wrap_news_to_lines(
        "[4 mins ago]",
        "@artboard",
        width,
        Style::default(),
        NewsPayload {
            title: "Nobody understands the point of hybrid cars".to_string(),
            summary: "YouTube video by Technology Connections.\nOpen the link to watch on YouTube."
                .to_string(),
            url: "https://www.youtube.com/watch?v=KnUFH5GX_fI".to_string(),
            ascii_art: ".. .-:::----\n. .:==-.....\n:-:--:     .".to_string(),
        },
    );

    for rendered in lines_to_strings(&lines) {
        assert!(
            UnicodeWidthStr::width(rendered.as_str()) <= width,
            "line overflowed {width} cells: {rendered:?}"
        );
    }
}

#[test]
fn wrap_chat_entry_to_lines_appends_reaction_footer() {
    let wrapped = wrap_chat_entry_to_lines(
        "hello world",
        "[1m]",
        "alice",
        80,
        Style::default(),
        None,
        Style::default(),
        false,
        false,
        None,
        None,
        &[
            ChatMessageReactionSummary {
                icon: "🧡".to_string(),
                count: 3,
            },
            ChatMessageReactionSummary {
                icon: "🔥".to_string(),
                count: 1,
            },
        ],
    );
    let rendered = lines_to_strings(&wrapped.lines).join("\n");
    assert!(rendered.contains("[🧡 3]"));
    assert!(rendered.contains("[🔥 1]"));
}

#[test]
fn wrap_message_has_left_padding() {
    let lines = wrap_message_to_lines(
        "hello",
        "[1m]",
        "alice",
        80,
        Style::default(),
        None,
        Style::default(),
        false,
        false,
    );
    let strings = lines_to_strings(&lines);
    assert!(strings[0].starts_with(" alice"));
    assert!(strings[1].starts_with(" hello"));
}

#[test]
fn wrap_message_respects_newlines() {
    let lines = wrap_message_to_lines(
        "line1\nline2\nline3",
        "[1m]",
        "bob",
        80,
        Style::default(),
        None,
        Style::default(),
        false,
        false,
    );
    let strings = lines_to_strings(&lines);
    assert_eq!(strings.len(), 4);
    assert!(strings[1].contains("line1"));
    assert!(strings[2].contains("line2"));
    assert!(strings[3].contains("line3"));
}

#[test]
fn wrap_message_empty_body() {
    let lines = wrap_message_to_lines(
        "",
        "[1m]",
        "alice",
        80,
        Style::default(),
        None,
        Style::default(),
        false,
        false,
    );
    assert_eq!(lines.len(), 1);
}

#[test]
fn wrap_message_author_tint_splits_only_the_username() {
    let tint = AuthorTint {
        range: (4, 9), // "alice" inside "★ alice 🌱" ("★" is 3 bytes)
        word: None,
        name_style: None,
    };
    let lines = wrap_message_to_lines(
        "hello",
        "[1m]",
        "★ alice 🌱",
        80,
        Style::default(),
        Some(tint),
        Style::default(),
        false,
        false,
    );
    // pad + prefix-before + tinted-username + prefix-after + stamp
    let header = &lines[0];
    assert_eq!(header.spans.len(), 5);
    assert_eq!(header.spans[2].content.as_ref(), "alice");
    // Text is identical to the untinted render.
    let untinted = wrap_message_to_lines(
        "hello",
        "[1m]",
        "★ alice 🌱",
        80,
        Style::default(),
        None,
        Style::default(),
        false,
        false,
    );
    assert_eq!(lines_to_strings(&lines), lines_to_strings(&untinted));
}

#[test]
fn wrap_message_author_tint_ignores_bad_ranges() {
    let tint = AuthorTint {
        range: (0, 99),
        word: None,
        name_style: None,
    };
    let lines = wrap_message_to_lines(
        "hello",
        "[1m]",
        "alice",
        80,
        Style::default(),
        Some(tint),
        Style::default(),
        false,
        false,
    );
    assert_eq!(lines[0].spans.len(), 3);
}

#[test]
fn wrap_message_name_style_paints_per_char_over_author_style() {
    let tint = AuthorTint {
        range: (0, 5),
        word: None,
        name_style: Some(NameStyle::Solid(Color::Rgb(255, 200, 80))),
    };
    let author_style = Style::default()
        .fg(Color::Rgb(1, 2, 3))
        .add_modifier(Modifier::BOLD);
    let lines = wrap_message_to_lines(
        "hello",
        "12:04",
        "alice",
        80,
        author_style,
        Some(tint),
        Style::default(),
        false,
        false,
    );
    // pad + 5 per-char spans + stamp
    let header = &lines[0];
    assert_eq!(header.spans.len(), 7);
    let name: String = header.spans[1..6]
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert_eq!(name, "alice");
    for span in &header.spans[1..6] {
        // Effect fg wins over the author fg; BOLD survives.
        assert_eq!(span.style.fg, Some(Color::Rgb(255, 200, 80)));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }
}

#[test]
fn wrap_message_prints_drunk_word_between_name_and_stamp() {
    let tint = AuthorTint {
        range: (0, 5),
        word: Some("wasted"),
        name_style: None,
    };
    let lines = wrap_message_to_lines(
        "hello",
        "12:04",
        "alice",
        80,
        Style::default(),
        Some(tint),
        Style::default(),
        false,
        false,
    );
    // pad + tinted-username + " (wasted)" + " 12:04"
    let header = &lines[0];
    assert_eq!(header.spans.len(), 4);
    assert_eq!(header.spans[2].content.as_ref(), " (wasted)");
    assert!(
        header.spans[2]
            .style
            .add_modifier
            .contains(Modifier::ITALIC)
    );
    assert_eq!(header.spans[3].content.as_ref(), " 12:04");
}

#[test]
fn wrap_message_omits_drunk_word_when_absent() {
    // A name_style-only tint with no drunk word: header stays lean.
    let tint = AuthorTint {
        range: (0, 5),
        word: None,
        name_style: None,
    };
    let lines = wrap_message_to_lines(
        "hello",
        "12:04",
        "alice",
        80,
        Style::default(),
        Some(tint),
        Style::default(),
        false,
        false,
    );
    // pad + tinted-username + " 12:04" — no aside.
    assert_eq!(lines[0].spans.len(), 3);
    assert_eq!(lines[0].spans[2].content.as_ref(), " 12:04");
}

#[test]
fn composer_rows_soft_wrap_words() {
    let rows = build_composer_rows("hello wide world", 8);
    let texts: Vec<&str> = rows.iter().map(|row| row.text.as_str()).collect();
    assert_eq!(texts, vec!["hello", "wide", "world"]);
}
