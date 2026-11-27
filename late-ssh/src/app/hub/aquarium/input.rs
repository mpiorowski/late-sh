//! The keys and clicks of the reef full screen (`a` on Home). The meal is
//! the house's one feed path (`input::feed_aquarium_globally`), the same a
//! click on the tile takes. `s` goes to the Shop for more fish.

use crate::app::{
    input::{MouseButton, MouseEventKind, ParsedInput},
    state::App,
};

pub(crate) fn handle_modal_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B | b'q' | b'Q' | b'a' | b'A')
        | ParsedInput::Char('q' | 'Q' | 'a' | 'A') => close(app),
        ParsedInput::Byte(b'f' | b'F') | ParsedInput::Char('f' | 'F') => {
            crate::app::input::feed_aquarium_globally(app);
        }
        // More fish: the Shop on the Companions tab, where the tank's
        // stock is sold. `/shop` cannot be typed from here.
        ParsedInput::Byte(b's' | b'S') | ParsedInput::Char('s' | 'S') => {
            crate::app::input::open_shop_modal_globally(app);
            app.shop_state
                .select_category(crate::app::hub::shop::catalog::ShopCategory::Companions);
        }
        ParsedInput::Mouse(mouse)
            if mouse.kind == MouseEventKind::Down && mouse.button == Some(MouseButton::Left) =>
        {
            crate::app::input::feed_aquarium_globally(app);
        }
        _ => {}
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    close(app);
}

fn close(app: &mut App) {
    app.show_aquarium_modal = false;
}
