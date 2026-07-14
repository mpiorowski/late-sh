// Key routing for Lateania.
//
// Key scheme:
//   - Before choosing a class: 1-5 pick Warrior/Mage/Cleric/Rogue/Ranger.
//   - Movement: w/a/s/d and arrows (N/S/E/W); < or , up and
//     > or . down (also shown as a hint in-game when a room has a vertical exit).
//   - Combat: space/x attack; 1-9 use the ability in that action-bar slot (0 is
//     slot 10; deeper rosters cast from the Abilities panel); z flee.
//   - Death: while a corpse, r (or Enter) releases to the temple; g casts the
//     Resurrection rite on a fallen adventurer in the room (holy/nature classes).
//   - World: y works a resource node here (chop/mine/fish/forage/skin);
//     u opens the crafting panel where a craft station stands.
//   - Panels: c character, v abilities, o look, b shop, t inventory ("things"),
//     p the Stable (companion vendor) where one stands. In the Stable, Enter
//     buys the selected beast and x feeds/tends the one you have. n opens the
//     housing ledger (buy a deed at the clerk, furnish a home you own from inside).
//     In a list panel, 1-9 select a row, Enter activates (equip/use/buy),
//     w/s move the cursor, x sells (inventory). List panels auto-scroll to
//     follow the cursor; [ / ] scroll the cursor-less text panels.
//   - Esc leaves the world for the Games hub.
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
    // While composing a chat line, keys feed the line until Enter (send) or Esc
    // (cancel). This runs before the Esc-leaves check so Esc cancels compose
    // rather than leaving the world. Chat is world-local (never hits late.sh).
    if state.chat_active() {
        match byte {
            0x1B => state.chat_cancel(),
            b'\r' | b'\n' => state.chat_send(),
            0x7f | 0x08 => state.chat_backspace(),
            0x20..=0x7e => state.chat_push(byte as char),
            _ => {}
        }
        return InputAction::Handled;
    }
    // Lateania reserves Esc for returning to its landing page.
    if byte == 0x1B {
        return InputAction::Leave;
    }

    let view = state.view();
    if !view.joined {
        state.ensure_player_present();
        return InputAction::Handled;
    }

    // Class selection gate: until a class is chosen, w/s move the highlight and
    // Enter chooses it; 1-9 quick-pick the first nine; r rerolls the scores.
    if !view.classed {
        match byte {
            b'w' | b'W' | b'k' | b'K' => state.class_cursor_up(),
            b's' | b'S' | b'j' | b'J' => state.class_cursor_down(),
            b'\r' | b'\n' => state.choose_class_at_cursor(),
            b'1'..=b'9' => {
                let i = (byte - b'1') as usize;
                if i < Class::ALL.len() {
                    state.choose_class(Class::ALL[i]);
                } else {
                    return InputAction::Ignored;
                }
            }
            b'r' | b'R' => state.reroll(),
            _ => return InputAction::Ignored,
        }
        return InputAction::Handled;
    }

    // Dead gate: a fallen player is a corpse and can only wait for a
    // resurrection or release to the temple (r or Enter). Esc still leaves
    // (handled above).
    if view.dead {
        match byte {
            b'r' | b'R' | b'\r' | b'\n' => state.release(),
            _ => return InputAction::Ignored,
        }
        return InputAction::Handled;
    }

    // Archetype selection gate: once eligible at level 10, the view offers two
    // paths and nothing else is reachable until one is chosen. 1/2 pick.
    if !view.archetype_choices.is_empty() {
        match byte {
            b'1'..=b'9' => {
                let i = (byte - b'1') as usize;
                if i < view.archetype_choices.len() {
                    state.choose_archetype(i);
                } else {
                    return InputAction::Ignored;
                }
            }
            _ => return InputAction::Ignored,
        }
        return InputAction::Handled;
    }

    let panel = state.panel();
    let in_list = matches!(
        panel,
        Panel::Inventory
            | Panel::Shop
            | Panel::Examine
            | Panel::Titles
            | Panel::Follow
            | Panel::Stable
            | Panel::Housing
            | Panel::Portal
            | Panel::Appearance
            | Panel::Crafting
            | Panel::Abilities
    );

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

    // `0` reaches the tenth hotbar slot; rosters deeper than that cast from
    // the Abilities panel (v, then Enter on the row).
    if byte == b'0' && !in_list {
        state.use_ability(10);
        return InputAction::Handled;
    }

    // Batch-sell shortcuts, only inside the Inventory panel (so they don't shadow
    // the global meanings of A/C/J elsewhere): A = sell all loose gear, C = sell
    // commons, J = sell junk (anything that wouldn't improve you). All keep your
    // potions and equipped gear, and need a merchant present.
    if panel == Panel::Inventory {
        use super::svc::SellBatch;
        match byte {
            b'A' => {
                state.sell_batch(SellBatch::All);
                return InputAction::Handled;
            }
            b'C' => {
                state.sell_batch(SellBatch::Common);
                return InputAction::Handled;
            }
            b'J' | b'j' => {
                state.sell_batch(SellBatch::NonUpgrades);
                return InputAction::Handled;
            }
            _ => {}
        }
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
        b'p' | b'P' => {
            // The companion vendor only opens at a capital Stable.
            if view.stable.is_some() {
                state.toggle_panel(Panel::Stable);
            }
            InputAction::Handled
        }
        b'n' | b'N' => {
            // The housing ledger opens at the clerk or inside a home you own.
            if view.housing.is_some() {
                state.toggle_panel(Panel::Housing);
            }
            InputAction::Handled
        }
        b'i' | b'I' => {
            // The waystone menu opens when standing on a portal. (Moved off `y`,
            // which the gather action uses.)
            if view.portal.is_some() {
                state.open_portal();
            }
            InputAction::Handled
        }
        b'm' | b'M' => {
            // Toggle the whole-world atlas.
            state.toggle_panel(Panel::Map);
            InputAction::Handled
        }
        b'o' | b'O' => {
            // Open the Examine list (the "look at things" panel) and refresh the
            // room description in the log.
            state.toggle_panel(Panel::Examine);
            state.look();
            InputAction::Handled
        }
        b'k' | b'K' => {
            // Titles: a selectable list; choose which one to display.
            state.toggle_panel(Panel::Titles);
            InputAction::Handled
        }
        b'j' | b'J' => {
            // Quest journal (read-only).
            state.toggle_panel(Panel::Quests);
            InputAction::Handled
        }
        b'r' | b'R' => {
            // Word of recall: warp back to the Town Square (out of combat only).
            state.recall();
            InputAction::Handled
        }
        b'f' | b'F' => {
            // Toggle auto-following another adventurer in the room.
            state.follow();
            InputAction::Handled
        }
        b'g' | b'G' => {
            // Resurrection rite: revive the nearest fallen adventurer here.
            state.resurrect();
            InputAction::Handled
        }
        b'e' | b'E' => {
            // Open the appearance / bio builder.
            state.open_appearance();
            InputAction::Handled
        }
        b'y' | b'Y' => {
            // Work a resource node here (chop/mine/fish/forage/skin).
            state.gather();
            InputAction::Handled
        }
        b'u' | b'U' => {
            // The crafting panel opens where a craft station stands.
            if view.crafting.is_some() {
                state.toggle_panel(Panel::Crafting);
            }
            InputAction::Handled
        }
        b'\'' => {
            // Open the local chat line (say to the room). World-local; does not
            // leak into late.sh.
            state.open_chat();
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
            if panel == Panel::Follow {
                state.stop_follow();
            } else if panel == Panel::Stable {
                // At the Stable, the secondary action tends (feeds) your beast.
                state.feed_pet();
            } else if panel == Panel::Appearance {
                // The secondary action cycles the trait the other way.
                state.cycle_appearance(-1);
            } else if in_list {
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
        // Manual scroll for cursor-less text panels (character/abilities/quests).
        // List panels auto-follow their cursor, so these are no-ops there.
        b'[' => {
            state.scroll_text_up();
            InputAction::Handled
        }
        b']' => {
            state.scroll_text_down();
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
    // Arrow keys do nothing while composing a chat line (they'd otherwise move).
    if state.chat_active() {
        return true;
    }
    let in_list = matches!(
        state.panel(),
        Panel::Inventory
            | Panel::Shop
            | Panel::Examine
            | Panel::Titles
            | Panel::Follow
            | Panel::Stable
            | Panel::Housing
            | Panel::Appearance
            | Panel::Crafting
    );
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
