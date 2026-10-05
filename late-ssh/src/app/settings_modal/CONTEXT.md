# Settings

Seven tabs share `state.rs`, `input.rs`, and `ui.rs`. Keyboard bindings and
Enter/Esc timing remain unchanged; Bio saves on Enter/Esc and its mouse Done
control. Username, system fields, and feed URL editors have Save and Cancel.

Mouse input is dispatched before editor/keyboard routing and is swallowed by
the foreground Settings surface. Only left-button presses activate controls;
keyboard-only interaction mode disables clicks and scrolling. Nested dialogs
replace the parent's geometry, so background clicks do nothing. Close controls
follow each dialog's existing pending-operation restrictions. Account linking
and deletion still require their existing typed username confirmations.

`mouse.rs` names what a click can land on (`Target`, `Field`) and the panes
the wheel scrolls (`Pane`). The bookkeeping is the shared
`app/common/mouse.rs` (`MouseState<T, P>`, also used by the tag picker with its
own target type): it records targets and pane bounds from the last render,
using zero-based Ratatui coordinates (input normalizes SGR coordinates once).
Geometry is invalidated after mutations and asynchronous content changes;
resized frames cannot use old targets. Virtual bodies are painted to a Buffer
and their visible cells and clipped targets are translated into the viewport.
Each tab, picker, and Statusline pane has its own clamped offset. Wheel events
move three visual rows and never move selection, apply a theme, toggle a
setting, or edit text. Keyboard input reveals selection/caret again. Tabs wrap
on narrow terminals.

The Settings and Tweaks bodies are laid out from one list each
(`settings_lines`, `tweak_lines` in `ui.rs`): `Row::ALL` / `TweakRow::ALL` in
order, grouped under headings by an exhaustive `match`. Drawing, hit targets and
scroll focus all walk that list. To add a row: add the variant, put it in `ALL`,
and the compiler asks for its group and its label/value arm; nothing else holds
a row position.

Settings/Tweaks clicks reuse keyboard actions, including per-device rail and
interaction-mode updates. Cooldown, notification format, both sidebar modes,
Terminal images, Land on, Gallery Art on Splash, and Text Brightness share bold
amber arrows. Only the arrows choose direction; other row/value clicks cycle
forward. Every cycle reserves its longest option width. Ratatui measures both
rendered spans and clipped hitboxes, and labels shorten at grapheme boundaries
before complete controls. Right sidebar's label and always-visible `[Panels]`
button open its panel editor, including when Off.

Target language and Interaction mode open the shared picker on click or
Enter/Space; Left/Right keyboard cycling stays available. Opening selects the
current value without applying it. Enter or a result click applies once and
closes; Esc/[x] cancels. Language filtering matches English/native names and
stored codes in the existing 17-language order. Interaction mode offers
Keyboard, Mouse, and Hybrid, explains that Keyboard disables clicks/wheel, and
uses App's setter for terminal reporting and persistence.
Interaction-mode persistence coalesces rapid choices through one writer per
account, so an older asynchronous write cannot overwrite the final choice.
Chooser values show `…` two spaces after the current label. Width padding follows
the hint so their hitboxes stay consistent across choices.

Statusline keeps its existing split layout. Its compact list column fits the
widest `>[ ] Name [↑↓]` entry, with reorder controls aligned after the widest name
and no gap between cursor and checkbox. Each half of `[↑↓]` independently moves
the row in its arrow's direction, including in Sidebar panels. Option titles sit
two cells from their controls; Label and component-specific options have
directional arrows, while Brief/Auto-hide are
simple toggles. Labels open options; checkboxes toggle. Pane offsets and wheel
behavior stay independent. Responsive single-pane switching is deferred.
Theme labels apply, headings fold, stars favorite, and search filters.
Country/timezone results apply and close; the shared profile Langs tag picker
toggles clicked tags and Done returns them to its caller. RSS exposes Add,
Remove, and Refresh; selection follows its UUID across asynchronous replacement.

A click away from an open text editor (username, system field, Bio, feed URL)
submits it exactly as its keyboard submit does (`submit_text_edit`), then the
click acts. There is one save path for both inputs: saves are fire-and-forget
and a failure surfaces as the usual banner. An empty feed URL is dropped
quietly. `[Cancel]` discards. Account confirmation inputs are not auto-saved.
Closing Settings with `[x]` during a theme search keeps the theme the search
previewed, as Esc does. The Invites dialog's code field takes the caret on
click and `[Add code]` submits it like Enter.

Caret hit targets in single-line fields follow the rendered graphemes,
including wide characters and horizontal scrolling. Bio is the exception: a
click starts editing with the caret at the end, and while editing the TextArea
renders itself and owns wrapping, viewport and caret; clicks inside it and the
wheel do nothing.

`app/common/mouse_test.rs` covers geometry; `mouse_flow_test.rs` covers rendered
and application flows with disposable databases/accounts. Run focused tests
through `make test-llm`.
