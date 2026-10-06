// Keyboard translation shared by every vt100-backed door. The ncurses games
// (Brogue, DCSS, NetHack under its opt-in curses windowport, dopewars) are the
// ones that actually request the mode; the rest (Usurper, BashQuest, CodeKeep,
// Rebels) are wired through the same gate so the bug class is closed once,
// and for them it stays a no-op.
//
// A door puts a `vt100::Parser` between the player's terminal and the game, and
// that parser is where the guest's terminal-mode requests stop: we render its
// cell grid, never its modes. Application cursor mode (DECCKM, `ESC [ ? 1 h`)
// is the one mode whose loss the player feels, because ncurses asks for it on
// every `keypad(win, TRUE)` (the `smkx` capability) and then decodes the arrow
// keys strictly from terminfo, where `kcuu1=\EOA` on every modern terminal
// (xterm, alacritty, kitty, ghostty, wezterm, tmux, screen; only the `linux`
// console sends CSI). The player's terminal never saw the request, so it keeps
// sending the CSI form, and the game decodes `ESC [ A` as three separate
// keystrokes: ESC, `[`, `A`. In brogue that is cancel + "turn on autopilot?".
//
// So the door speaks the mode the game actually asked for, which is what a real
// terminal does. The parser tracks the mode for us (`Screen::application_cursor`)
// and it flips back on `rmkx`, so no door has to remember anything.

/// Rewrite the cursor-key escapes in `data` from their CSI form (`ESC [ A`) to
/// the SS3 form (`ESC O A`) the guest is listening for while it holds
/// application cursor mode. Call it only in that mode: in normal mode the CSI
/// form is already correct.
///
/// Covers exactly the keys terminfo moves between the two forms: the four
/// arrows plus Home (`khome=\EOH`) and End (`kend=\EOF`). Page Up/Down
/// (`ESC [ 5 ~` / `ESC [ 6 ~`) are identical in both modes and modified arrows
/// (`ESC [ 1 ; 2 A`) stay CSI in both, so both pass through untouched. So does
/// a sequence truncated at a chunk boundary, rather than swallowing the
/// keystroke that follows it.
pub fn to_application_cursor(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0x1b
            && data.get(i + 1) == Some(&b'[')
            && let Some(&final_byte) = data.get(i + 2)
            && matches!(final_byte, b'A' | b'B' | b'C' | b'D' | b'H' | b'F')
        {
            out.extend_from_slice(&[0x1b, b'O', final_byte]);
            i += 3;
            continue;
        }
        out.push(data[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
#[path = "keys_test.rs"]
mod keys_test;
