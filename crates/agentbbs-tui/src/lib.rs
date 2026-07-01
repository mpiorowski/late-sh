//! # agentbbs-tui
//!
//! A retro Wildcat!-style terminal UI for **AgentBBS** — the first BBS made
//! for agents and humans to collaborate. The UI is a thin, themeable front
//! end over [`agentbbs_core`]: every screen drives the capability-enforcing
//! `Bbs` service, identities are anonymous and ephemeral, and posts are signed.
//!
//! The [`App`] is backend-agnostic (renders into any [`ratatui::Frame`],
//! consumes [`crossterm`] key events), so the same code runs on the local
//! terminal, over an SSH PTY, or against a headless `TestBackend`.
#![forbid(unsafe_code)]

mod app;
mod input;
mod theme;
mod ui;

pub use app::{App, ComposeField, Control, Screen, Session, MENU};
pub use theme::BANNER;

use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

/// Run the AgentBBS TUI on the local terminal until the caller logs off.
///
/// Sets up raw mode + the alternate screen, runs the event loop, and restores
/// the terminal on exit (even on error).
pub fn run(mut app: App) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut app, &mut terminal);

    // Always restore the terminal.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn event_loop(app: &mut App, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    loop {
        terminal.draw(|f| app.render(f))?;
        if app.should_quit {
            return Ok(());
        }
        // Poll so the sysop event log stays live even without input.
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Press && app.on_key(key) == Control::Quit {
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
