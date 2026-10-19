// Terminal cells are tiles. Block elements must reach their exact edges,
// regardless of the font fallback's bearings, advance width or line spacing.
export const CELL_WIDTH = 10;
export const CELL_HEIGHT = 20;
export const FONT_SIZE = CELL_HEIGHT * 14 / 16; // Same text scale as /gallery.
export const FONT_FAMILY = 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Symbols Nerd Font", "Noto Sans Symbols 2", "Noto Sans Symbols", "Apple Color Emoji", "Segoe UI Emoji", monospace';

const QUADRANTS = [4, 8, 1, 13, 9, 7, 11, 2, 6, 14]; // U+2596–259F: TL, TR, BL, BR bits.
const SHADE = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
// U+2500–257F connections: left=1, right=2, up=4, down=8. Dashed lines
// and diagonals keep their native gaps rather than extending stroke ends.
const CONNECTIONS = '33cc00000000aaaa' + '999966665555eeee' +
    'eeeeddddddddbbbb' + 'bbbb77777777ffff' + 'ffffffffffff0000' +
    '3caaa999666555ee' + 'edddbbb777fffa95' + '6000142814283c3c';

function fillPart(ctx, box, x0, y0, x1, y1) {
    const left = box.x + Math.round(box.width * x0);
    const top = box.y + Math.round(box.height * y0);
    const right = box.x + Math.round(box.width * x1);
    const bottom = box.y + Math.round(box.height * y1);
    ctx.fillRect(left, top, right - left, bottom - top);
}

function drawBlock(ctx, box, code) {
    if (code < 0x2580 || code > 0x259f) return false;
    if (code === 0x2580) {
        fillPart(ctx, box, 0, 0, 1, 0.5);
    } else if (code <= 0x2588) {
        fillPart(ctx, box, 0, 1 - (code - 0x2580) / 8, 1, 1);
    } else if (code <= 0x258f) {
        fillPart(ctx, box, 0, 0, (0x2590 - code) / 8, 1);
    } else if (code === 0x2590) {
        fillPart(ctx, box, 0.5, 0, 1, 1);
    } else if (code <= 0x2593) {
        // Anchor the pattern to the image, so it continues across cell edges.
        const density = (code - 0x2590) * 4;
        for (let y = box.y; y < box.y + box.height; y++) {
            for (let x = box.x; x < box.x + box.width; x++) {
                if (SHADE[y % 4][x % 4] < density) ctx.fillRect(x, y, 1, 1);
            }
        }
    } else if (code === 0x2594) {
        fillPart(ctx, box, 0, 0, 1, 1 / 8);
    } else if (code === 0x2595) {
        fillPart(ctx, box, 7 / 8, 0, 1, 1);
    } else {
        const mask = QUADRANTS[code - 0x2596];
        for (let quadrant = 0; quadrant < 4; quadrant++) {
            if (!(mask & (1 << quadrant))) continue;
            const x = (quadrant % 2) / 2;
            const y = Math.floor(quadrant / 2) / 2;
            fillPart(ctx, box, x, y, x + 0.5, y + 0.5);
        }
    }
    return true;
}

function boxGlyphs(font) {
    // Measure rasterized strokes: text metrics alone can differ from the
    // hinted pixels, especially at small sizes or with a fallback symbol font.
    const sample = document.createElement('canvas');
    const size = Number.parseFloat(font);
    sample.width = sample.height = Math.ceil(size * 4);
    const ctx = sample.getContext('2d');
    ctx.font = font;
    ctx.textBaseline = 'alphabetic';
    const origin = Math.ceil(size * 2);
    function paint(glyph, color = '#ffffff') {
        ctx.clearRect(0, 0, sample.width, sample.height);
        ctx.fillStyle = color;
        ctx.fillText(glyph, origin, origin);
    }
    function bounds(glyph) {
        paint(glyph);
        const pixels = ctx.getImageData(0, 0, sample.width, sample.height).data;
        let left = sample.width, top = sample.height, right = 0, bottom = 0;
        for (let y = 0; y < sample.height; y++) {
            for (let x = 0; x < sample.width; x++) {
                if (!pixels[(y * sample.width + x) * 4 + 3]) continue;
                left = Math.min(left, x);
                top = Math.min(top, y);
                right = Math.max(right, x + 1);
                bottom = Math.max(bottom, y + 1);
            }
        }
        return { left, top, right, bottom };
    }
    function frame(horizontal, vertical) {
        const across = [...horizontal].map(bounds), down = [...vertical].map(bounds);
        const left = Math.max(...across.map(box => box.left));
        const right = Math.min(...across.map(box => box.right));
        const top = Math.max(...down.map(box => box.top));
        const bottom = Math.min(...down.map(box => box.bottom));
        return {
            x: left, y: top,
            width: Math.max(1, right - left),
            height: Math.max(1, bottom - top),
        };
    }
    // Fonts may give the double-line set different extents or a different
    // fallback face. Keep its corners and junctions on the same line grid.
    const single = frame('─━', '│┃'), double = frame('═', '║');
    const cache = new Map();
    return (glyph, color) => {
        const key = glyph + color;
        if (!cache.has(key)) {
            const code = glyph.codePointAt(0);
            const crop = code >= 0x2550 && code <= 0x256c ? double : single;
            const edges = Number.parseInt(CONNECTIONS[code - 0x2500], 16);
            const ink = bounds(glyph);
            // Some fallback faces give an individual corner slightly different
            // endpoints. Align only its connecting edges; retain the empty
            // half of corners and one-sided strokes inside the nominal cell.
            const left = edges & 1 ? ink.left : crop.x;
            const right = edges & 2 ? ink.right : crop.x + crop.width;
            const top = edges & 4 ? ink.top : crop.y;
            const bottom = edges & 8 ? ink.bottom : crop.y + crop.height;
            paint(glyph, color);
            const tile = document.createElement('canvas');
            tile.width = Math.max(1, right - left);
            tile.height = Math.max(1, bottom - top);
            tile.getContext('2d').drawImage(sample,
                left, top, tile.width, tile.height,
                0, 0, tile.width, tile.height);
            cache.set(key, tile);
        }
        return cache.get(key);
    };
}

