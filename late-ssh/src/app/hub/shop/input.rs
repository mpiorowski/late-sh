use late_core::models::pet::{PET_SPECIES_CAT, PET_SPECIES_DOG};

use crate::app::{common::primitives::Banner, input::ParsedInput, state::App};

pub fn handle_input(app: &mut App, event: &ParsedInput) -> bool {
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
        ParsedInput::Byte(b'[') | ParsedInput::Char('[') => {
            app.shop_state.select_previous_category();
            true
        }
        ParsedInput::Byte(b']') | ParsedInput::Char(']') => {
            app.shop_state.select_next_category();
            true
        }
        ParsedInput::Byte(b'\r' | b'\n') => {
            if let Some(banner) = app.shop_state.activate_selected() {
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
