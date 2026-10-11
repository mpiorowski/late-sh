# `late.sh` TUI Design Guide

## Status and provenance

We did not develop a design guide alongside the app, so this document had to be
reconstructed from the existing interface and code. That explains why the first
draft included descriptions of the app as it is, and why some inconsistencies in
the document reflect inconsistencies in the app itself.

The initial draft covered a large surface area with limited review and testing.
Its provenance is therefore uneven, and its caution about prescribing a single
design is deliberate. This is a working reference: as we review the interface in
use, we can distinguish useful conventions from accidental behavior and become
more confident about which guidance deserves to be firmer.

### How to read this guide

| Status | Meaning |
| :--- | :--- |
| **Observed** | A convention or implementation found in the current app. Its scope matters; describing it does not endorse every detail. |
| **Suggested** | A reasonable starting point for new or revised UI, subject to the needs of the screen and further testing. |
| **Open** | A variation, gap, or proposal that needs more investigation before we choose a common treatment. |

Explicit design decisions are identified locally, such as the ellipsis guidance
in §4.3. Most stylistic guidance remains tentative. This document does not
authorize an app-wide harmonization pass merely because two screens differ.

The scope is the interface drawn by late.sh: its shell, screens, launchers,
dialogs, and native games. Embedded applications such as NetHack own their
in-game presentation and input conventions. Repository architecture and
correctness contracts remain documented in [CONTEXT.md](CONTEXT.md) and the
relevant domain context files. Source links below are implementation references;
the illustrative snippets are not a new shared API.

## 1. Design aims — suggested

1. **Density with breathing room.** Keep related information compact while giving controls and sections enough separation to be recognizable.
2. **Predictable alignment.** Use stable gutters where they help scanning. Adapt the layout for long labels and narrow spaces while keeping important values readable.
3. **Readable hierarchy.** Establish emphasis with tested foreground/background pairs, bold, glyphs, and spacing. A quieter treatment should still be readable when it carries necessary information.
4. **Keyboard and mouse access.** Support clear keyboard paths and useful mouse targets, while respecting the chosen interaction mode and the input owned by an editor or game.
5. **Recoverable compact layouts.** Aim to keep core tasks usable on small terminals. When a canvas cannot fit, explain the space it needs and preserve a way to leave or recover.

These are design aims, not a claim that every current screen satisfies them.

## 2. Terminal dimensions and responsive behavior

### 2.1 Useful review sizes — suggested

The first draft used these reference sizes. They are useful places to inspect
behavior, rather than universal breakpoints or verified guarantees.

| Terminal geometry | What to inspect |
| :--- | :--- |
| **44×22** | Compact phone-sized space: focus visibility, reachable fields, clipped controls, and a usable exit path. Some canvases require more room. |
| **96×34** | Compact desktop space: interaction between the shell, rails, popups, and available body area. |
| **180×60** | A roomy desktop reference: spacing, competing emphasis, and multi-pane readability. |
| **500×200** | An optional extreme-size check: excessive text width, awkward centering, and unhelpful expansion. |

**Observed:** the shell's Auto room-list threshold is 96 terminal columns and its
Auto right-sidebar threshold is 72. Explicit On settings bypass those Auto
thresholds. The right sidebar is drawn on Home and Arcade; its contents also
depend on configuration and available space.
See [shell layout](late-ssh/src/app/render.rs) and
[sidebar](late-ssh/src/app/common/sidebar.rs).

Distinguish terminal size from the size of a supplied content or pane `Rect`.
The outer frame consumes two columns and two rows, and inner chrome can consume
more. A component's minimum size usually refers to its supplied area, not the
whole terminal.

### 2.2 Minimum-size notices — observed, with suggested use

[draw_too_small](late-ssh/src/app/common/primitives.rs) names the component,
required dimensions, and available space. Existing thresholds include:

| Component | Minimum checked area |
| :--- | :--- |
| Pool Table | 112×30 |
| Traffic | 70×20, checked inside its own frame |
| Games Hub | 60×6 |
| Arcade Lobby | 50×10 |
| Leaderboards | 48×8 |
| Rubik's Cube | 42×18 |
| Rice | 40×12 |
| Green Dragon | 30×10 |

