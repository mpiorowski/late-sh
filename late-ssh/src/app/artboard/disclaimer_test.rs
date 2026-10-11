use super::*;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

fn text_position(buffer: &Buffer, text: &str) -> (u16, u16) {
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        if let Some(index) = row.find(text) {
            return (Line::from(&row[..index]).width() as u16, y);
        }
    }
    panic!("missing text: {text}");
}

#[test]
fn artboard_disclaimer_matches_requested_layout_and_colors() {
    for theme_id in ["late", "latte"] {
        theme::set_current_by_id(theme_id);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let choices = Cell::default();
        terminal
            .draw(|frame| draw(frame, frame.area(), &choices))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let warning = "/!\\     Artboard may contain NSFW content     /!\\";
        let (x, y) = text_position(buffer, warning);
        let colors = "YYY     YYYYYYYY YYY YYYYYYY RRRR YYYYYYY     YYY";
        for (index, color) in colors.chars().enumerate() {
            let expected = match color {
                'Y' => Color::Yellow,
                'R' => Color::Red,
                _ => continue,
            };
            assert_eq!(buffer[(x + index as u16, y)].fg, expected);
        }
        let aside = text_position(buffer, "⁽ᵘˢᵘᵃˡˡʸ ᵇᵒᵒᵇⁱᵉˢ⁾");
        assert_eq!(aside.1, y + 1);
        assert_eq!(buffer[aside].fg, theme::TEXT_FAINT());
        let expected_choices = [
            ("View          (will ask next time too)", Color::Yellow),
            ("Always View       (will not ask again)", Color::Red),
            ("Back to Chat    (asks again next time)", Color::Green),
        ];
        let mut positions = Vec::new();
        for (index, (text, color)) in expected_choices.iter().enumerate() {
            let (x, y) = text_position(buffer, text);
            positions.push((x, y));
            assert_eq!(buffer[(x, y)].fg, *color);
            for (offset, ch) in text.chars().enumerate().skip(1) {
                let expected = if ch == '(' || ch == ')' {
                    theme::TEXT_DIM()
                } else {
                    theme::TEXT()
                };
                assert_eq!(buffer[(x + offset as u16, y)].fg, expected);
            }
            assert!(choices.get()[index].contains((x, y).into()));
        }
        assert_eq!(positions[0].0, positions[1].0);
        assert_eq!(positions[1].0, positions[2].0);
        assert_eq!(positions[1].1, positions[0].1 + 2);
        assert_eq!(positions[2].1, positions[1].1 + 2);
        let footer = text_position(
            buffer,
            "ʸᵒᵘ ᶜᵃⁿ ᵃˡʷᵃʸˢ ʳᵉˢᵉᵗ ᵗʰⁱˢ ᵈⁱᵃˡᵒᵍ ⁱⁿ ˢᵉᵗᵗⁱⁿᵍˢ⁻ᵗʷᵉᵃᵏˢ",
        );
        assert_eq!(footer.1, positions[2].1 + 3);
        assert_eq!(buffer[footer].fg, theme::TEXT_FAINT());
        let top = text_position(buffer, "╔");
        let bottom = text_position(buffer, "╚");
        assert_eq!(bottom.1, top.1 + 13);
    }
    theme::set_current_by_id("late");
}

#[test]
fn artboard_disclaimer_keeps_choices_and_reset_hint_on_small_terminals() {
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    let choices = Cell::default();
    terminal
        .draw(|frame| draw(frame, frame.area(), &choices))
        .unwrap();
    let buffer = terminal.backend().buffer();
    text_position(buffer, "Artboard may contain NSFW content");
    let footer = text_position(buffer, "ʳᵉˢᵉᵗ ⁱⁿ ˢᵉᵗᵗⁱⁿᵍˢ⁻ᵗʷᵉᵃᵏˢ");
    assert_eq!(buffer[footer].fg, theme::TEXT_FAINT());
    for (index, label) in ["View", "Always View", "Back to Chat"].iter().enumerate() {
        let position = text_position(buffer, label);
        assert!(choices.get()[index].contains(position.into()));
    }
}
