use late_core::models::chat_message_gild::GildTier;

use crate::app::{common::primitives::Banner, input::ParsedInput, state::App};

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) => close(app),
        ParsedInput::Arrow(b'A')
        | ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K') => {
            app.gild_modal_state.move_selection(-1);
        }
        ParsedInput::Arrow(b'B')
        | ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J') => {
            app.gild_modal_state.move_selection(1);
        }
        ParsedInput::Byte(b'1') | ParsedInput::Char('1') => app.gild_modal_state.select_index(0),
        ParsedInput::Byte(b'2') | ParsedInput::Char('2') => app.gild_modal_state.select_index(1),
        ParsedInput::Byte(b'3') | ParsedInput::Char('3') => app.gild_modal_state.select_index(2),
        ParsedInput::Byte(b'\r' | b'\n') | ParsedInput::Char('\r' | '\n') => submit(app),
        _ => {}
    }
}

pub(crate) fn close(app: &mut App) {
    app.gild_modal_state.close();
    app.show_gild_modal = false;
}

/// Confirm the purchase. The affordability check here is a courtesy so the
/// buyer is not sent on a round trip for an answer the balance already gives;
/// the floor guard that actually decides lives in the chip move.
fn submit(app: &mut App) {
    let Some(submit) = app.gild_modal_state.submit() else {
        close(app);
        return;
    };
    if !can_afford(app.chip_balance, submit.tier) {
        app.banner = Some(Banner::error("Not enough chips for that tier"));
        return;
    }
    app.chat.gild_message(submit.message_id, submit.tier);
    close(app);
    app.banner = Some(Banner::info("Gilding..."));
}

/// A tier is affordable while paying for it leaves the chip floor standing.
pub(crate) fn can_afford(balance: i64, tier: GildTier) -> bool {
    balance - tier.price() >= late_core::models::chips::CHIP_FLOOR
}