These describe individual implementations, not standard minimums for new
screens. For a genuine geometric requirement, the existing helper gives a useful
recovery message:

```rust
if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
    draw_too_small(frame, area, "Pool Table", MIN_WIDTH, MIN_HEIGHT);
    return;
}
```

**Suggested:** first consider hiding secondary chrome, scrolling, or paging for
forms and lists. Merely clamping a popup to the viewport does not make its fields
visible or reachable. Compact Settings behavior remains a gap; see §9.1.

## 3. Grid, spacing, and borders

### 3.1 Spacing — observed and suggested

Common reference points are two columns between roomy groups, one column in
dense layouts, and a breathing row between sections. These are useful starting
points, not app-wide invariants. Lists often have no blank row between items;
individual tables and panels use different gutters.

**Observed:** the right sidebar leaves a one-column gap after its divider, and
the chat composer uses a one-column horizontal inset.
See [sidebar](late-ssh/src/app/common/sidebar.rs) and
[chat layout](late-ssh/src/app/chat/ui.rs).

**Suggested:** judge spacing with the actual content and supplied area. Reducing
padding can help a compact screen, but it cannot replace budgeting its controls
or providing a way to reach hidden content.

### 3.2 Column alignment and text measurement

**Observed:** Settings commonly uses a 16-character formatted label column;
longer settings descriptions use 28 or 32, and compact fact displays use narrower
columns. Those choices are local conventions. Settings' character-count
measurement is also a known limitation, not a reference implementation for
arbitrary Unicode.

**Correctness:** display layout needs terminal-cell widths. Use Ratatui's
`Line::width()` or `Span::width()`, and its styled graphemes when clipping.
Byte length, scalar-value counts, and Rust format-string padding do not measure
the rendered width of CJK text, emoji, or combining sequences.

**Suggested:** budget the prefix, label, separator, and value within the row's
available width. A long label can be shortened, moved to another row, or paired
with a different compact layout. Appending an unrestricted label and a space
does not prevent overflow.

References: [leaderboard row budgeting](late-ssh/src/app/leaderboard/ui.rs),
[calendar title clipping](late-ssh/src/app/calendar/ui.rs), and the illustrative
single-line example in §6.5.

### 3.3 Borders and titles — observed and suggested

Padding a title with one leading and trailing space is a common, useful treatment:

```rust
Block::default().title(" Settings ")
```

The existing palette roles generally distinguish active borders
(`BORDER_ACTIVE`), ordinary borders (`BORDER`), quiet dividers (`BORDER_DIM`),
and some destructive or error frames (`ERROR`). These roles are starting points;
the actual color pair and surrounding emphasis still need review.

## 4. Text hierarchy and glyphs

### 4.1 Semantic text roles — observed

Terminals provide a fixed cell grid; bold and other styling vary by emulator and
font. The palette names below express intended roles, not a guaranteed ordering
of brightness or contrast.

| Role | Common token/treatment | Existing uses |
| :--- | :--- | :--- |
| Emphasis | `TEXT_BRIGHT` + bold | Titles, active values, totals |
| Accent | `AMBER` or `AMBER_GLOW` + bold | Titles, cursors, action keys |
| Body | `TEXT`, or domain-specific `CHAT_BODY` | Labels, values, body text |
| Metadata | `TEXT_DIM` | Timestamps, action descriptions, table headers |
| Quiet detail | `TEXT_FAINT` | Dividers, some placeholders, inactive hints |
| Mentions | `MENTION` | Mention highlighting |

**Suggested:** choose emphasis by checking the actual foreground/background
combination. In the nominal Latte palette, `TEXT` has 7.06:1 canvas contrast,
while `TEXT_BRIGHT` has 2.34:1 and `AMBER_GLOW` has 2.31:1. A token called
"bright" is not automatically a more readable heading color. Selection fills
and account brightness adjustments also change the pairing.

### 4.2 Glyph glossary — observed reference

Styling below names palette roles plus modifiers; it is not literal Rust method
syntax on a `Color`.

