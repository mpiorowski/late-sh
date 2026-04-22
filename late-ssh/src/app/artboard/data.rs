use super::state::HelpTab;

pub fn lines_for(tab: HelpTab) -> Vec<String> {
    match tab {
        HelpTab::Overview => overview_lines(),
        HelpTab::Drawing => drawing_lines(),
        HelpTab::Brushes => brushes_lines(),
        HelpTab::Session => session_lines(),
    }
}

fn overview_lines() -> Vec<String> {
    [
        "Artboard",
        "",
        "A shared, persistent ASCII canvas everyone paints into. One board, 384 x 192 cells, saved to the server.",
        "",
        "Two modes",
        "  view mode         pan and read, no edits",
        "  active mode       type to draw, select, stamp",
        "",
        "  i / Enter         enter active mode",
        "  Esc               return to view mode",
        "  Ctrl+\\           toggle owner overlay",
        "",
        "What is local vs shared",
        "  shared            canvas cells, peer list, your color",
        "  local             cursor, viewport, selection, swatches, brush",
        "",
        "Two users can hold different selections and swatches while painting the same canvas. Only cell changes sync.",
        "",
        "Persistence",
        "  the board autosaves every 5 minutes and on shutdown",
        "  on boot it resumes from the last snapshot",
        "",
        "This modal",
        "  Tab / Shift+Tab   next / previous tab",
        "  j / k / ↑ / ↓     scroll current tab",
        "  Ctrl+P / Esc / q  close",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn drawing_lines() -> Vec<String> {
    [
        "Drawing",
        "",
        "Active mode is keyboard-first. Type to draw, Space to erase, arrows to move.",
        "",
        "Move",
        "  ← ↑ ↓ →            move cursor",
        "  Home / End         jump to ← / → edge",
        "  PgUp / PgDn        jump to ↑ / ↓ edge",
        "  Enter              move down one row",
        "  Alt+arrows         pan viewport",
        "  Ctrl+Shift+arrows  pan (no floating brush)",
        "",
        "Draw / erase",
        "  <type>             draw a character at the cursor",
        "  Space              erase at the cursor",
        "  Backspace          erase left and move back",
        "  Delete             erase at the cursor",
        "",
        "Selection",
        "  Shift+arrows       start or extend selection",
        "  Shift+Home / End   extend to ← / → edge",
        "  Shift+PgUp / PgDn  extend to ↑ / ↓ edge",
        "  <type>             fill selection with that character",
        "  Backspace / Del    clear selection to spaces",
        "  Esc                clear selection",
        "  Ctrl+T             flip selection corner",
        "",
        "Shape ops (on the current selection)",
        "  Ctrl+H / Ctrl+⌫    push column left",
        "  Ctrl+J             push row down",
        "  Ctrl+K             push row up",
        "  Ctrl+L             push column right",
        "  Ctrl+Y             pull from left",
        "  Ctrl+U             pull from below",
        "  Ctrl+I / Tab       pull from above",
        "  Ctrl+O             pull from right",
        "  Ctrl+B             draw selection border",
        "  Ctrl+Space         smart-fill selection",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn brushes_lines() -> Vec<String> {
    [
        "Brushes and swatches",
        "",
        "A swatch is a stored block of cells. Copy a selection into one, then activate it as a floating brush that stamps anywhere.",
        "",
        "Copy into a swatch",
        "  Ctrl+C             copy selection into a swatch slot",
        "  Ctrl+X             cut selection into a swatch slot",
        "  Alt+C / Meta+C     copy to OS clipboard",
        "",
        "Activate a swatch (home row = slots 1..5)",
        "  Ctrl+A             slot 1",
        "  Ctrl+S             slot 2",
        "  Ctrl+D             slot 3",
        "  Ctrl+F             slot 4",
        "  Ctrl+G             slot 5",
        "  click swatch body  activate that slot",
        "",
        "Once active, the swatch follows your cursor as a floating preview.",
        "",
        "Place a floating brush",
        "  Enter              stamp and keep the brush active",
        "  Ctrl+V             stamp and keep the brush active",
        "  Ctrl+Shift+arrows  stroke (repeat stamps while moving)",
        "  Esc                dismiss the floating brush",
        "",
        "Transparency",
        "  activate the same swatch again to toggle whether spaces in the brush erase or pass through the canvas underneath.",
        "",
        "Swatch management (mouse)",
        "  click pin icon     pin / unpin a slot",
        "  Ctrl+click body    clear that slot",
        "",
        "Sample and glyphs",
        "  double-click cell  sample it into a one-glyph brush",
        "  Ctrl+]             open the glyph picker for emoji / Unicode",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn session_lines() -> Vec<String> {
    [
        "Session",
        "",
        "How the artboard fits into the rest of late.sh.",
        "",
        "Modes",
        "  i / Enter          enter active mode",
        "  Esc                return to view mode",
        "  Ctrl+\\            toggle owner overlay",
        "",
        "View-mode controls",
        "  arrows             move the viewport focus",
        "  Alt+arrows         pan",
        "  right-drag         pan with the mouse",
        "  mouse wheel        pan over the canvas",
        "  1-4 / Tab          switch app pages",
        "",
        "Help and quit",
        "  Ctrl+P             toggle this help",
        "  q                  open quit confirm",
        "",
        "Other",
        "  cell changes sync to every connected peer in real time",
        "  snapshots save every 5 minutes and on shutdown",
        "  peer slots are bounded, overflow connections get a rejection notice",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
