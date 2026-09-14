use ratatui::{Terminal, backend::TestBackend};

use super::ui::{ADDRESS, DIFFICULTY, VERSION, WORLD_BORDER, draw_landing};

/// Render the hub landing card the way the Games page does and return its
/// text, so the assertions read what a player reads.
fn landing_text() -> String {
    let backend = TestBackend::new(110, 70);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            draw_landing(frame, frame.area(), 0);
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

fn infra_file(name: &str) -> String {
    let path = format!("{}/../infra/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"))
}

/// The quoted value of the container env named `name`: the `value = "..."`
/// line that follows its `name = "..."` line.
fn env_value(tf: &str, name: &str) -> String {
    let needle = format!("\"{name}\"");
    let mut lines = tf.lines();
    lines
        .by_ref()
        .find(|line| line.contains("name") && line.contains(&needle))
        .unwrap_or_else(|| panic!("env {name} not found in infra/minecraft.tf"));
    let value_line = lines
        .next()
        .unwrap_or_else(|| panic!("env {name} has no value line"));
    quoted(value_line)
}

fn quoted(line: &str) -> String {
    let start = line.find('"').expect("opening quote") + 1;
    let end = line[start..].find('"').expect("closing quote") + start;
    line[start..end].to_string()
}

/// A player needs three things from this card: where to connect, which client
/// version, and who to ask for the whitelist.
#[test]
fn landing_tells_a_player_how_to_join() {
    let text = landing_text();
    assert!(text.contains("address   mc.late.sh"), "{text}");
    assert!(text.contains("Minecraft: Java Edition 26.2"), "{text}");
    assert!(
        text.contains("/dm       a moderator with your exact Java Edition username"),
        "{text}"
    );
    assert!(text.contains("Whitelist only."), "{text}");
}

/// Nothing is protected until it is claimed, so the card has to say how to
/// claim, how to let a friend in, and what griefing is still possible.
#[test]
fn landing_explains_claims_and_griefing() {
    let text = landing_text();
    assert!(
        text.contains("golden shovel   right-click two opposite corners to claim"),
        "{text}"
    );
    assert!(
        text.contains("/trust name         full access: build and break"),
        "{text}"
    );
    assert!(
        text.contains("the Nether and End cannot be claimed at all"),
        "{text}"
    );
    assert!(
        text.contains("mob griefing is on so villager and piglin farms work"),
        "{text}"
    );
}

/// Every setting the card quotes comes from the Terraform that runs the
/// server. A change there has to update `ui.rs` too, or players read a world
/// that no longer exists.
#[test]
fn quoted_settings_match_the_terraform() {
    let minecraft = infra_file("minecraft.tf");
    let defaults = infra_file("defaults.tf");

    let version_line = defaults
        .lines()
        .find(|line| line.trim_start().starts_with("minecraft_version"))
        .expect("minecraft_version in infra/defaults.tf");
    assert_eq!(quoted(version_line), VERSION);

    let port_line = defaults
        .lines()
        .find(|line| line.trim_start().starts_with("minecraft_port"))
        .expect("minecraft_port in infra/defaults.tf");
    assert!(
        port_line.ends_with("25565"),
        "the card gives a bare address, which only works on the default port: {port_line}"
    );

    assert!(
        minecraft.contains(ADDRESS),
        "infra/minecraft.tf no longer documents {ADDRESS}"
    );
    assert_eq!(env_value(&minecraft, "DIFFICULTY"), DIFFICULTY);
    assert_eq!(env_value(&minecraft, "ONLINE_MODE"), "TRUE");
    assert_eq!(env_value(&minecraft, "ENABLE_WHITELIST"), "TRUE");
    assert_eq!(
        env_value(&minecraft, "MODRINTH_PROJECTS"),
        "griefprevention"
    );

    let startup = env_value(&minecraft, "RCON_CMDS_STARTUP");
    assert!(
        startup.contains(&format!("worldborder set {WORLD_BORDER}")),
        "startup commands: {startup}"
    );
    assert!(
        !startup.contains("mob_griefing"),
        "the card promises mob griefing at the vanilla default (on); startup commands: {startup}"
    );
}
