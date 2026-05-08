#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalHelpTopic {
    Copy,
    Links,
    Selection,
    Notifications,
}

impl TerminalHelpTopic {
    pub const ALL: [TerminalHelpTopic; 4] = [
        TerminalHelpTopic::Copy,
        TerminalHelpTopic::Links,
        TerminalHelpTopic::Selection,
        TerminalHelpTopic::Notifications,
    ];

    pub fn short_label(self) -> &'static str {
        match self {
            TerminalHelpTopic::Copy => "Copy",
            TerminalHelpTopic::Links => "Links",
            TerminalHelpTopic::Selection => "Selection",
            TerminalHelpTopic::Notifications => "Notifications",
        }
    }

    pub fn index(self) -> usize {
        match self {
            TerminalHelpTopic::Copy => 0,
            TerminalHelpTopic::Links => 1,
            TerminalHelpTopic::Selection => 2,
            TerminalHelpTopic::Notifications => 3,
        }
    }
}

pub fn lines_for(topic: TerminalHelpTopic) -> Vec<String> {
    match topic {
        TerminalHelpTopic::Copy => copy_lines(),
        TerminalHelpTopic::Links => link_lines(),
        TerminalHelpTopic::Selection => selection_lines(),
        TerminalHelpTopic::Notifications => notification_lines(),
    }
}