| Purpose | Glyphs | Common treatment |
| :--- | :--- | :--- |
| Selected row | `›` (U+203A) | Accent + bold; coordinate with selection style |
| Binary toggle | `● on` (U+25CF), `○ off` (U+25CB) | Success for on; quiet text for off |
| Automatic option | `◐ auto` (U+25D0) | Accent or success, depending on context |
| Disclosure | `▸` (U+25B8), `▾` (U+25BE) | Collapsed / expanded |
| Cycle selector | `◂ Value ▸` | Accented arrows |
| Action / separation | `⏎` (U+23CE) or `↵`; `·` (U+00B7) | Action cue; separator |
| Truncation / elision | `…` (U+2026) | Inherit visible text style; see §4.3 |
| Status | `✓` (U+2713), `✗` (U+2717), `•` (U+2022) | Success, error, neutral information |
| Markdown headings | `▍` (U+258D), `▎` (U+258E), `▏` (U+258F) | Accented H1/H2/H3 markers |
| Text caret | `█` (U+2588) | Accent |
| Tree branches | `├─`, `└─` | Quiet hierarchy lines |

These are common examples, not a requirement to replace every existing glyph.
Check font support and whether a symbol's meaning is clear in its context.

### 4.3 Ellipsis usage — agreed direction

Reserve the Unicode ellipsis character (`…`, U+2026) almost always for indicating
truncation or elision. When used for that purpose, show it only while the value is
actually truncated or content is currently elided. Omit it when the value is
displayed in full or elision is no longer in effect, including after resizing or
changing the value.

Avoid using `…` merely to signal that a control opens a picker or dialog. For
example, a cycle selector whose value fits should read `◂ Server ▸`, without an
ellipsis after `Server`. Existing picker hints that do this are migration gaps,
not exceptions established by their presence in the app.

### 4.4 Mnemonics and date emphasis

**Observed and agreed for Calendar:** toolbar labels integrate their mnemonic
letter: the label is bold, the mnemonic is accented, and the remaining letters
use brighter text. This avoids a separate key badge competing with the label.
Toolbar controls use the calendar canvas background.
See [calendar toolbar](late-ssh/src/app/calendar/toolbar.rs).

Calendar uses a subtle today background where the day has a paintable canvas,
with stronger emphasis reserved for the selected event. Terminal-owned
backgrounds are preserved. Today-date emphasis does not use underlining, which
can render poorly in terminals.
See [calendar rendering](late-ssh/src/app/calendar/ui.rs).

**Suggested:** consider these treatments for similar controls elsewhere, and
prefer tint, bold, a marker, or reversal for new emphasis. Their suitability
outside Calendar is still a design question.

## 5. Color, theming, and selection

### 5.1 Palette roles and scope — observed, with suggested use

The [theme module](late-ssh/src/app/common/theme.rs) contains 105 built-in choices
and 27 `Palette` fields:

- Canvas/surface: `bg_canvas`, `bg_selection`, `bg_highlight`
- Borders: `border_dim`, `border`, `border_active`
- Text: `text_faint`, `text_dim`, `text_muted`, `text`, `text_bright`
- Accents: `amber`, `amber_dim`, `amber_glow`
- Chat/social: `chat_body`, `chat_author`, `mention`
- State: `success`, `error`, `bot`
- Domain colors: `bonsai_sprout`, `bonsai_leaf`, `bonsai_canopy`, `bonsai_bloom`, `badge_bronze`, `badge_silver`, `badge_gold`

**Suggested:** use semantic accessors for interface chrome and UI state, so the
treatment follows the session's theme. User artwork, content colors, and
established game identities can legitimately use their own colors; their
legibility against UI backgrounds is a separate concern.

**Rendering contract:** theme state is thread-local. The render path installs
the reader's theme and brightness adjustment before drawing. Background work
should carry data or semantic ink roles rather than resolve and cache
reader-specific theme colors on an unrelated worker thread.
See [render initialization](late-ssh/src/app/render.rs).

### 5.2 Nominal palette contrast — observed snapshot

These calculated RGB contrast ratios compare tokens with `bg_canvas` before
account brightness adjustments. They do not describe every control, selection
pair, or terminal configuration, and are not a claim of accessibility compliance.

