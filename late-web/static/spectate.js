(function () {
    const terminalEl = document.getElementById("terminal");
    const overlayEl = document.getElementById("spectate-overlay");
    const term = new Terminal({
        allowProposedApi: true,
        convertEol: false,
        cursorBlink: false,
        disableStdin: true,
        fontFamily: '"Symbols Nerd Font", "SFMono-Regular", "Menlo", "Consolas", monospace',
        fontSize: 14,
        letterSpacing: 0,
        lineHeight: 1.0,
        scrollback: 0,
        theme: {
            background: "#050906",
            foreground: "#d8f3dc",
            cursor: "#22c55e",
            black: "#050906",
            red: "#ef4444",
            green: "#22c55e",
            yellow: "#facc15",
            blue: "#38bdf8",
            magenta: "#c084fc",
            cyan: "#2dd4bf",
            white: "#e5e7eb",
            brightBlack: "#374151",
            brightRed: "#f87171",
            brightGreen: "#86efac",
            brightYellow: "#fde047",
            brightBlue: "#7dd3fc",
            brightMagenta: "#d8b4fe",
            brightCyan: "#5eead4",
            brightWhite: "#ffffff"
        }
    });
    const fitAddon = new FitAddon.FitAddon();
    let socket = null;
    let resizeTimer = null;

    term.loadAddon(fitAddon);
    term.attachCustomKeyEventHandler(function () {
        return false;
    });
    term.open(terminalEl);
    fitAddon.fit();

    function dimensions() {
        return {
            cols: Math.max(1, term.cols || 120),
            rows: Math.max(1, term.rows || 40)
        };
    }

    function wsUrl() {
        const dims = dimensions();
        const url = new URL("/ws/spectate", window.location.href);
        url.protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
        url.searchParams.set("cols", String(dims.cols));
        url.searchParams.set("rows", String(dims.rows));
        return url.toString();
    }

    function sendResize() {
        if (!socket || socket.readyState !== WebSocket.OPEN) {
            return;
        }
        const dims = dimensions();
        socket.send(JSON.stringify({ t: "resize", cols: dims.cols, rows: dims.rows }));
    }

    function showEnded() {
        overlayEl.hidden = false;
    }

    socket = new WebSocket(wsUrl());
    socket.binaryType = "arraybuffer";
    socket.addEventListener("message", function (event) {
        if (typeof event.data === "string") {
            return;
        }
        term.write(new Uint8Array(event.data));
    });
    socket.addEventListener("close", showEnded);
    socket.addEventListener("error", showEnded);

    window.addEventListener("resize", function () {
        window.clearTimeout(resizeTimer);
        resizeTimer = window.setTimeout(function () {
            fitAddon.fit();
            sendResize();
        }, 150);
    });
})();
