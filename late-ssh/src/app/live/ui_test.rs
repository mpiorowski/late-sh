use super::*;

#[test]
fn the_card_picks_the_form_and_keeps_rows_for_the_messages() {
    let card = |width, height| Rect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let (size, strip, rest) = fit_live_strip(card(80, 30)).unwrap();
    assert_eq!(size, StripSize::Full);
    assert_eq!(strip.height, LIVE_STRIP_HEIGHT);
    assert_eq!(rest.height, 30 - LIVE_STRIP_HEIGHT);

    let (size, _, rest) = fit_live_strip(card(80, 21)).unwrap();
    assert_eq!(size, StripSize::Full);
    assert_eq!(rest.height, 12, "the messages keep the larger share");

    let (size, strip, _) = fit_live_strip(card(80, 20)).unwrap();
    assert_eq!(
        size,
        StripSize::Compact,
        "a short card gets the one-row form"
    );
    assert_eq!(strip.height, LIVE_STRIP_COMPACT_HEIGHT);

    let (size, _, _) = fit_live_strip(card(40, 30)).unwrap();
    assert_eq!(size, StripSize::Compact, "a narrow card too");

    assert!(fit_live_strip(card(80, 4)).is_none(), "no room at all");
}

#[test]
fn a_picture_wider_than_its_column_never_touches_the_words() {
    let picture_row = "x".repeat(usize::from(PICTURE_COLS) + 5);
    let body = StripBody {
        picture: (0..PICTURE_ROWS)
            .map(|_| Line::from(picture_row.clone()))
            .collect(),
        words: (0..PICTURE_ROWS)
            .map(|_| vec![Span::raw("words")])
            .collect(),
        glow: false,
    };
    let lines = frame_lines(80, body);
    let words_at = usize::from(PICTURE_COLS + GAP);
    for line in &lines[..PICTURE_ROWS as usize] {
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        let chars: Vec<char> = text.chars().collect();
        assert_eq!(
            chars[words_at..].iter().collect::<String>(),
            "words",
            "the words start at their column: {text:?}"
        );
        assert!(
            chars[usize::from(PICTURE_COLS)..words_at]
                .iter()
                .all(|c| *c == ' '),
            "the gap stays blank: {text:?}"
        );
    }
}

#[test]
fn every_key_in_a_hint_stands_out_until_it_no_longer_fits() {
    let parts = [
        HintPart::Key("o"),
        HintPart::Text(" read · "),
        HintPart::Key("r"),
        HintPart::Text(" reply"),
    ];
    let spans = key_hint_spans(16, &parts);
    let colours: Vec<(String, Option<ratatui::style::Color>)> = spans
        .iter()
        .map(|span| (span.content.to_string(), span.style.fg))
        .collect();
    assert_eq!(
        colours,
        vec![
            ("o".to_string(), Some(theme::AMBER_DIM())),
            (" read · ".to_string(), Some(theme::TEXT_FAINT())),
            ("r".to_string(), Some(theme::AMBER_DIM())),
            (" reply".to_string(), Some(theme::TEXT_FAINT())),
        ]
    );

    let narrow = key_hint_spans(10, &parts);
    assert_eq!(narrow.len(), 1, "too narrow, one faint run");
    assert_eq!(narrow[0].content, "o read · …");
    assert_eq!(narrow[0].style.fg, Some(theme::TEXT_FAINT()));
}