| Family | Theme ID | Canvas | `text` | `border_active` | `amber` |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Core brand | `late` | #000000 | 8.08:1 | 4.55:1 | 5.76:1 |
| High Contrast (default) | `contrast` | #0c0e0c | 15.98:1 | 10.73:1 | 12.27:1 |
| Catppuccin | `mocha` | #1e1e2e | 11.34:1 | 8.07:1 | 9.27:1 |
| Kanagawa | `kanagawa` | #1f1f28 | 9.84:1 | 5.44:1 | 8.15:1 |
| Gruvbox | `gruvboxdark` | #282828 | 10.75:1 | 3.81:1 | 5.94:1 |
| Light | `latte` | #eff1f5 | 7.06:1 | 4.79:1 | 2.64:1 |

### 5.3 Selection and terminal defaults — observed contract

`theme::selection_style()` uses `BG_SELECTION` on a paintable canvas. For a
`Color::Reset` canvas, it sets foreground and background to Reset and adds
`REVERSED`, allowing the terminal's own foreground/background pair to provide
the selection. Reset inherits terminal defaults, which may be opaque or
transparent.

Patch order is significant:

```rust
// Ordinary text: apply selection after the text style.
let selected_text = ordinary_style.patch(theme::selection_style());

// Deliberate semantic foreground: set it after applying selection.
let selected_semantic = theme::selection_style().fg(semantic_fg);
```

The second treatment is an intentional exception for colors that carry meaning,
such as suits or pieces. Under reversal, that foreground becomes the cell fill;
it needs its own contrast review. Neither composition order is a universal
answer for all selected spans.

Adding `REVERSED` again does not toggle it off. Combining selection with another
reversed treatment needs an explicit branch or removal of the modifier, rather
than assuming a later style cancels it.
See [selection_style](late-ssh/src/app/common/theme.rs).

### 5.4 Glyph cutouts — observed technique

Existing games use `theme::punch_through(fill)` for inverted glyph cutouts:

```rust
pub fn punch_through(fill: Color) -> Style {
    Style::default().fg(fill).add_modifier(Modifier::REVERSED)
}
```

The intended effect is a filled cell whose character cutout uses the inherited
background. Reversal swaps the effective foreground/background; the inherited
style therefore matters. This is a game-rendering technique, not a general
selection recipe. Reference: [theme helpers](late-ssh/src/app/common/theme.rs).

## 6. Implementation references and illustrative patterns

### 6.1 Modal sizing and framing — observed inventory

These are current outer-popup reference sizes, not app-wide requirements:

| Component | Sizing | Reference |
| :--- | :--- | :--- |
| Settings | Desired 96×34 | [Settings](late-ssh/src/app/settings_modal/ui.rs) |
| Quit confirmation | Desired 60×11 | [Quit](late-ssh/src/app/quit_confirm/ui.rs) |
| Character sheet | Desired 80×28 | [Sheet](late-ssh/src/app/sheet_modal/ui.rs) |
| Radio booth | Desired 120×40 | [Booth](late-ssh/src/app/audio/booth/ui.rs) |
| Help | 80% of available width, 85% of height | [Help](late-ssh/src/app/help_modal/ui.rs) |
| Calendar | Desired width 76, task-dependent height | [Calendar](late-ssh/src/app/calendar/ui.rs) |
| Profile | Width capped at 110 | [Profile](late-ssh/src/app/profile_modal/ui.rs) |
| Paper / News article | Width capped at 160 | [Paper](late-ssh/src/app/paper/ui.rs), [News](late-ssh/src/app/chat/news/ui.rs) |

A common framing pattern clears the covered cells before drawing the popup.
Clearing and painting the intended background are separate operations:
`Clear` resets cells to terminal defaults; it does not paint `BG_CANVAS`.

Illustrative fragment, after computing a bounded `popup` for the task:

```rust
frame.render_widget(Clear, popup);
let block = Block::default()
    .title(" Settings ")
    .title_style(Style::default().fg(theme::TEXT()).add_modifier(Modifier::BOLD))
    .borders(Borders::ALL)
    .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
    .style(Style::default().bg(theme::BG_CANVAS()));
let inner = block.inner(popup);
frame.render_widget(block, popup);
// Lay out the body and footer within the inner rectangle.
```

