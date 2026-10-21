use dartboard_core::Pos;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect, style::Modifier};

use super::*;
use crate::app::deadchannel::poster::state::{Draft, Edit, test_card};
use crate::app::deadchannel::runner::state::Tint;

fn draw_bench(width: u16, height: u16, body: Rect, view: &PosterView<'_>) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw(frame, body, view)).unwrap();
    terminal.backend().buffer().clone()
}

fn screen(buffer: &Buffer) -> String {
    let mut screen = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            screen.push_str(buffer[(x, y)].symbol());
        }
        screen.push('\n');
    }
    screen
}

fn painted_draft(marks: i32) -> Draft {
    let mut draft = Draft::new(None, marks);
    draft.edit(Edit::Type('H'));
    draft.edit(Edit::Type('I'));
    draft
}

/// The poster on its paper at the body's top left, in the brush's colour,
/// the cursor reversed; the brush and the keys beside it; the body
/// recorded for the mouse.
#[test]
fn the_bench_draws_the_poster_and_the_keys_beside_it() {
    let bench = Bench::Open(Box::new(painted_draft(0)));
    let view = PosterView {
        bench: &bench,
        hung: false,
        writing: None,
        word: None,
    };
    let body = Rect::new(1, 1, 94, 14);
    let buffer = draw_bench(96, 16, body, &view);

    let white = rgb(tint_paint(Tint::White));
    assert_eq!(buffer[(1, 1)].symbol(), "H");
    assert_eq!((buffer[(1, 1)].fg, buffer[(1, 1)].bg), (white, PAPER));
    assert_eq!(buffer[(2, 1)].symbol(), "I");
    // The cursor sits after the typing, on paper.
    assert!(buffer[(3, 1)].modifier.contains(Modifier::REVERSED));
    // The paper ends at the poster's edge.
    assert_eq!(buffer[(32, 12)].bg, PAPER);
    assert_ne!(buffer[(33, 1)].bg, PAPER);

    let screen = screen(&buffer);
    for line in [
        "the poster  32 x 12",
        "unsaved: not on your profile yet",
        "one Old Signal mark opens every colour",
        "[^S] save it  [^D] burn",
        "[^K] tint  [^]] glyphs",
        "[Esc] back to the street",
    ] {
        assert!(screen.contains(line), "{line}\n{screen}");
    }
    let Bench::Open(draft) = &bench else {
        unreachable!("the bench was opened above");
    };
    assert_eq!(draft.drawn_at(), Some(body));
}

/// Armed, the burn's row says what the next `^D` does, in red; a saved
/// poster says where it hangs.
#[test]
fn an_armed_burn_warns_in_its_own_row() {
    let mut draft = painted_draft(0);
    draft.saved(draft.canvas.clone());
    draft.arm_burn();
    let bench = Bench::Open(Box::new(draft));
    let view = PosterView {
        bench: &bench,
        hung: true,
        writing: None,
        word: None,
    };
    let buffer = draw_bench(96, 16, Rect::new(1, 1, 94, 14), &view);
    let screen = screen(&buffer);
    assert!(screen.contains("on your profile"), "{screen}");
    let (row, line) = screen
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("[^D] again burns it. any other key keeps it."))
        .unwrap_or_else(|| panic!("the warning\n{screen}"));
    let column = line[..line.find("[^D]").unwrap()].chars().count() as u16;
    assert_eq!(Some(buffer[(column, row as u16)].fg), lit(Neon::Red).fg);
    assert!(!screen.contains("[^S] save it"), "{screen}");
}

/// On every colour the brush is its hex, and `^K` puts the picker over the
/// bench.
#[test]
fn the_full_palette_shows_the_colour_and_its_picker() {
    let mut bench = Bench::Open(Box::new(painted_draft(1)));
    let shown = |bench: &Bench| {
        screen(&draw_bench(
            96,
            16,
            Rect::new(1, 1, 94, 14),
            &PosterView {
                bench,
                hung: false,
                writing: None,
                word: None,
            },
        ))
    };
    let closed = shown(&bench);
    let hex = format!("#{}", color_picker::hex_of(tint_paint(Tint::White)));
    for line in [
        hex.as_str(),
        "every colour: the mark opened them",
        "[^K] colour",
    ] {
        assert!(closed.contains(line), "{line}\n{closed}");
    }
    assert!(!closed.contains("Presets"), "{closed}");

    let Bench::Open(draft) = &mut bench else {
        unreachable!("the bench was opened above");
    };
    draft.colour_key();
    let open = shown(&bench);
    for line in [" brush ", "Presets"] {
        assert!(open.contains(line), "{line}\n{open}");
    }
}

/// A body no wider than the poster keeps the poster and drops the column;
/// one shorter than it shows the rows it has.
#[test]
fn a_narrow_or_short_body_keeps_the_poster() {
    let bench = Bench::Open(Box::new(painted_draft(0)));
    let view = PosterView {
        bench: &bench,
        hung: false,
        writing: None,
        word: None,
    };
    let screen = screen(&draw_bench(40, 8, Rect::new(0, 0, 34, 6), &view));
    assert!(screen.starts_with("HI"), "{screen}");
    assert!(!screen.contains("the poster"), "{screen}");
}

#[test]
fn the_rows_say_why_there_is_nothing_to_paint() {
    let text = |bench: &Bench| {
        panel_rows(&PosterView {
            bench,
            hung: false,
            writing: None,
            word: None,
        })
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
    };
    assert_eq!(
        text(&Bench::Reading { marks: 0 }),
        vec!["the printer goes to find what hangs on your wall...".to_string()]
    );
    assert_eq!(
        text(&Bench::NoRunner),
        vec!["nothing to print. you have no runner yet.".to_string()]
    );
    assert_eq!(
        text(&Bench::Open(Box::new(Draft::new(None, 0)))).len(),
        POSTER_HEIGHT
    );
}

#[test]
fn canvas_lines_are_the_poster_cell_for_cell() {
    let card = test_card(3);
    let lines = canvas_lines(&card, None, None);
    assert_eq!(lines.len(), POSTER_HEIGHT);
    for (y, line) in lines.iter().enumerate() {
        assert_eq!(line.width(), POSTER_WIDTH);
        for (x, span) in line.spans.iter().enumerate() {
            let pos = Pos { x, y };
            assert_eq!(span.content, card.get(pos).to_string());
            assert_eq!(span.style.fg, Some(rgb(card.fg(pos).unwrap())));
        }
    }
}
