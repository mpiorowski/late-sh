use super::state::SharedBuffer;
use super::terminal_backend::GlyphIsolatingBackend;
use ratatui::{
    Terminal, TerminalOptions, Viewport, backend::Backend, buffer::Cell, layout::Rect,
    widgets::Paragraph,
};

/// What the client terminal sees, reduced to the two things that decide
/// where a glyph lands: absolute cursor moves and the text between them.
/// Style sequences are dropped, so adjacent text runs merge.
#[derive(Debug, PartialEq)]
enum Wire {
    Move { x: u16, y: u16 },
    Text(String),
}

fn wire(bytes: &[u8]) -> Vec<Wire> {
    let text = String::from_utf8(bytes.to_vec()).expect("frame bytes are utf-8");
    let mut out = Vec::new();
    let mut run = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            run.push(ch);
            continue;
        }
        assert_eq!(chars.next(), Some('['), "only CSI sequences expected");
        let mut params = String::new();
        let final_byte = loop {
            let ch = chars.next().expect("unterminated CSI");
            if ('\x40'..='\x7e').contains(&ch) {
                break ch;
            }
            params.push(ch);
        };
        if final_byte != 'H' {
            continue;
        }
        if !run.is_empty() {
            out.push(Wire::Text(std::mem::take(&mut run)));
        }
        let (row, col) = params.split_once(';').expect("CUP carries row;col");
        out.push(Wire::Move {
            x: col.parse::<u16>().unwrap() - 1,
            y: row.parse::<u16>().unwrap() - 1,
        });
    }
    if !run.is_empty() {
        out.push(Wire::Text(run));
    }
    out
}

fn cell(symbol: &str) -> Cell {
    let mut cell = Cell::default();
    cell.set_symbol(symbol);
    cell
}

fn draw(cells: &[(u16, u16, Cell)]) -> Vec<Wire> {
    let shared = SharedBuffer::default();
    let mut backend = GlyphIsolatingBackend::new(shared.clone());
    backend
        .draw(cells.iter().map(|(x, y, cell)| (*x, *y, cell)))
        .unwrap();
    wire(&shared.take())
}

#[test]
fn ascii_run_stays_contiguous() {
    let out = draw(&[(0, 0, cell("a")), (1, 0, cell("b")), (2, 0, cell("c"))]);
    assert_eq!(
        out,
        vec![Wire::Move { x: 0, y: 0 }, Wire::Text("abc".to_string())]
    );
}

#[test]
fn non_ascii_cell_re_anchors_the_next_cell() {
    let out = draw(&[(0, 0, cell("─")), (1, 0, cell("x")), (2, 0, cell("y"))]);
    assert_eq!(
        out,
        vec![
            Wire::Move { x: 0, y: 0 },
            Wire::Text("─".to_string()),
            Wire::Move { x: 1, y: 0 },
            Wire::Text("xy".to_string()),
        ]
    );
}

#[test]
fn wide_cell_is_blanked_before_the_glyph_and_re_anchors() {
    // A VS16 emoji presentation sequence: width 2 for unicode-width, width 1
    // for a per-codepoint wcwidth terminal. The blank leaves cell 1 clean on
    // such a terminal and the absolute move puts `x` at cell 2 either way.
    let out = draw(&[(0, 0, cell("\u{2600}\u{FE0F}")), (2, 0, cell("x"))]);
    assert_eq!(
        out,
        vec![
            Wire::Move { x: 0, y: 0 },
            Wire::Text("  ".to_string()),
            Wire::Move { x: 0, y: 0 },
            Wire::Text("\u{2600}\u{FE0F}".to_string()),
            Wire::Move { x: 2, y: 0 },
            Wire::Text("x".to_string()),
        ]
    );
}

#[test]
fn chat_row_after_an_emoji_lands_at_its_own_column() {
    // The reported leak: "hi ☀️ w]" written as one run lets the terminal's
    // width for ☀️ decide where " w]" starts. Through the full ratatui
    // diff, every cell after the emoji must still be placed absolutely.
    let shared = SharedBuffer::default();
    let backend = GlyphIsolatingBackend::new(shared.clone());
    let viewport = Viewport::Fixed(Rect::new(0, 0, 10, 1));
    let mut terminal = Terminal::with_options(backend, TerminalOptions { viewport }).unwrap();

    terminal
        .draw(|frame| {
            frame.render_widget(Paragraph::new("hi \u{2600}\u{FE0F} w]"), frame.area());
        })
        .unwrap();

    // Cells that stay a blank space against the fresh buffer are not in the
    // diff, so the run after the emoji starts at the `w`.
    assert_eq!(
        wire(&shared.take()),
        vec![
            Wire::Move { x: 0, y: 0 },
            Wire::Text("hi".to_string()),
            Wire::Move { x: 3, y: 0 },
            Wire::Text("  ".to_string()),
            Wire::Move { x: 3, y: 0 },
            Wire::Text("\u{2600}\u{FE0F}".to_string()),
            Wire::Move { x: 6, y: 0 },
            Wire::Text("w]".to_string()),
        ]
    );

    // Scrolling the emoji away: the diff rewrites only the changed cells, and
    // each of them arrives with its own column, so nothing the previous
    // frame left behind can be mistaken for current content.
    terminal
        .draw(|frame| {
            frame.render_widget(Paragraph::new("hi there"), frame.area());
        })
        .unwrap();

    assert_eq!(
        wire(&shared.take()),
        vec![Wire::Move { x: 3, y: 0 }, Wire::Text("there".to_string())]
    );
}