The canvas fill is appropriate when the popup is meant to use the theme canvas.
Other treatments can be intentional. Body scrolling, focus visibility, footer
space, and minimum usable geometry remain the component's responsibility.
Reference: [Room Info framing](late-ssh/src/app/room_info_modal/ui.rs).

### 6.2 Form rows — observed reference, with suggested budgeting

[Settings](late-ssh/src/app/settings_modal/ui.rs) has a local `ValueSpan` /
`row_line` pattern combining a marker, label, value, and selected-row padding.
Its `● on` / `○ off` toggles are useful examples of state communicated through
both a word and a glyph.

It is not yet a shared control primitive. Its character-count width handling
should not be copied as the Unicode layout contract. A candidate reusable row
would need explicit budgets for each part, compact behavior, and a deliberate
selection-color policy (§5.3). Choosing a 16-cell label column alone does not
provide those properties.

### 6.3 Action hints — observed helper contracts

`hint_line` styles every supplied action. It does not receive a width budget or
remove hints:

```rust
let footer = hint_line(&[
    ("↑↓", "move"),
    ("Space", "pick"),
    ("Esc", "done"),
]);
frame.render_widget(Paragraph::new(footer), footer_area);
```

`row_with_hint(left, right, width)` separates the two groups when they fit,
otherwise returns the left group alone. It drops the whole right group, and does
not shorten an oversized left group.

**Suggested:** let the caller select shorter action sets or prioritize whole
actions when space is tight. Preserve a discoverable exit/help path. Wrapping can
be intentional when the layout reserves space for it; accidental clipping or
wrapping of a nominally one-row footer needs attention.

References: [hint helpers](late-ssh/src/app/common/primitives.rs) and
[compact directory hints](late-ssh/src/app/directory/ui.rs).

### 6.4 Tabs — observed variation

The outer navigation uses bold amber active-tab pills. Settings uses an accented
foreground on a highlight background. These are separate existing treatments;
whether they should converge is open.

The outer treatment, shown as a style-only excerpt:

```rust
let active_style = Style::default()
    .fg(theme::BG_SELECTION())
    .bg(theme::AMBER())
    .add_modifier(Modifier::BOLD);
```

Clickable tabs additionally need measured, clipped hit regions recorded for the
current frame. Styling a span does not create those regions.
References: [outer navigation](late-ssh/src/app/render.rs) and
[Settings tabs](late-ssh/src/app/settings_modal/ui.rs).

### 6.5 Single-line clipping and numeric rows — illustrative

This local helper illustrates cell-width budgeting and an ellipsis only when
content is omitted. It is not an exported app helper. The fragment uses Ratatui's
`Line`, `Span`, and `Style`; a caller remains responsible for styles and layout.

```rust
fn clip_cells(text: &str, width: usize) -> String {
    let line = Line::from(text);
    if line.width() <= width {
        return text.to_owned();
    }
    let ellipsis_width = Span::raw("…").width();
    if width < ellipsis_width {
        return String::new();
    }
    let limit = width - ellipsis_width;
    let mut used = 0;
    let mut out = String::new();
    for grapheme in line.styled_graphemes(Style::default()) {
        let cells = Span::raw(grapheme.symbol).width();
        if used + cells > limit {
            break;
        }
        out.push_str(grapheme.symbol);
        used += cells;
    }
    out.push('…');
    out
}
```

A suggested table treatment keeps names left-aligned and comparable numeric
values right-aligned. Existing chip counts use `primitives::thousands(i64)`.
This example reserves the value first, then budgets the name and gap:

```rust
let value = clip_cells(&format!("{} chips", thousands(entry.chips)), width);
let value_width = Line::from(value.as_str()).width();
let name_budget = width.saturating_sub(value_width + 2);
let name = clip_cells(&entry.username, name_budget);
let gap = width.saturating_sub(Line::from(name.as_str()).width() + value_width);
let row = Line::from(vec![
    Span::styled(name, name_style),
    Span::raw(" ".repeat(gap)),
    Span::styled(value, value_style),
]);
```

Additional rank or metadata spans need their own budgets. The gap can shrink when
the value occupies the row; unconditionally forcing a minimum gap after laying
out all content can overflow. For an existing richer implementation, see
[leaderboard rows](late-ssh/src/app/leaderboard/ui.rs).

