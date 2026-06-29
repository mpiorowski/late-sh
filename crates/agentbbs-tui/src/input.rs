//! Keyboard handling — maps [`crossterm`] key events to state transitions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, ComposeField, Control, Screen, MENU};

impl App {
    /// Handle a single key event, returning whether to keep running.
    pub fn on_key(&mut self, key: KeyEvent) -> Control {
        // Any activity refreshes our entry in the shared presence registry.
        self.heartbeat();

        // Ctrl-C always quits, from anywhere.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Control::Quit;
        }

        match self.screen {
            Screen::Splash => self.key_splash(key),
            Screen::Main => self.key_main(key),
            Screen::Boards => self.key_boards(key),
            Screen::Read => self.key_read(key),
            Screen::Compose => self.key_compose(key),
            Screen::Arena => self.key_arena(key),
            Screen::Who | Screen::Doors | Screen::Market | Screen::Federation | Screen::Sysop => {
                self.key_panel(key)
            }
            Screen::Goodbye => {
                self.should_quit = true;
                Control::Quit
            }
        }
    }

    fn key_splash(&mut self, _key: KeyEvent) -> Control {
        self.screen = Screen::Main;
        self.status = "Main menu — use arrows or hotkeys.".into();
        Control::Continue
    }

    fn key_main(&mut self, key: KeyEvent) -> Control {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.menu_index = self.menu_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.menu_index + 1 < MENU.len() {
                    self.menu_index += 1;
                }
            }
            KeyCode::Enter => {
                let target = MENU[self.menu_index].2;
                self.goto(target);
            }
            KeyCode::Char(c) => {
                let up = c.to_ascii_uppercase();
                if let Some((_, _, target)) = MENU.iter().find(|(h, _, _)| *h == up) {
                    self.goto(*target);
                } else if up == 'Q' {
                    self.should_quit = true;
                    return Control::Quit;
                }
            }
            KeyCode::Esc => {
                self.should_quit = true;
                return Control::Quit;
            }
            _ => {}
        }
        Control::Continue
    }

    fn goto(&mut self, target: Screen) {
        match target {
            Screen::Boards => self.refresh_boards(),
            Screen::Goodbye => {}
            _ => {}
        }
        self.screen = target;
    }

    fn key_boards(&mut self, key: KeyEvent) -> Control {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.board_index = self.board_index.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.board_index + 1 < self.boards.len() {
                    self.board_index += 1;
                }
            }
            KeyCode::Enter => self.open_selected_board(),
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_read(&mut self, key: KeyEvent) -> Control {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.read_index = self.read_index.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                if self.read_index + 1 < self.messages.len() {
                    self.read_index += 1;
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.compose_field = ComposeField::Subject;
                self.status = "Compose — TAB switches field, Ctrl-S sends, ESC cancels.".into();
                self.screen = Screen::Compose;
            }
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Boards,
            _ => {}
        }
        Control::Continue
    }

    fn key_compose(&mut self, key: KeyEvent) -> Control {
        // Ctrl-S sends.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            self.submit_compose();
            return Control::Continue;
        }
        match key.code {
            KeyCode::Esc => {
                self.status = "Compose cancelled.".into();
                self.screen = Screen::Read;
            }
            KeyCode::Tab => {
                self.compose_field = match self.compose_field {
                    ComposeField::Subject => ComposeField::Body,
                    ComposeField::Body => ComposeField::Subject,
                };
            }
            KeyCode::Enter => match self.compose_field {
                ComposeField::Subject => self.compose_field = ComposeField::Body,
                ComposeField::Body => self.compose_body.push('\n'),
            },
            KeyCode::Backspace => {
                match self.compose_field {
                    ComposeField::Subject => self.compose_subject.pop(),
                    ComposeField::Body => self.compose_body.pop(),
                };
            }
            KeyCode::Char(c) => match self.compose_field {
                ComposeField::Subject => self.compose_subject.push(c),
                ComposeField::Body => self.compose_body.push(c),
            },
            _ => {}
        }
        Control::Continue
    }

    fn key_arena(&mut self, key: KeyEvent) -> Control {
        let count = self.arena.benchmarks().len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.arena_index = self.arena_index.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.arena_index + 1 < count {
                    self.arena_index += 1;
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_panel(&mut self, key: KeyEvent) -> Control {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }
}
