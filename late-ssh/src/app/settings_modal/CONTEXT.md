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

`mouse.rs` records typed targets and pane bounds from the last render, using
zero-based Ratatui coordinates (input normalizes SGR coordinates once). Geometry
is invalidated after mutations and asynchronous content changes; resized frames
cannot use old targets. Virtual bodies are painted to a Buffer and their visible
cells and clipped targets are translated into the viewport. Each tab, picker,
and Statusline pane has its own clamped offset. Wheel events move three visual
rows and never move selection, apply a theme, toggle a setting, or edit text.
Keyboard input reveals selection/caret again. Tabs wrap on narrow terminals.

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

Mouse navigation away from an ordinary text edit retains its buffer and queues
one typed destination until a private oneshot result arrives. Profile saves and
feed additions use result-bearing internal service helpers. A failed write keeps
editing with the error; success commits the draft and executes the destination
once. Feed acknowledgement follows subscription storage, while fetching continues
in the background. Search and account confirmation inputs are not auto-saved.
The existing keyboard fire-and-forget methods keep their submission timing.

Caret hit targets follow the rendered graphemes, including wide characters and
horizontal scrolling. Bio uses TextArea's own word wrapping and screen positions;
a clone renders the complete text while the original retains its keyboard viewport
and cursor. Wheel scrolling does not call TextArea's cursor-moving scroll method.

`mouse_test.rs` covers geometry; `mouse_flow_test.rs` covers rendered and application
flows with disposable databases/accounts. Run focused tests through `make test-llm`.