### 6.6 Text inputs and caret coordinates — observed contract

Settings' inline caret helper takes a Unicode scalar-value index, matching the
character index supplied by its TextArea. Naming that argument `cursor_col`
can obscure its units:

```rust
fn text_with_caret(text: &str, cursor_char_index: usize) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    chars.insert(cursor_char_index.min(chars.len()), '█');
    chars.into_iter().collect()
}
```

This does not convert a byte offset, grapheme index, or terminal-cell position.
Mouse-to-caret placement needs the appropriate conversion, including the visible
scroll offset. Prefer an existing editor widget where it fits the task.
References: [Settings input](late-ssh/src/app/settings_modal/ui.rs) and
[calendar mouse placement](late-ssh/src/app/calendar/editor.rs).

### 6.7 Banners — observed lifecycle

`Banner::is_active()` defines a five-second lifetime; `draw_banner` only renders.
Create the banner when the event occurs, keep it in state, and arrange a redraw
when it expires. Illustrative lifecycle using caller-owned state:

```rust
// Event handler:
state.banner = Some(Banner::success("Account settings updated"));

// Drawing:
if let Some(banner) = state.banner.as_ref().filter(|b| b.is_active()) {
    draw_banner(frame, toast_area, banner);
}

// Tick/update:
if state.banner.as_ref().is_some_and(|b| !b.is_active()) {
    state.banner = None;
    changed = true; // Request the final frame that clears the banner.
}
```

Creating a fresh banner during each draw restarts its lifetime.
References: [Banner and drawing](late-ssh/src/app/common/primitives.rs),
[active-banner filtering](late-ssh/src/app/render.rs), and
[expiry redraw](late-ssh/src/app/tick.rs).

## 7. Split views — observed inventory and open choices

| Component | Current split behavior | Reference |
| :--- | :--- | :--- |
| Leaderboards | 25-column rail including divider; split retained down to its 48×8 area minimum | [Leaderboards](late-ssh/src/app/leaderboard/ui.rs) |
| Games Hub | 19-column sidebar including rule; 60×6 area minimum | [Games](late-ssh/src/app/door/hub/ui.rs) |
| Artboard | 21-column allocation; visibility follows editing/focus state | [Artboard](late-ssh/src/app/artboard/ui.rs) |
| Artboard gallery | Preview omitted below 56 pane columns; listing requires 24×6 | [Gallery](late-ssh/src/app/artboard/gallery/ui.rs) |
| Jobs shelf | List gets 42% when wide; list/detail drill-down below 100 columns of directory content width | [Jobs](late-ssh/src/app/jobs/ui.rs), [Directory](late-ssh/src/app/directory/ui.rs) |

A common treatment gives one pane ownership of the divider, avoiding a doubled
rule. Some details start below a breathing row. Rail width, padding, and collapse
behavior are component-specific; there is no app-wide "collapse below 72" rule.

**Suggested:** choose a split by the useful widths of its actual contents.
Consider drill-down when simultaneous panes stop being usable. Keep the active
pane and the path back apparent. Whether related rails should share more
geometry or navigation conventions remains open.

## 8. Interaction and rendering contracts

### 8.1 Interaction modes — observed

| Mode | Terminal mouse reporting | Intended emphasis |
| :--- | :--- | :--- |
| Keyboard | Off | Keyboard paths; native terminal text selection/copy |
| Mouse | On | Mouse-first controls and hints |
| Hybrid (default) | On | Keyboard shortcuts and mouse access |

Mode describes intent, not a guarantee that every screen has complete mouse
coverage. New controls should respect the mode and preserve a usable alternative
for essential actions. See [InteractionMode](late-core/src/models/user.rs) and
[input routing](late-ssh/src/app/input.rs).

### 8.2 Input ownership and hit geometry — observed, with suggested checks

The router gives open dialogs and relevant overlays ownership before the
underlying screen. Text editors and active games have local key meanings.
Adding a mnemonic should preserve those paths, including ordinary text entry.

The render path clears several previous-frame hit regions, and Calendar
invalidates its geometry before drawing. A sound clickable-control pattern
records the visible, clipped target for the current layout and clears targets
that are hidden. Resize, scrolling, and switching screens must not leave a
previous control active at its old location.

