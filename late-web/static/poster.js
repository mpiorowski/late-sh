// One canvas on a page (a runner's poster on their profile, a piece on
// its gallery page), drawn by board.js like a /gallery snapshot. The PNG
// link is offered once the symbol fonts are in, so the file matches what
// is on screen.
(async () => {
    const canvas = document.getElementById('profile-poster');
    const board = JSON.parse(document.getElementById('poster-data').textContent);
    lateBoard.draw(canvas, board);
    await lateBoard.fontsReady(board);
    lateBoard.draw(canvas, board);
    const png = document.getElementById('poster-png');
    png.href = canvas.toDataURL('image/png');
    png.hidden = false;
})();
