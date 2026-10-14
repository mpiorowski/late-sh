//! Vintage Story's card in the Games hub. The server is not a door: it is its
//! own pod on the arm64 game node (`infra/vintagestory.tf`) and players join
//! from the Vintage Story client, so this landing is information only and
//! Enter does nothing. The settings it quotes mirror the Terraform and the
//! image recipe; `ui_test.rs` reads `infra/vintagestory.tf`,
//! `infra/defaults.tf`, and `docker/vintagestory/Dockerfile` and fails when
//! they drift.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::common::theme;
use crate::app::door::landing;

/// The address players type. The `vs.late.sh` A record points at the node
/// running the pod; the client's default port 42420 is the hostPort.
pub const ADDRESS: &str = "vs.late.sh";
/// `VS_VERSION` in `docker/vintagestory/Dockerfile`, which is also the game
/// version in the `vintagestory_image` tag: the client must match.
pub const VERSION: &str = "1.22.7";

pub fn draw_landing(frame: &mut Frame, area: Rect, scroll: u16) -> u16 {
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)[1];

    let client = format!("Vintage Story {VERSION}");
    let add_server = format!("Multiplayer, add {ADDRESS} as a server, connect");

    let lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            "Vintage Story",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "A friends survival server, run by late.sh. ",
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Whitelist only.", Style::default().fg(theme::AMBER_DIM())),
        ]),
        Line::from(Span::styled(
            "The official server, no mods. Not played in the terminal: join from the game.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(""),
        landing::heading("Connect"),
        landing::stat("address", ADDRESS, 10),
        landing::stat("client", &client, 10),
        landing::stat(
            "account",
            "your Vintage Story account; the server checks it with Anego",
            10,
        ),
        Line::from(""),
        landing::heading("Get Whitelisted"),
        landing::hint(
            "/dm",
            "a moderator with your exact Vintage Story username",
            10,
        ),
        landing::hint("then", &add_server, 10),
        Line::from(""),
        // Claims are what protects a build, so how to make one comes before
        // the rules: a short terminal cuts the bottom of this card first.
        landing::heading("Claim Your Land"),
        landing::hint(
            "/land claim new",
            "start a claim, then mark two opposite corners",
            19,
        ),
        landing::hint(
            "/land claim start",
            "one corner, where you stand; /land claim end for the other",
            19,
        ),
        landing::hint(
            "/land claim add",
            "add the marked box; claims are 3D, so reach down and up as you need",
            19,
        ),
        landing::hint(
            "/land claim grow",
            "push one side out, e.g. /land claim grow up 20",
            19,
        ),
        landing::hint(
            "/land claim save",
            "give it a name; nothing is protected until you save",
            19,
        ),
        landing::hint(
            "allowance",
            "up to 3 claims and 262144 blocks in all (a 64x64x64 box)",
            19,
        ),
        landing::hint(
            "/land list",
            "your claims; /land free N confirm deletes claim N",
            19,
        ),
        Line::from(""),
        landing::heading("Share a Claim"),
        landing::hint(
            "/land claim load N",
            "pick the claim to change first, and /land claim save after",
            33,
        ),
        landing::hint(
            "/land claim grant name all",
            "full access: build and break",
            33,
        ),
        landing::hint(
            "/land claim grant name use",
            "containers, doors, and other blocks you can use",
            33,
        ),
        landing::hint(
            "/land claim grant name traverse",
            "doors and trapdoors only",
            33,
        ),
        Line::from(""),
        landing::heading("World"),
        landing::stat(
            "in a claim",
            "only the owner and those granted can build, break, or use things",
            12,
        ),
        landing::stat("outside", "anything goes", 12),
        landing::stat("pvp", "on", 12),
        landing::stat("fire", "spreads; keep it away from wood", 12),
        landing::stat("time", "the world's clock stops while nobody is online", 12),
    ];

    crate::app::door::landing::render_scrolled(
        frame,
        inner,
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        scroll,
    )
}
