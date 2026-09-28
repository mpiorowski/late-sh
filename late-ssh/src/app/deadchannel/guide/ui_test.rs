use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::draw;
use crate::app::deadchannel::guide::data::{Block, SECTIONS};
use crate::app::deadchannel::guide::state::State;

fn render(state: &State, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| draw(frame, frame.area(), state))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

/// Every frame from the top to the end of the scroll, concatenated.
fn read_to_the_end(state: &mut State, width: u16, height: u16) -> String {
    let mut seen = String::new();
    let mut last_scroll = None;
    while last_scroll != Some(state.scroll()) {
        last_scroll = Some(state.scroll());
        seen.push_str(&render(state, width, height));
        state.scroll_by(5);
    }
    seen
}

/// The copy as the runner reads it: the marks gone, the opening words
/// (short enough to sit on one row, where a wrap cannot split them).
fn opening(text: &str) -> String {
    text.chars()
        .filter(|ch| *ch != '`' && *ch != '*')
        .take(30)
        .collect()
}

#[test]
fn the_guide_shows_every_block_between_the_top_and_the_end() {
    let mut state = State::new();
    state.open();

    let seen = read_to_the_end(&mut state, 100, 60);
    assert!(seen.contains("the street, explained"));
    assert!(seen.contains("Esc  close"));
    for section in SECTIONS {
        assert!(seen.contains(section.title), "missing {}", section.title);
        for block in section.blocks {
            match block {
                Block::Loop(stages) => {
                    for stage in *stages {
                        assert!(seen.contains(stage), "missing {stage:?}");
                    }
                }
                Block::Prize { title, lines } => {
                    assert!(seen.contains(title), "missing {title:?}");
                    for line in *lines {
                        assert!(seen.contains(&opening(line)), "missing {line:?}");
                    }
                }
                Block::Keys(keys) => {
                    for key in *keys {
                        assert!(seen.contains(key.key), "missing {:?}", key.key);
                        assert!(seen.contains(&opening(key.does)), "missing {:?}", key.does);
                    }
                }
                Block::Figures(figures) => {
                    for figure in *figures {
                        assert!(seen.contains(figure.value), "missing {:?}", figure.value);
                        assert!(seen.contains(&opening(figure.means)), "missing {:?}", figure.means);
                    }
                }
                Block::Rule(text) => {
                    assert!(seen.contains(&opening(text)), "missing {text:?}");
                }
            }
        }
    }
    // The marks are read, never printed.
    assert!(!seen.contains('`'));
    assert!(!seen.contains('*'));
}

#[test]
fn the_short_version_is_the_first_screen() {
    let mut state = State::new();
    state.open();

    // The box at full size: the loop, the prize, and the keys all show
    // before a single scroll, since most runners read nothing else.
    let top = render(&state, 100, 45);
    assert!(top.contains("fight glyphs ▸ buy gear"));
    assert!(top.contains("40,000 chips"));
    assert!(top.contains("fight"));
    assert!(top.contains("this guide"));
    assert!(top.contains("gear wins fights."));
    assert!(!top.contains("the wire ─"));

    // The end after a long scroll, the top gone.
    state.scroll_by(1000);
    let end = render(&state, 100, 45);
    assert!(end.contains("opens it again."));
    assert!(!end.contains("fight glyphs ▸ buy gear"));
}

#[test]
fn a_narrow_frame_wraps_instead_of_cutting() {
    let mut state = State::new();
    state.open();

    // Forty columns: the keys fall to one column and every line of the
    // copy still shows somewhere between the top and the end.
    let seen = read_to_the_end(&mut state, 40, 30);
    for section in SECTIONS {
        for block in section.blocks {
            if let Block::Keys(keys) = block {
                for key in *keys {
                    let first_word = key.does.split(' ').next().unwrap();
                    assert!(seen.contains(first_word), "missing {:?}", key.does);
                }
            }
        }
    }
    assert!(seen.contains("40,000 chips"));
}
