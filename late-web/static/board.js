// Draws Artboard cells onto a <canvas>: /gallery snapshots and a
// runner's poster on a profile page. A board is the JSON the server renders
// (`pages::gallery::snapshot_data`): { width, height, cells }, each cell
// [x, y, glyph, width, colour, ...]. Fonts come from board.css.
const lateBoard = {
    cellW: 8,
    cellH: 16,
    fontSize: 14,
    fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Symbols Nerd Font", "Noto Sans Symbols 2", "Noto Sans Symbols", "Apple Color Emoji", "Segoe UI Emoji", monospace',

    draw(canvas, board) {
        canvas.width = board.width * this.cellW;
        canvas.height = board.height * this.cellH;
        canvas.style.width = canvas.width + 'px';
        canvas.style.height = canvas.height + 'px';

        const ctx = canvas.getContext('2d');
        ctx.fillStyle = '#111827';
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        ctx.font = this.fontSize + 'px ' + this.fontFamily;
        ctx.textBaseline = 'top';

        for (const [x, y, ch, , fg] of board.cells) {
            ctx.fillStyle = fg || '#d1d5db';
            ctx.fillText(ch, x * this.cellW, y * this.cellH);
        }
    },

    // Settles once the symbol fonts are in, or failed to load. Passing the
    // board's characters lets unicode-range gate the fetch.
    async fontsReady(board) {
        if (!document.fonts) return;
        const text = Array.from(new Set(board.cells.map((cell) => cell[2]))).join('');
        await Promise.allSettled([
            document.fonts.load(this.fontSize + 'px "Symbols Nerd Font"'),
            document.fonts.load(this.fontSize + 'px "Noto Sans Symbols 2"', text),
        ]);
    },
};
