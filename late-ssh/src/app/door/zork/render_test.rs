use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

#[test]
fn native_menus_and_default_game_text_follow_light_and_dark_themes() {
    for id in ["github-light", "github-dark"] {
        theme::set_current_by_id(id);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let state = State::new(super::super::state::StateConfig {
            user_id: uuid::Uuid::from_u128(1),
            host: String::new(),
            port: 1,
            secret: String::new(),
            term: "xterm".into(),
            enabled: false,
            repaint: None,
        });
        terminal
            .draw(|frame| draw_page(frame, frame.area(), &state))
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(2, 0)].fg, theme::AMBER());
        assert_eq!(buffer[(2, 7)].symbol(), "T");
        assert_eq!(buffer[(2, 7)].fg, theme::TEXT());
        assert_ne!(theme::AMBER(), theme::BG_CANVAS());
        assert_ne!(theme::TEXT(), theme::BG_CANVAS());

        let mut parser = vt100::Parser::new(4, 20, 0);
        parser.process(b"\x1b[7mWest of House\x1b[0m\r\n>\x1b[31mred");
        let mut buffer = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 20, 4));
        blit_game(&mut buffer, Rect::new(0, 0, 20, 4), parser.screen());
        assert_eq!(buffer[(0, 0)].fg, theme::TEXT());
        assert!(buffer[(0, 0)].modifier.contains(Modifier::REVERSED));
        assert_eq!(buffer[(0, 1)].fg, theme::TEXT());
        assert_eq!(buffer[(1, 1)].fg, Color::Indexed(1));
    }
    theme::set_current_by_id("contrast");
}
