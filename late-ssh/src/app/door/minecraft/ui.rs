//! Minecraft's card in the Games hub. The server is not a door: it is its own
//! pod (`infra/minecraft.tf`) and players join from the Minecraft Java client,
//! so this landing is information only and Enter does nothing. The settings it
//! quotes mirror the Terraform; `ui_test.rs` reads `infra/minecraft.tf` and
//! `infra/defaults.tf` and fails when they drift.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::common::theme;
use crate::app::door::landing;

/// The address players type. The `mc.late.sh` A record points at the node
/// running the pod; the client's default port 25565 is the hostPort.
pub const ADDRESS: &str = "mc.late.sh";
/// `local.minecraft_version` in `infra/defaults.tf`: the client must match.
pub const VERSION: &str = "26.2";
/// The `DIFFICULTY` env in `infra/minecraft.tf`.
pub const DIFFICULTY: &str = "normal";
/// `worldborder set` in `RCON_CMDS_STARTUP`, in blocks.
pub const WORLD_BORDER: u32 = 6000;

pub fn draw_landing(frame: &mut Frame, area: Rect, scroll: u16) -> u16 {
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)[1];

    let client = format!("Minecraft: Java Edition {VERSION} (Bedrock can't join)");
    let border = format!("{WORLD_BORDER} blocks wide, centred on 0, 0");
    let add_server = format!("Multiplayer > Add Server > {ADDRESS} > Join");

    let lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            "Minecraft",
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
            "Paper with GriefPrevention land claims. Not played in the terminal: join from the game.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(""),
        landing::heading("Connect"),
        landing::stat("address", ADDRESS, 10),
        landing::stat("client", &client, 10),
        landing::stat(
            "account",
            "your Microsoft account; offline logins are refused",
            10,
        ),
        Line::from(""),
        landing::heading("Get Whitelisted"),
        landing::hint(
            "/dm",
            "a moderator with your exact Java Edition username",
            10,
        ),
        landing::hint("then", &add_server, 10),
        Line::from(""),
        // Claims are what protects a build, so how to make one comes before
        // the rules: a short terminal cuts the bottom of this card first.
        landing::heading("Claim Your Land"),
        landing::hint(
            "chest",
            "your first chest claims the 9x9 around it; resize it or it lapses after 7 days away",
            16,
        ),
        landing::hint(
            "golden shovel",
            "right-click two opposite corners to claim, or a corner to drag it bigger",
            16,
        ),
        landing::hint(
            "stick",
            "right-click to see who owns land and where the border runs",
            16,
        ),
        landing::hint(
            "claim blocks",
            "start with 100, earn 100 per hour played, up to 80000; at least 5 wide",
            16,
        ),
        landing::hint(
            "/claimslist",
            "your claims and the claim blocks you have left",
            16,
        ),
        landing::hint(
            "/abandonclaim",
            "drop the claim you stand in, blocks refunded",
            16,
        ),
        Line::from(""),
        landing::heading("Share a Claim"),
        landing::hint("/trust name", "full access: build and break", 20),
        landing::hint(
            "/containertrust name",
            "chests, crops, animals, villager trades",
            20,
        ),
        landing::hint(
            "/accesstrust name",
            "doors, beds, buttons and levers only",
            20,
        ),
        landing::hint("/untrust name", "take it all back", 20),
        Line::from(""),
        landing::heading("Griefing"),
        landing::stat(
            "in a claim",
            "nobody else builds, breaks, loots, uses switches, or hurts animals",
            12,
        ),
        landing::stat(
            "outside",
            "anything goes; the Nether and End cannot be claimed at all",
            12,
        ),
        landing::stat(
            "explosions",
            "creepers and TNT only break blocks outside claims and below sea level",
            12,
        ),
        landing::stat(
            "mobs",
            "mob griefing is on so villager and piglin farms work; endermen can't take blocks",
            12,
        ),
        landing::stat("fire", "never spreads and never burns blocks", 12),
        landing::stat(
            "idle",
            "a claim lapses after 60 days away, unless you hold 10000+ claim blocks",
            12,
        ),
        landing::stat(
            "/trapped",
            "stuck in someone's claim: teleports you out, long cooldown",
            12,
        ),
        Line::from(""),
        landing::heading("World"),
        landing::stat("difficulty", DIFFICULTY, 12),
        landing::stat("border", &border, 12),
        landing::stat("pvp", "on in the open, off inside claims", 12),
        landing::stat(
            "death",
            "you drop everything, anyone can pick it up; items despawn after 5 minutes",
            12,
        ),
    ];

    crate::app::door::landing::render_scrolled(
        frame,
        inner,
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        scroll,
    )
}
