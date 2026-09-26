use super::state::SharedBuffer;
use super::terminal_backend::GlyphIsolatingBackend;
use ratatui::{
    Terminal, TerminalOptions, Viewport,
    backend::{Backend, CrosstermBackend},
    buffer::Cell,
    layout::Rect,
    widgets::{Block, Paragraph},
};

/// What the client terminal sees, reduced to the things that decide where a
/// glyph lands: cursor moves and the text between them. Style sequences are
/// dropped, so adjacent text runs merge.
#[derive(Debug, PartialEq)]
enum Wire {
    /// `CSI row;col H`: row and column.
    Move {
        x: u16,
        y: u16,
    },
    /// `CSI col G`: column only, the row is left as it is.
    Column {
        x: u16,
    },
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
        let moved = match final_byte {
            'H' => {
                let (row, col) = params.split_once(';').expect("CUP carries row;col");
                Wire::Move {
                    x: col.parse::<u16>().unwrap() - 1,
                    y: row.parse::<u16>().unwrap() - 1,
                }
            }
            'G' => Wire::Column {
                x: params.parse::<u16>().expect("CHA carries col") - 1,
            },
            _ => continue,
        };
        if !run.is_empty() {
            out.push(Wire::Text(std::mem::take(&mut run)));
        }
        out.push(moved);
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
fn a_single_codepoint_re_anchors_the_next_cell_by_column() {
    // Box drawing is East Asian Ambiguous: a CJK-locale terminal draws it two
    // wide. That is at most one column off and never past the row end, so
    // the row is still known and the cheaper column move is enough.
    let out = draw(&[(0, 0, cell("─")), (1, 0, cell("x")), (2, 0, cell("y"))]);
    assert_eq!(
        out,
        vec![
            Wire::Move { x: 0, y: 0 },
            Wire::Text("─".to_string()),
            Wire::Column { x: 1 },
            Wire::Text("xy".to_string()),
        ]
    );
}

#[test]
fn a_row_change_after_a_glyph_takes_the_full_move() {
    let out = draw(&[(5, 0, cell("─")), (0, 1, cell("x"))]);
    assert_eq!(
        out,
        vec![
            Wire::Move { x: 5, y: 0 },
            Wire::Text("─".to_string()),
            Wire::Move { x: 0, y: 1 },
            Wire::Text("x".to_string()),
        ]
    );
}

#[test]
fn a_wide_single_codepoint_is_blanked_and_re_anchors_by_column() {
    // CJK: width 2 for unicode-width and for every terminal that has the
    // font; a terminal without it draws a one-column tofu. The blank keeps
    // cell 1 clean either way, and the row is certain.
    let out = draw(&[(0, 0, cell("中")), (2, 0, cell("x"))]);
    assert_eq!(
        out,
        vec![
            Wire::Move { x: 0, y: 0 },
            Wire::Text("  ".to_string()),
            Wire::Move { x: 0, y: 0 },
            Wire::Text("中".to_string()),
            Wire::Column { x: 2 },
            Wire::Text("x".to_string()),
        ]
    );
}

#[test]
fn a_wide_grapheme_is_blanked_before_the_glyph_and_takes_the_full_move() {
    // A VS16 emoji presentation sequence: width 2 for unicode-width, width 1
    // for a per-codepoint wcwidth terminal. The blank leaves cell 1 clean on
    // such a terminal, and since a sequence can run any number of columns
    // on the wrong terminal, the row is not trusted: `x` gets the full move.
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
fn every_multi_codepoint_grapheme_takes_the_full_move() {
    // A flag pair (two regional indicators, 2 wide), a ZWJ family (2 wide
    // for unicode-width, 6 for per-codepoint wcwidth) and a combining
    // sequence (e + acute, 1 wide): none of them may leave the row trusted.
    for (grapheme, next) in [
        ("\u{1F1F5}\u{1F1F1}", 2),
        ("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}", 2),
        ("e\u{0301}", 1),
    ] {
        let out = draw(&[(0, 0, cell(grapheme)), (next, 0, cell("x"))]);
        assert_eq!(
            out.last(),
            Some(&Wire::Text("x".to_string())),
            "{grapheme:?}: the next cell is written"
        );
        assert_eq!(
            out[out.len() - 2],
            Wire::Move { x: next, y: 0 },
            "{grapheme:?}: the next cell is placed with row and column"
        );
    }
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

/// The bytes one full repaint of a bordered frame costs, through `backend`.
/// Every panel and modal draws a border, so this is the chrome the isolation
/// rule taxes on every `force_full_repaint`, resize and Ctrl+R.
fn bordered_full_frame_bytes<B: Backend>(backend: B, shared: &SharedBuffer) -> usize {
    let viewport = Viewport::Fixed(Rect::new(0, 0, 160, 40));
    let mut terminal = Terminal::with_options(backend, TerminalOptions { viewport }).unwrap();
    terminal
        .draw(|frame| {
            let block = Block::bordered().title("late.sh");
            let inner = block.inner(frame.area());
            frame.render_widget(block, frame.area());
            frame.render_widget(
                Paragraph::new("hello, this is an ordinary ascii line of chat"),
                inner,
            );
        })
        .unwrap();
    shared.take().len()
}

#[test]
fn a_bordered_full_frame_stays_within_its_byte_budget() {
    // A 160x40 border is 396 non-ASCII cells. The stock backend writes each
    // horizontal edge as one run; this backend re-anchors after every cell,
    // by column on the same row. Measured: 1,896 bytes stock, 3,470 here.
    // The ceiling leaves a tenth of headroom, so reaching for the full
    // row-and-column move after every glyph (about 4,400 bytes) fails.
    let shared = SharedBuffer::default();
    let stock = bordered_full_frame_bytes(CrosstermBackend::new(shared.clone()), &shared);
    let isolating = bordered_full_frame_bytes(GlyphIsolatingBackend::new(shared.clone()), &shared);
    assert!(
        isolating <= 3_800,
        "a bordered 160x40 frame costs {isolating} bytes (stock backend: {stock})"
    );
    assert!(
        isolating <= stock * 2,
        "isolation more than doubled the frame: {isolating} bytes against {stock}"
    );
}
