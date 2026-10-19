# Board avatars

On `feature/board-avatars`, users paint on the live public Artboard, then choose
**Set avatar** from the rail beside **Hang a piece**. The default frame is
16×8 cells, which looks roughly square with typical terminal fonts. Arrows or
`h/j/k/l` move it. Shift+left/right change its width; Shift+up/down change its
height. `+`/`-` (also `[`/`]`) change both dimensions by one cell. Click to place
the frame, or drag to size it. Width and height are independent: any rectangle
from 1×1 up to 32×32 cells is allowed, including 32×4 and 4×32. There is no
aspect ratio lock or minimum amount of artwork beyond a nonempty crop.

Enter previews a frozen crop; arrows scroll it when the terminal is short.
Enter again saves it. Esc returns from the preview
to the frame, then cancels. `x` opens a removal confirmation. Empty crops are
refused. Only whole wide glyphs inside the frame are included. Glyph positions,
blank space, and RGB colours are preserved. The avatar is an independent copy:
painting over the source and the monthly board wipe do not affect it. Gallery
ownership, glyph-count, daily-cap and applause rules do not apply to avatars.
Artboard edit bans and the live-board requirement still apply.

Settings → Bio also shows the saved avatar and its management controls.
`a` (or Set/Change avatar) opens the Artboard picker, respecting the content
disclaimer, and returns to Bio after saving or cancelling the frame. The bio
draft is preserved. `x` (or Remove avatar) asks for confirmation; Enter removes
the crop and Esc cancels. Tall previews scroll with arrows or the mouse wheel.
While editing bio text, shortcut letters remain ordinary text. Removal refreshes
the preview and profile snapshots, and follows the same Artboard ban check.

Migration 246 creates `user_avatars`, with one crop per account and cascading
deletion. Avatar writes are independent of profile/settings writes. The profile
modal displays the crop above late.fetch, and a user's existing web profile
displays the same crop. Choosing an avatar does not create a web profile.
Previously saved crops retain their original dimensions and artwork. The
existing `size` database column continues to store the crop's width; width and
height also live in the saved canvas. Migration 247 lowers the width minimum
to one cell for databases that already applied migration 246.

The web view paints each glyph at a fixed cell position on a high-resolution
canvas. It loads the gallery's bundled symbol fonts, clips text to its cell
width, preserves the terminal's 1:2 cell proportions, and offers **save PNG**.
Unicode block elements are drawn as tiles that reach the exact cell boundaries,
including partial blocks, quadrants and shades. Box-drawing glyphs use their
rasterized stroke bounds so adjacent lines connect without font line spacing.
Shared boundaries are rounded in image pixels to avoid seams at fractional
display scales. Intentional blank areas inside glyphs remain blank.
Web avatars fit within 160×320 CSS pixels while preserving their terminal cell
proportions; the exported PNG retains the full cell resolution. A no-JavaScript
visitor gets plain text. PNG export freezes the
browser's rendering; it is not a server-rendered canonical font image. Compare
symbol, braille, box-drawing and wide-glyph artwork in different browsers and
terminals before choosing whether to add server-side PNG rendering.

## Local testing

Run the usual `make start` to build the branch and apply its migration. Draw on
page 4, Esc out of edit mode, then Esc back to the rail. Select **Set avatar**,
frame and save. Open
`/profile` to check the terminal crop, then visit the URL of an existing web
profile. Test 1×1, 16×8, 32×4, 4×32 and 32×32 cells, the board's bottom/right
edges, replacement, removal, and editing the source after saving.

Targeted automated coverage (using the repository's isolated check DB):

```sh
make test-llm ARGS="-p late-core -p late-ssh -p late-web --lib -E 'test(avatar) or test(artboard) or test(profile_modal) or test(pages::profiles)'"
```
