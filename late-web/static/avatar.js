// Rasterize after loading symbol fonts, so PNG export matches the visible art.
(async () => {
    const canvas = document.getElementById('profile-avatar');
    const source = document.getElementById('avatar-data');
    if (!canvas || !source) return;
    const { FONT_FAMILY, FONT_SIZE, renderAvatar } = await import('./avatar-renderer.mjs');
    const data = JSON.parse(source.textContent);
    const scale = Math.max(2, window.devicePixelRatio || 1);
    const draw = () => renderAvatar(canvas, data, { pixelRatio: scale });
    draw();
    if (document.fonts) {
        const text = data.cells.map(cell => cell[2]).join('');
        await Promise.allSettled([
            document.fonts.load(FONT_SIZE * scale + 'px "Symbols Nerd Font"', text),
            document.fonts.load(FONT_SIZE * scale + 'px "Noto Sans Symbols 2"', text),
            document.fonts.load(FONT_SIZE * scale + 'px ' + FONT_FAMILY, text),
        ]);
        draw();
    }
    const download = document.getElementById('avatar-png');
    download.href = canvas.toDataURL('image/png');
    download.hidden = false;
})();