**Suggested:** when appropriate to the content, review single-click selection,
double-click opening, right-click actions, wheel scrolling, and list selection.
Calendar provides examples of these behaviors; they are opportunities to assess
for other screens, not requirements to add every gesture everywhere.

References: [input router](late-ssh/src/app/input.rs),
[render geometry lifecycle](late-ssh/src/app/render.rs),
[calendar input](late-ssh/src/app/calendar/input.rs), and
[calendar state](late-ssh/src/app/calendar/state.rs).

### 8.3 Animation and terminal output — observed and suggested

The adaptive scheduler uses 66 ms hot ticks, 132 ms half-rate animation,
264 ms quarter-rate animation, and a 500 ms idle floor. Input and push events
can wake the loop; the idle floor is not an input delay.
See [tick scheduler](late-ssh/src/app/tick.rs).

**Suggested:** fit animation into the existing scheduler and use the lowest
cadence that serves the interaction. An animation budget is a ceiling, not a
request for every pane to redraw at that rate. Hidden or idle content should not
force unnecessary work.

Ordinary cell rendering uses Ratatui's diffing. Terminal background commands,
notifications, and image protocols have intentional output outside that cell
model. For popups intended to cover underlying content, the clearance and
background treatment described in §6.1 still apply.

## 9. Gaps, open questions, and possible next steps

### 9.1 Known gaps from the review — observed

These document behavior to revisit; editing this guide does not fix the app.

| Gap | Relation to this guide |
| :--- | :--- |
| At 44×22 in High Contrast, Settings hides several fields, including the selected Username row | Popup clamping alone does not meet the compact-layout aim |
| Settings adds picker ellipses to fully visible values | Does not follow the agreed ellipsis direction |
| Settings rows use character counts and format padding for width | Not a safe model for arbitrary Unicode cell layout |
| Nominal emphasis tokens have weak canvas contrast in some palettes | Token names alone cannot establish a readable hierarchy |

The review combined source inspection with targeted real-SSH checks in High
Contrast: Settings at 120×40 and 44×22, Leaderboards at 44×22, and Games at 96×34.
The palette and Unicode measurement findings are based on source inspection.
This was not an exhaustive screen or theme audit.

### 9.2 Variations to evaluate — open

- **Selected colors:** which semantic colors need to remain visible, and where is the terminal's plain reversed pair the clearer treatment?
- **Active tabs and rails:** do the different treatments help distinguish navigation levels, or merely reflect separate implementations?
- **Spacing and label widths:** which differences serve the content, and which make related tasks harder to scan?
- **Other ellipsis uses:** loading indicators are an existing non-truncation use; decide whether explicit loading text or another cue would be clearer.
- **Glyph consistency:** would replacing an ASCII checkbox or cursor improve clarity and terminal support in that particular control?

A difference can be useful. Record the reason for a chosen treatment as evidence
accumulates; avoid treating visual uniformity as sufficient justification for a
broad rewrite.

### 9.3 Shared primitives and component gallery — proposals

Possible follow-ups include comparing local `centered_rect` contracts before
consolidation, or extracting a form-row primitive after its width, selection,
and compact behavior have been exercised. Fixed-size and percentage-based
dialogs may need different sizing policies.

A small developer component gallery could make those decisions easier to review:
forms, selectors, tabs, hints, banners, Markdown, and selected semantic colors
shown together. This is a proposal, not an implemented screen or prerequisite
for ordinary UI work.

### 9.4 Validation and maintenance — suggested

Start interface review with High Contrast at the sizes relevant to the change.
Include short and long values, Unicode text, selected/unselected states, resize,
and the affected keyboard/mouse paths. Raster captures help judge contrast and
spacing; interaction checks establish whether the controls remain usable.

Expand theme coverage when the change concerns palette behavior, terminal
defaults, or a particular theme. An exhaustive theme sweep is not a routine
prerequisite. Theme contributions have their own guidance in [THEME.md](THEME.md).
A targeted `tmux-tui-test` workflow could automate useful viewport captures.

When updating this document, keep the status and scope of a claim clear, link
useful implementation references, and record the reason for an explicit design
decision. Source inspection establishes what the code does; use and testing help
establish whether that behavior makes a good default.