export function renderAvatar(canvas, data, { pixelRatio = 2, fontFamily = FONT_FAMILY } = {}) {
    const scale = Math.max(1, pixelRatio);
    canvas.width = Math.round(data.width * CELL_WIDTH * scale);
    canvas.height = Math.round(data.height * CELL_HEIGHT * scale);
    const displayScale = Math.min(1, 160 / (data.width * CELL_WIDTH),
        320 / (data.height * CELL_HEIGHT));
    canvas.style.width = data.width * CELL_WIDTH * displayScale + 'px';
    canvas.style.height = 'auto';
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.fillStyle = '#111827';
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.font = FONT_SIZE * scale + 'px ' + fontFamily;
    ctx.textBaseline = 'alphabetic';

    const normal = ctx.measureText('M');
    const ascent = normal.fontBoundingBoxAscent ?? FONT_SIZE * scale * 0.8;
    const descent = normal.fontBoundingBoxDescent ?? FONT_SIZE * scale * 0.2;
    const metrics = new Map();
    let boxGlyph;

    for (const [x, y, glyph, columns, color] of data.cells) {
        // Round shared boundaries once, in image pixels. Fractional display
        // scaling must not leave partially covered seams between solid tiles.
        const left = Math.round(x * CELL_WIDTH * scale);
        const top = Math.round(y * CELL_HEIGHT * scale);
        const box = {
            x: left,
            y: top,
            width: Math.round((x + columns) * CELL_WIDTH * scale) - left,
            height: Math.round((y + 1) * CELL_HEIGHT * scale) - top,
        };
        ctx.fillStyle = color || '#e5e7eb';
        const code = glyph.codePointAt(0);
        if (drawBlock(ctx, box, code)) continue;
        if (code >= 0x2500 && code <= 0x257f) {
            boxGlyph ??= boxGlyphs(ctx.font);
            ctx.drawImage(boxGlyph(glyph, ctx.fillStyle), box.x, box.y, box.width, box.height);
            continue;
        }

        ctx.save();
        ctx.beginPath();
        ctx.rect(box.x, box.y, box.width, box.height);
        ctx.clip();
        ctx.translate(box.x, box.y);
        // Keep ordinary text's natural bearings and spaces. Only shrink
        // ink that actually overflows (e.g. a wide fallback icon), rather
        // than shrinking every character to fit its font's advance.
        if (!metrics.has(glyph)) metrics.set(glyph, ctx.measureText(glyph));
        const ink = metrics.get(glyph);
        const inkWidth = ink.actualBoundingBoxLeft + ink.actualBoundingBoxRight;
        const inkHeight = ink.actualBoundingBoxAscent + ink.actualBoundingBoxDescent;
        const sx = inkWidth > box.width ? box.width / inkWidth : 1;
        const sy = inkHeight > box.height ? box.height / inkHeight : 1;
        const baseline = Math.max(ink.actualBoundingBoxAscent,
            Math.min((box.height - ascent - descent) / 2 + ascent,
                box.height - ink.actualBoundingBoxDescent));
        ctx.scale(sx, sy);
        ctx.fillText(glyph, sx < 1 ? ink.actualBoundingBoxLeft : 0,
            sy < 1 ? ink.actualBoundingBoxAscent : baseline);
        ctx.restore();
    }
}