fn copy_lines() -> Vec<String> {
    [
        "Why copy sometimes silently fails",
        "",
        "When you press c on a chat message, late.sh emits OSC 52 - a standard",
        "escape sequence that asks the host terminal to put text on the system",
        "clipboard. The server has no other way to reach your clipboard over SSH;",
        "we send the bytes and hope the terminal cooperates.",
        "",
        "Terminals that work out of the box",
        "  kitty, Ghostty, foot, wezterm, alacritty, rxvt-unicode, konsole, xterm",
        "  Apple Terminal works in recent macOS versions",
        "",
        "iTerm2",
        "  OSC 52 is disabled by default. Enable it in:",
        "    Settings -> General -> Selection",
        "    -> Applications in terminal may access clipboard",
        "  Without that toggle, copy will look like nothing happened.",
        "",
        "tmux",
        "  tmux strips OSC 52 unless you opt in. Add to ~/.tmux.conf:",
        "    set -g set-clipboard on",
        "    set -ga terminal-overrides ',*:Ms=\\E]52;%p1%s;%p2%s\\007'",
        "  Then reload with: tmux source ~/.tmux.conf",
        "  Some older tmux builds need set-clipboard external instead of on.",
        "",
        "screen",
        "  GNU screen does not forward OSC 52 at all. Use tmux or run without",
        "  a multiplexer if you need clipboard from late.sh.",
        "",
        "Mosh",
        "  mosh strips OSC 52. There is no flag to enable it. Use plain ssh",
        "  if clipboard matters to you.",
        "",
        "How to tell if it worked",
        "  No banner means the bytes left late.sh successfully. Whether anything",
        "  landed on your clipboard is between your terminal and your OS.",
        "  Try pasting somewhere - that is the only real check.",
        "",
        "Workaround",
        "  Most messages are short. You can always select text with the mouse",
        "  the old-fashioned way and copy through your terminal's own menu.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn link_lines() -> Vec<String> {
    [
        "Why links are not always clickable",
        "",
        "There is a fancier escape sequence (OSC 8) that wraps a span of text",
        "as a real hyperlink the terminal can open on click. late.sh does not",
        "emit it.",
        "",
        "Why we skipped OSC 8",
        "  - it overlays normal text, so it fights with selection, mouse",
        "    forwarding, and our own click handlers",
        "  - terminal support is uneven; on some terminals the link layer",
        "    swallows clicks meant for buttons or chat messages",
        "  - getting it right across kitty, Ghostty, iTerm2, wezterm, tmux,",
        "    and the various Linux terminals is a constant maintenance tax",
        "  - the URL is right there in the message - you can read it",
        "",
        "What works on most terminals",
        "  Plain URLs in chat are still detected by the terminal's own URL",
        "  scanner. The exact modifier varies:",
        "",
        "  kitty           Ctrl+Shift+click, or `kitty +open`",
        "  Ghostty         Cmd+click (mac) / Ctrl+click (linux)",
        "  iTerm2          Cmd+click",
        "  wezterm         Ctrl+Shift+click, or right-click -> Open link",
        "  alacritty       Ctrl+click (with mouse.url config)",
        "  GNOME Terminal  Ctrl+click",
        "  Konsole         Ctrl+click",
        "  Windows Terminal  Ctrl+click",
        "  foot            Ctrl+click, or t to enter URL mode",
        "",
        "Built-in copy",
        "  When a chat message has a URL, press c on it. late.sh extracts the",
        "  URL and sends it through the same OSC 52 path described in the Copy",
        "  tab. That works even on terminals that refuse to make the URL",
        "  clickable.",
        "",
        "Per-feature shortcuts",
        "  Showcase, Work, News, Feeds: Enter on a row copies the URL",
        "  News modal: Enter copies and closes",
        "",
        "If clicks just do not register",
        "  Your terminal may need mouse reporting enabled, or tmux may be",
        "  intercepting the click. Use the keyboard shortcut to copy the URL",
        "  and paste it into your browser instead.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn selection_lines() -> Vec<String> {
    [
        "Why selecting text is sometimes blocked",
        "",
        "late.sh enables mouse reporting so it can react to clicks, scroll",
        "wheels, and drags inside the TUI. Once mouse reporting is on, the",
        "terminal forwards mouse events to the app instead of doing its own",
        "selection - that is why click-and-drag often does nothing.",
        "",
        "How to force a real selection",
        "",
        "  kitty           hold Shift while dragging",
        "  Ghostty         hold Shift while dragging",
        "  iTerm2          hold Option (Alt) while dragging",
        "  wezterm         hold Shift while dragging",
        "  alacritty       hold Shift while dragging",
        "  GNOME Terminal  hold Shift while dragging",
        "  Konsole         hold Shift while dragging, or middle-click to paste",
        "  Windows Terminal  hold Shift while dragging",
        "  foot            hold Shift while dragging",
        "  xterm           hold Shift while dragging",
        "",
        "Rule of thumb: Shift+drag is the standard escape hatch. macOS",
        "terminals often use Option/Alt instead.",
        "",
        "tmux extra step",
        "  tmux runs its own selection layer on top. Either:",
        "    - hold Shift to bypass tmux entirely (uses the terminal's selection)",
        "    - or enter copy mode: prefix [ then move/select with vi keys",
        "  If you have set -g mouse on in tmux, Shift+drag is again the way",
        "  to bypass tmux's selection and use the terminal's.",
        "",
        "Why we keep mouse reporting on",
        "  Click-to-select messages, click-to-react, scroll-to-page, and the",
        "  Artboard cursor all need it. Disabling it would break more than it",
        "  fixes.",
        "",
        "Built-in alternatives",
        "  c on a chat message copies its content via OSC 52",
        "  c on rooms/news/feeds copies the relevant URL",
        "  s in Bonsai copies the bonsai art",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn notification_lines() -> Vec<String> {
    [
        "Why notifications sometimes do not show up",
        "",
        "Desktop notifications use OSC 777 (kitty/Ghostty/foot/wezterm/konsole)",
        "and OSC 9 (iTerm2). late.sh sends one or both based on Settings ->",
        "Notify Format. The terminal then decides whether to surface a system",
        "notification.",
        "",
        "What can swallow them",
        "  - tmux strips notification escape sequences by default",
        "  - some terminals show notifications only when unfocused",
        "  - macOS Focus modes block them at the OS level",
        "  - the bell character is muted in most modern terminals",
        "",
        "tmux",
        "  Add to ~/.tmux.conf:",
        "    set -g allow-passthrough on",
        "  Or rely on tmux's own activity/bell monitoring instead.",
        "",
        "If nothing fires",
        "  Try switching Notify Format to the other option in Settings, then",
        "  test with a DM from another session. The Bell toggle is a separate",
        "  audible cue that some terminals also drop.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
