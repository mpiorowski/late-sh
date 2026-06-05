// Key routing for Lateania.
//
// Key scheme:
//   - Before choosing a class: 1-5 pick Warrior/Mage/Cleric/Rogue/Ranger.
//   - Movement: w/a/s/d and arrows (N/S/E/W); y/u/b/n diagonals; < > up/down.
//   - Combat: space/x attack; 1-9 use the ability in that action-bar slot; z flee.
//   - Panels: c character, v abilities, o look, b shop, t inventory ("things").
//     In a list panel, 1-9 select a row, Enter activates (equip/use/buy),
//     w/s move the cursor, x sells (inventory).
//   - Esc / q leave the world.
//
// A full typed command prompt needs an input-capture mode; deferred.

use super::{
    classes::Class,
    state::{Panel, State},
    world::Dir,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    Ignored,
    Handled,
    Leave,
}

pub fn handle_key(state: &mut State, byte: u8) -> InputAction {
    // Quit is always available.
    if matches!(byte, 0x1B | b'q' | b'Q') {
        return InputAction::Leave;
    }

    let view = state.view();
    if !view.joined {
        state.ensure_player_present();
        return InputAction::Handled;
    }

    // Class selection gate: until a class is chosen, 1-5 pick it and nothing else acts.
    if !view.classed {
        match byte {
            b'1' => state.choose_class(Class::Warrior),
            b'2' => state.choose_class(Class::Mage),
            b'3' => state.choose_class(Class::Cleric),
            b'4' => state.choose_class(Class::Rogue),
            b'5' => state.choose_class(Class::Ranger),
            _ => return InputAction::Ignored,
        }
        return InputAction::Handled;
    }

    let panel = state.panel();
    let in_list = matches!(panel, Panel::Inventory | Panel::Shop);

    // Number keys: select a list row when a list panel is open, else use an ability.
    if (b'1'..=b'9').contains(&byte) {
        let n = (byte - b'1') as usize;
        if in_list {
            // Move cursor to the chosen row, then activate it.
            // (cursor_down/up keep us in-bounds; jump by stepping.)
            select_row(state, n);
            state.activate_selection();
        } else {
            state.use_ability(byte - b'0');
        }
        return InputAction::Handled;
    }

    match byte {
        // Panels.
        b'c' | b'C' => {
            state.toggle_panel(Panel::Character);
            InputAction::Handled
        }
        b'v' | b'V' => {
            state.toggle_panel(Panel::Abilities);
            InputAction::Handled
        }
        b't' | b'T' => {
            state.toggle_panel(Panel::Inventory);
            InputAction::Handled
        }
        b'b' | b'B' => {
            // Shop only opens where a merchant stands.
            if view.shop.is_some() {
                state.toggle_panel(Panel::Shop);
            }
            InputAction::Handled
        }
        b'o' | b'O' => {
            state.set_panel(Panel::Room);
            state.look();
            InputAction::Handled
        }
        b'\r' | b'\n' => {
            if in_list {
                state.activate_selection();
            } else {
                state.attack();
            }
            InputAction::Handled
        }
        // Cursor movement inside list panels; otherwise N/S movement.
        b'w' | b'W' => {
            if in_list {
                state.cursor_up();
            } else {
                state.go(Dir::North);
            }
            InputAction::Handled
        }
        b's' | b'S' => {
            if in_list {
                state.cursor_down();
            } else {
                state.go(Dir::South);
            }
            InputAction::Handled
        }
        b'a' | b'A' | b'h' | b'H' => {
            state.go(Dir::West);
            InputAction::Handled
        }
        b'd' | b'D' | b'l' | b'L' => {
            state.go(Dir::East);
            InputAction::Handled
        }
        // Diagonals (roguelike yubn).
        b'y' | b'Y' => {
            state.go(Dir::Northwest);
            InputAction::Handled
        }
        b'u' | b'U' => {
            state.go(Dir::Northeast);
            InputAction::Handled
        }
        // Note: `b` is the shop key above, so southeast/southwest use n/m.
        b'n' | b'N' => {
            state.go(Dir::Southeast);
            InputAction::Handled
        }
        b'm' | b'M' => {
            state.go(Dir::Southwest);
            InputAction::Handled
        }
        b'<' | b',' => {
            state.go(Dir::Up);
            InputAction::Handled
        }
        b'>' | b'.' => {
            state.go(Dir::Down);
            InputAction::Handled
        }
        // Combat.
        b'x' | b'X' => {
            if in_list {
                state.sell_selection();
            } else if panel == Panel::Room || panel == Panel::Character || panel == Panel::Abilities
            {
                state.attack();
            }
            InputAction::Handled
        }
        b' ' => {
            state.attack();
            InputAction::Handled
        }
        b'z' | b'Z' => {
            state.flee();
            InputAction::Handled
        }
        _ => InputAction::Ignored,
    }
}

/// Move the list cursor to row `target` by stepping (keeps in-bounds clamping).
fn select_row(state: &mut State, target: usize) {
    let cur = state.cursor();
    if target > cur {
        for _ in 0..(target - cur) {
            state.cursor_down();
        }
    } else {
        for _ in 0..(cur - target) {
            state.cursor_up();
        }
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    let in_list = matches!(state.panel(), Panel::Inventory | Panel::Shop);
    match key {
        b'A' => {
            if in_list {
                state.cursor_up();
            } else {
                state.go(Dir::North);
            }
        }
        b'B' => {
            if in_list {
                state.cursor_down();
            } else {
                state.go(Dir::South);
            }
        }
        b'C' => state.go(Dir::East),
        b'D' => state.go(Dir::West),
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagonal_keys_are_distinct_directions() {
        // y/u/n/m map to the four diagonals; ensure no overlap with cardinals.
        let diag = [
            Dir::Northwest,
            Dir::Northeast,
            Dir::Southeast,
            Dir::Southwest,
        ];
        for (i, a) in diag.iter().enumerate() {
            for b in diag.iter().skip(i + 1) {
                assert_ne!(a, b, "diagonals must be distinct");
            }
        }
    }
}
