use super::protocol::{Action, Availability, Edition, Slot};
use super::proxy::ProxyStatus;
use super::state::{Mode, State};
use crate::app::common::theme;
use crate::app::door::rebels::render::blit_screen;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

pub fn edition_label(edition: Edition) -> &'static str {
    match edition {
        Edition::Zork1 => "Zork I",
        Edition::Zork2 => "Zork II",
        Edition::Zork3 => "Zork III",
    }
}
fn title(text: impl Into<String>) -> Line<'static> {
    Line::styled(
        text.into(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )
}
fn choice(label: impl Into<String>, selected: bool, enabled: bool) -> Line<'static> {
    Line::styled(
        format!("{} {}", if selected { ">" } else { " " }, label.into()),
        Style::default().fg(if !enabled {
            theme::TEXT_FAINT()
        } else if selected {
            theme::SUCCESS()
        } else {
            theme::TEXT_BRIGHT()
        }),
    )
}
fn slot_line(label: &str, slot: Option<&Slot>) -> Line<'static> {
    let status = match slot.map(|slot| slot.status) {
        None => "unavailable".into(),
        Some(Availability::Missing) => "empty".into(),
        Some(Availability::Invalid) => "invalid save".into(),
        Some(Availability::Unavailable) => "unavailable".into(),
        Some(Availability::Ready) => slot
            .and_then(|slot| slot.saved_at)
            .and_then(|seconds| i64::try_from(seconds).ok())
            .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
            .map(|time| time.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "saved".into()),
    };
    Line::styled(
        format!("{label}: {status}"),
        Style::default().fg(theme::TEXT_DIM()),
    )
}
pub fn draw_landing(frame: &mut Frame, area: Rect, enabled: bool, scroll: u16) -> u16 {
    let mut lines = vec![Line::raw("")];
    lines.extend(zork_logo());
    lines.extend([
        Line::raw(""),
        Line::styled(
            "Zork I, Zork II, and Zork III on late.sh!",
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::raw("Explore the Great Underground Empire, one command at a time."),
        Line::raw(""),
        Line::raw("Choose an edition, then continue, load your manual save, or start anew."),
        Line::raw("Each edition has an autosave and one manual save, private to your account."),
        Line::raw(""),
        choice(
            if enabled {
                "Enter · choose a Zork"
            } else {
                "Currently unavailable"
            },
            true,
            enabled,
        ),
        Line::raw(""),
        title("Once inside"),
        Line::raw("Type commands such as LOOK, INVENTORY, NORTH, or OPEN MAILBOX."),
        Line::raw("SAVE replaces that edition's manual save; its description is optional."),
        Line::raw("RESTORE returns to the edition menu after confirmation."),
        Line::raw("` steps out while the game stays open. Autosave resumes at the last prompt."),
    ]);
    crate::app::door::landing::render_scrolled(
        frame,
        Rect::new(
            area.x.saturating_add(1),
            area.y,
            area.width.saturating_sub(1),
            area.height,
        ),
        Paragraph::new(lines)
            .style(Style::default().fg(theme::TEXT()))
            .wrap(Wrap { trim: false }),
        scroll,
    )
}

fn zork_logo() -> Vec<Line<'static>> {
    [
        "███████╗ ██████╗ ██████╗ ██╗  ██╗",
        "╚══███╔╝██╔═══██╗██╔══██╗██║ ██╔╝",
        "  ███╔╝ ██║   ██║██████╔╝█████╔╝ ",
        " ███╔╝  ██║   ██║██╔══██╗██╔═██╗ ",
        "███████╗╚██████╔╝██║  ██║██║  ██╗",
        "╚══════╝ ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝",
    ]
    .into_iter()
    .map(|line| {
        Line::styled(
            line,
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
    })
    .collect()
}

pub fn draw_page(frame: &mut Frame, area: Rect, state: &State) {
    if state.game_visible() {
        if let Some(proxy) = state.proxy() {
            if proxy.status() == ProxyStatus::Connecting {
                frame.render_widget(
                    Paragraph::new(format!("Starting {}…", edition_label(state.edition())))
                        .style(Style::default().fg(theme::TEXT())),
                    area,
                );
            } else {
                proxy.with_screen(|screen| blit_game(frame.buffer_mut(), area, screen));
            }
        }
        return;
    }
    let mut lines = vec![title("Zork Trilogy"), Line::raw("")];
    if !state.enabled() {
        lines.push(Line::styled(
            "Currently unavailable",
            Style::default().fg(theme::ERROR()),
        ));
    }
    match state.mode() {
        Mode::Editions => {
            for (index, edition) in Edition::ALL.into_iter().enumerate() {
                let suffix = if state.live() == Some(edition) {
                    " · game open"
                } else {
                    ""
                };
                lines.push(choice(
                    format!("{}{suffix}", edition_label(edition)),
                    state.selected() == index,
                    true,
                ));
            }
            lines.extend([
                Line::raw(""),
                Line::raw("Two saves per edition: autosave + manual save."),
                Line::raw("Enter choose · ↑/↓ select · Esc back to Games"),
            ]);
        }
        Mode::Actions | Mode::Confirm(_) => {
            lines.push(title(edition_label(state.edition())));
            let slots = state.slots(state.edition());
            lines.push(slot_line("Autosave", slots.map(|s| &s.autosave)));
            lines.push(slot_line("Manual save", slots.map(|s| &s.manual)));
            if let Some(slot) = slots.map(|s| &s.manual)
                && !slot.description.is_empty()
            {
                lines.push(Line::styled(
                    slot.description
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(128)
                        .collect::<String>(),
                    Style::default().fg(theme::TEXT_DIM()),
                ));
            }
            lines.push(Line::raw(""));
            for (index, label, action) in [
                (0, "Continue", Some(Action::Auto)),
                (1, "Load manual save", Some(Action::Manual)),
                (2, "Start new game", Some(Action::New)),
                (3, "Back", None),
            ] {
                lines.push(choice(
                    label,
                    state.action() == index,
                    action.is_none_or(|action| state.available(action)),
                ));
            }
            if let Mode::Confirm(action) = state.mode() {
                lines.push(Line::raw(""));
                lines.push(title(match action {
                    Action::Manual => "Load manual save and replace automatic progress? [y/N]",
                    _ => "Start a new game and replace automatic progress? [y/N]",
                }));
                lines.push(Line::raw("Your manual save is kept. Enter / Esc cancels."));
            } else {
                lines.extend([
                    Line::raw(""),
                    Line::raw("Enter choose · ↑/↓ select · Esc back · r refresh saves"),
                ]);
            }
        }
        Mode::Running => {}
    }
    if state.loading() {
        lines.push(Line::styled(
            "Reading saves…",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if !state.message().is_empty() {
        lines.push(Line::styled(
            state.message(),
            Style::default().fg(theme::ERROR()),
        ));
    }
    let inner = Rect::new(
        area.x.saturating_add(2),
        area.y,
        area.width.saturating_sub(4),
        area.height,
    );
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(theme::TEXT()))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn blit_game(buffer: &mut ratatui::buffer::Buffer, area: Rect, screen: &vt100::Screen) {
    blit_screen(buffer, area, screen);
    // Frotz uses the terminal's default foreground. The surrounding app can
    // change its background with OSC 11, so inherit the theme's matching text
    // color rather than the user's potentially opposite terminal foreground.
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buffer[(x, y)];
            if cell.fg == Color::Reset {
                cell.fg = theme::TEXT();
            }
        }
    }
}

#[cfg(test)]
#[path = "render_test.rs"]
mod render_test;
