# Contributing a Theme

`late.sh` supports built-in SSH themes. New themes are added in code and submitted via pull request.

## Before you start

Theme selection is persisted in `users.settings.theme_id`.

That means:

- pick a stable `id`
- do not rename an existing theme `id` casually
- do not remove an existing theme unless you also handle migration/fallback deliberately

The user-facing `label` can change later. The `id` should be treated as durable.

## Files to edit

Add the theme in:

- `late-ssh/src/app/common/theme.rs`

## What to add

To add a new theme:

1. Add a new `ThemeKind` enum variant.
2. Add a new `ThemeOption` entry to `OPTIONS`.
3. Add a new `Palette` constant.
4. Extend `current_palette()` to return the new palette.

Once that is done, the theme will automatically:

- appear in the profile theme switcher
- participate in theme cycling
- use the saved `theme_id` preference

## Minimal example

Use the existing themes in `late-ssh/src/app/common/theme.rs` as the source of truth. The shape should look like this:

```rust
pub enum ThemeKind {
    Late = 0,
    Contrast = 1,
    Purple = 2,
    Forest = 3,
}

pub const OPTIONS: &[ThemeOption] = &[
    ThemeOption {
        kind: ThemeKind::Late,
        id: "late",
        label: "Late",
    },
    ThemeOption {
        kind: ThemeKind::Contrast,
        id: "contrast",
        label: "High Contrast",
    },
    ThemeOption {
        kind: ThemeKind::Purple,
        id: "purple",
        label: "Purple Haze",
    },
    ThemeOption {
        kind: ThemeKind::Forest,
        id: "forest",
        label: "Forest Night",
    },
];

const PALETTE_FOREST: Palette = Palette {
    // fill every required semantic color
    // use the existing palettes as the template
};

fn current_palette() -> &'static Palette {
    CURRENT_THEME.with(|current| match current.get() {
        ThemeKind::Contrast => &PALETTE_CONTRAST,
        ThemeKind::Purple => &PALETTE_PURPLE,
        ThemeKind::Forest => &PALETTE_FOREST,
        ThemeKind::Late => &PALETTE_LATE,
    })
}
```

## Palette expectations

Themes are not just decorative. They need to work across the app.

Your palette should keep these states clearly distinguishable:

- normal text
- dim/faint/muted text
- borders vs active borders
- selected backgrounds
- chat author vs chat body
- mentions
- success and error states
- bonsai greens
- badge colors

## Readability requirements

Please test for real terminal usability, not just aesthetics.

At minimum:

- body text should remain readable on common dark terminals
- active borders and selected rows should be obvious
- `MENTION`, `SUCCESS`, and `ERROR` should not blur together
- the theme should still work when the terminal has background opacity/transparency enabled

Avoid themes that rely on very subtle dark-on-dark contrast.

## Local verification

Before opening a PR, run:

```bash
cargo fmt --all
cargo check -p late-ssh
```

If possible, also verify the theme manually in:

- profile/settings
- dashboard/sidebar
- chat
- games

## Opening the PR

Suggested workflow:

1. Create a branch for the theme.
2. Add the theme in `late-ssh/src/app/common/theme.rs`.
3. Run local verification.
4. Commit the change.
5. Open a pull request.

Please include in the PR:

- the theme name
- the stable theme `id`
- a short note about the visual direction
- screenshots if helpful
- any contrast or accessibility considerations

Keep theme PRs focused. Prefer a PR that only adds the theme, or the theme plus tiny related copy tweaks.
