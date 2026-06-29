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
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn screen_text(app: &App, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        buffer
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn seeds_default_boards() {
        let app = App::in_memory();
        assert_eq!(app.boards.len(), 4);
        assert!(app.boards.iter().any(|b| b.slug == "general"));
    }

    #[test]
    fn splash_renders_banner() {
        let app = App::in_memory();
        let text = screen_text(&app, 100, 30);
        assert!(text.contains("AgentBBS"));
        assert!(text.contains("CONNECT"));
    }

    #[test]
    fn navigate_to_boards_and_back() {
        let mut app = App::in_memory();
        assert_eq!(app.screen, Screen::Splash);
        app.on_key(press(KeyCode::Enter)); // splash -> main
        assert_eq!(app.screen, Screen::Main);
        app.on_key(press(KeyCode::Char('m'))); // hotkey -> boards
        assert_eq!(app.screen, Screen::Boards);
        app.on_key(press(KeyCode::Esc)); // back to main
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn compose_and_post_flow() {
        let mut app = App::in_memory();
        let before = app.bbs.store().message_count().unwrap();

        app.on_key(press(KeyCode::Enter)); // -> main
        app.on_key(press(KeyCode::Char('M'))); // -> boards
        app.on_key(press(KeyCode::Enter)); // open first board -> read
        assert_eq!(app.screen, Screen::Read);
        app.on_key(press(KeyCode::Char('P'))); // -> compose
        assert_eq!(app.screen, Screen::Compose);

        for c in "Hello".chars() {
            app.on_key(press(KeyCode::Char(c)));
        }
        app.on_key(press(KeyCode::Tab)); // -> body
        for c in "first post".chars() {
            app.on_key(press(KeyCode::Char(c)));
        }
        app.on_key(ctrl('s')); // send

        let after = app.bbs.store().message_count().unwrap();
        assert_eq!(after, before + 1);
        assert_eq!(app.screen, Screen::Read);
        assert!(app.messages.iter().any(|m| m.body.body == "first post"));
        // Posted message must verify.
        assert!(app.messages.last().unwrap().verify().is_ok());
    }

    #[test]
    fn empty_message_is_rejected() {
        let mut app = App::in_memory();
        app.on_key(press(KeyCode::Enter));
        app.on_key(press(KeyCode::Char('M')));
        app.on_key(press(KeyCode::Enter));
        app.on_key(press(KeyCode::Char('P')));
        let before = app.bbs.store().message_count().unwrap();
        app.on_key(ctrl('s')); // send with empty body
        assert_eq!(app.bbs.store().message_count().unwrap(), before);
        assert!(app.status.contains("empty"));
    }

    #[test]
    fn goodbye_quits() {
        let mut app = App::in_memory();
        app.on_key(press(KeyCode::Enter)); // -> main
        app.on_key(press(KeyCode::Char('G'))); // -> goodbye screen
        assert_eq!(app.screen, Screen::Goodbye);
        let ctl = app.on_key(press(KeyCode::Enter));
        assert_eq!(ctl, Control::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn arena_shows_leaderboard() {
        let mut app = App::in_memory();
        assert!(app.arena.submission_count() >= 3);
        app.on_key(press(KeyCode::Enter)); // -> main
        app.on_key(press(KeyCode::Char('A'))); // -> arena
        assert_eq!(app.screen, Screen::Arena);
        let text = screen_text(&app, 110, 30);
        assert!(text.contains("Arena Leaderboard"));
        assert!(text.contains("CVE-Bench"));
        app.on_key(press(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn sysop_panel_renders() {
        let mut app = App::in_memory();
        app.on_key(press(KeyCode::Enter));
        app.on_key(press(KeyCode::Char('S'))); // sysop panel
        let text = screen_text(&app, 100, 30);
        assert!(text.contains("Sysop Report"));
    }

    #[test]
    fn shared_presence_sees_other_sessions() {
        use agentbbs_core::{MemoryStore, Presence};
        use std::sync::Arc;
        let presence = Arc::new(Presence::default());
        let store: Arc<dyn agentbbs_core::Store> = Arc::new(MemoryStore::new());
        let a = App::with_presence(store.clone(), presence.clone());
        let b = App::with_presence(store.clone(), presence.clone());
        let (aid, bid) = (a.session.identity.id(), b.session.identity.id());
        let online = presence.online(10);
        assert!(online.len() >= 2);
        assert!(online.iter().any(|m| m.id == aid));
        assert!(online.iter().any(|m| m.id == bid));
        // Dropping a session leaves the registry.
        drop(b);
        assert!(presence.online(10).iter().all(|m| m.id != bid));
        let _ = a; // keep a alive until here
    }

    #[test]
    fn who_panel_renders_real_presence() {
        let mut app = App::in_memory();
        app.on_key(press(KeyCode::Enter));
        app.on_key(press(KeyCode::Char('W'))); // who's online
        let text = screen_text(&app, 110, 30);
        assert!(text.contains("Who's Online"));
        assert!(text.contains("(you)")); // our own session is listed
        assert!(text.contains("online"));
    }

    #[test]
    fn marketplace_renders_signed_listings() {
        let mut app = App::in_memory();
        app.on_key(press(KeyCode::Enter));
        app.on_key(press(KeyCode::Char('K'))); // marketplace
        assert_eq!(app.screen, Screen::Market);
        let text = screen_text(&app, 110, 30);
        assert!(text.contains("Marketplace"));
        assert!(text.contains("Echo Door"));
    }
}
