use std::cell::Cell;

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::app::{
    common::{primitives::Screen, theme},
    input::{MouseButton, MouseEventKind, ParsedInput},
    state::App,
};

const OPTIONS: [(u8, &str, &str, usize, Color); 3] = [
    (b'V', "View", "will ask next time too", 10, Color::Yellow),
    (b'A', "Always View", "will not ask again", 7, Color::Red),
    (
        b'B',
        "Back to Chat",
        "asks again next time",
        4,
        Color::Green,
    ),
];

const ASIDE: &str = "⁽ᵘˢᵘᵃˡˡʸ ᵇᵒᵒᵇⁱᵉˢ⁾";
const FOOTER: &str = "ʸᵒᵘ ᶜᵃⁿ ᵃˡʷᵃʸˢ ʳᵉˢᵉᵗ ᵗʰⁱˢ ᵈⁱᵃˡᵒᵍ ⁱⁿ ˢᵉᵗᵗⁱⁿᵍˢ⁻ᵗʷᵉᵃᵏˢ";

impl App {
    pub(crate) fn artboard_disclaimer_visible(&self) -> bool {
        let enabled = if self.show_settings {
            self.settings_modal_state.draft().artboard_disclaimer
        } else {
            self.profile_state.profile().artboard_disclaimer
        };
        self.screen == Screen::Artboard && enabled && !self.artboard_content_accepted
    }

    /// The board seat follows consent: connect only once the page may show,
    /// so a visitor idling on the prompt holds none of the board's seats.
    /// Idempotent; runs on page entry, on consent, and every tick (the saved
    /// tweak can land after the page was opened).
    pub(crate) fn sync_dartboard_connection(&mut self) {
        if self.screen == Screen::Artboard && !self.artboard_disclaimer_visible() {
            self.enter_dartboard();
        }
    }
}

pub(crate) fn handle_input(app: &mut App, event: &ParsedInput) {
    let choice = match event {
        ParsedInput::Byte(byte) => Some(byte.to_ascii_uppercase()),
        ParsedInput::Char(ch) if ch.is_ascii() => Some((*ch as u8).to_ascii_uppercase()),
        ParsedInput::Mouse(event)
            if app.interaction_mode.mouse_enabled()
                && event.kind == MouseEventKind::Down
                && event.button == Some(MouseButton::Left) =>
        {
            event
                .x
                .checked_sub(1)
                .zip(event.y.checked_sub(1))
                .and_then(|(x, y)| {
                    app.artboard_disclaimer_choices
                        .get()
                        .iter()
                        .position(|rect| rect.contains((x, y).into()))
                        .map(|index| OPTIONS[index].0)
                })
        }
        _ => None,
    };
    match choice {
        Some(b'V') => {
            app.artboard_content_accepted = true;
            app.sync_dartboard_connection();
        }
        Some(b'B' | 0x1B) => app.set_screen(Screen::Dashboard),
        Some(b'A') => {
            app.artboard_content_accepted = true;
            app.profile_state.dismiss_artboard_disclaimer();
            app.sync_dartboard_connection();
        }
        _ => {}
    }
}

pub(crate) fn draw(frame: &mut Frame, area: Rect, choices: &Cell<[Rect; 3]>) {
    // Clear the whole page, not just the dialog: no art may show around it.
    frame.render_widget(Clear, area);
    let warning = warning_line(false);
    let footer = Line::from(FOOTER).style(Style::default().fg(theme::TEXT_FAINT()));
    let width = warning.width().max(footer.width()) as u16 + 4;
    let vertical = Layout::vertical([Constraint::Length(14.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let popup = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0])[0];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let spacious = inner.height >= 12;
    let warning_row = u16::from(spacious);
    let compact = warning.width() > inner.width as usize;
    let warning = if compact { warning_line(true) } else { warning };
    // Centre the aside under NSFW, using the same measured warning as the renderer.
    let prefix = if compact {
        "Artboard may contain "
    } else {
        "/!\\     Artboard may contain "
    };
    let warning_x = inner.x + inner.width.saturating_sub(warning.width() as u16) / 2;
    let aside_width = Line::from(ASIDE).width() as u16;
    let aside_x = (warning_x + Line::from(prefix).width() as u16 + 2)
        .saturating_sub(aside_width / 2)
        .clamp(
            inner.x,
            inner.right().saturating_sub(aside_width.min(inner.width)),
        );
    frame.render_widget(Paragraph::new(warning).centered(), row(inner, warning_row));
    frame.render_widget(
        Paragraph::new(ASIDE).style(Style::default().fg(theme::TEXT_FAINT())),
        Rect::new(aside_x, inner.y + warning_row + 1, aside_width, 1).intersection(inner),
    );
    let mut rects = [Rect::default(); 3];
    for (index, (_, label, explanation, gap, color)) in OPTIONS.iter().enumerate() {
        let (initial, rest) = label.split_at(1);
        let line = Line::from(vec![
            Span::styled(initial, Style::default().fg(*color)),
            Span::raw(rest),
            Span::raw(" ".repeat(*gap)),
            Span::styled("(", Style::default().fg(theme::TEXT_DIM())),
            Span::raw(*explanation),
            Span::styled(")", Style::default().fg(theme::TEXT_DIM())),
        ])
        .style(Style::default().fg(theme::TEXT()));
        let option_row = row(
            inner,
            if spacious {
                4 + index as u16 * 2
            } else {
                2 + index as u16
            },
        );
        let line_width = (line.width() as u16).min(option_row.width);
        rects[index] = Rect::new(
            option_row.x + option_row.width.saturating_sub(line_width) / 2,
            option_row.y,
            line_width,
            option_row.height,
        );
        frame.render_widget(Paragraph::new(line).centered(), option_row);
    }
    let footer = if footer.width() > inner.width as usize {
        Line::from("ʳᵉˢᵉᵗ ⁱⁿ ˢᵉᵗᵗⁱⁿᵍˢ⁻ᵗʷᵉᵃᵏˢ").style(Style::default().fg(theme::TEXT_FAINT()))
    } else {
        footer
    };
    frame.render_widget(
        Paragraph::new(footer).centered(),
        row(inner, inner.height.saturating_sub(1)),
    );
    choices.set(rects);
}

fn row(area: Rect, index: u16) -> Rect {
    Rect::new(area.x, area.y.saturating_add(index), area.width, 1).intersection(area)
}

fn warning_line(compact: bool) -> Line<'static> {
    let mut spans = Vec::new();
    if !compact {
        spans.push(Span::raw("/!\\     "));
    }
    spans.extend([
        Span::raw("Artboard may contain "),
        Span::styled("NSFW", Style::default().fg(Color::Red)),
        Span::raw(" content"),
    ]);
    if !compact {
        spans.push(Span::raw("     /!\\"));
    }
    Line::from(spans).style(Style::default().fg(Color::Yellow))
}

#[cfg(test)]
#[path = "disclaimer_test.rs"]
mod disclaimer_test;
