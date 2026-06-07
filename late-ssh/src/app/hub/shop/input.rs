use late_core::models::pet::{PET_SPECIES_CAT, PET_SPECIES_DOG};

use crate::app::{
    common::primitives::Banner, hub::shop::state::RoomEffectTarget, input::ParsedInput, state::App,
};

pub fn handle_input(app: &mut App, event: &ParsedInput) -> bool {
    if app.shop_state.pending_room_effect().is_some() {
        match event {
            ParsedInput::Byte(b'\r' | b'\n' | b'y' | b'Y') | ParsedInput::Char('y' | 'Y') => {
                if let Some(banner) = app.shop_state.confirm_pending_room_effect() {
                    app.banner = Some(banner);
                }
                return true;
            }
            ParsedInput::Byte(0x1B | b'n' | b'N' | b'q' | b'Q')
            | ParsedInput::Char('n' | 'N' | 'q' | 'Q') => {
                if let Some(banner) = app.shop_state.cancel_pending_room_effect() {
                    app.banner = Some(banner);
                }
                return true;
            }
            _ => return true,
        }
    }

    match event {
        ParsedInput::Byte(b't' | b'T') | ParsedInput::Char('t' | 'T') => {
            if let Some(banner) = toggle_pet_species(app) {
                app.banner = Some(banner);
                return true;
            }
            false
        }
        ParsedInput::Arrow(b'A')
        | ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K') => {
            app.shop_state.move_selection(-1);
            true
        }
        ParsedInput::Arrow(b'B')
        | ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J') => {
            app.shop_state.move_selection(1);
            true
        }
        ParsedInput::Byte(b'[' | b'h' | b'H') | ParsedInput::Char('[' | 'h' | 'H') => {
            app.shop_state.select_previous_category();
            true
        }
        ParsedInput::Byte(b']' | b'l' | b'L') | ParsedInput::Char(']' | 'l' | 'L') => {
            app.shop_state.select_next_category();
            true
        }
        ParsedInput::Byte(b'\r' | b'\n') => {
            let current_room = current_room_effect_target(app);
            if let Some(banner) = app.shop_state.activate_selected(current_room) {
                app.banner = Some(banner);
            }
            true
        }
        ParsedInput::Byte(b'+' | b'=') | ParsedInput::Char('+' | '=') => {
            if let Some(banner) = app.shop_state.adjust_selected_aquarium_fish(1) {
                app.banner = Some(banner);
                return true;
            }
            false
        }
        ParsedInput::Byte(b'-' | b'_') | ParsedInput::Char('-' | '_') => {
            if let Some(banner) = app.shop_state.adjust_selected_aquarium_fish(-1) {
                app.banner = Some(banner);
                return true;
            }
            false
        }
        _ => false,
    }
}

fn current_room_effect_target(app: &App) -> Option<RoomEffectTarget> {
    let room_id = app.chat.selected_room_id?;
    let (room, _) = app.chat.rooms.iter().find(|(room, _)| room.id == room_id)?;
    let label = if room.kind == "dm" {
        "current DM".to_string()
    } else {
        room.slug
            .as_deref()
            .map(|slug| format!("#{slug}"))
            .unwrap_or_else(|| "current room".to_string())
    };
    Some(RoomEffectTarget {
        room_id,
        label,
        kind: room.kind.clone(),
        visibility: room.visibility.clone(),
        permanent: room.permanent,
        slug: room.slug.clone(),
    })
}

fn toggle_pet_species(app: &mut App) -> Option<Banner> {
    let item = app.shop_state.selected_item()?;
    if !item.is_pet_companion() || !item.owned {
        return None;
    }
    let next = if app.pet_state.species == PET_SPECIES_DOG {
        PET_SPECIES_CAT
    } else {
        PET_SPECIES_DOG
    };
    app.pet_state.set_species(next.to_string());
    Some(Banner::success(&format!(
        "Switched companion to {}",
        if next == PET_SPECIES_DOG {
            "dog"
        } else {
            "cat"
        }
    )))
}
