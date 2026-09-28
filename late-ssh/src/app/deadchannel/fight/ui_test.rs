use chrono::NaiveDate;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use uuid::Uuid;

use super::{SceneView, corrupt, draw_scene};
use crate::app::deadchannel::fight::data::OLD_SIGNAL_TIER;
use crate::app::deadchannel::fight::session::Scene;
use crate::app::deadchannel::fight::state::{Fight, Quarry, Sheet};

#[test]
fn corruption_takes_cells_in_proportion_and_holds_still() {
    let rows = [" ╬═╬ ", "▐◈ ◈▌", " ▟▓▙ "];
    let whole = corrupt(rows, 0.0, 9);
    assert!(whole.iter().flatten().all(|(_, lost)| !lost));

    let half = corrupt(rows, 0.5, 9);
    let lost = half.iter().flatten().filter(|(_, lost)| *lost).count();
    assert_eq!(lost, 8, "half of fifteen, rounded");
    assert_eq!(
        corrupt(rows, 0.5, 9),
        half,
        "the same seed loses the same cells"
    );
    assert_ne!(corrupt(rows, 0.5, 10), half, "another seed, another wound");

    let gone = corrupt(rows, 1.0, 9);
    assert!(
        gone.iter()
            .flatten()
            .all(|(ch, lost)| *lost && "░▒▓".contains(*ch))
    );
}

/// The scene drawn on a 90 by 24 terminal, as rows of text.
fn render(sheet: &Sheet, scene: &Scene, tick: u64) -> Vec<String> {
    let backend = TestBackend::new(90, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            draw_scene(
                frame,
                frame.area(),
                SceneView {
                    sheet: Some(sheet),
                    scene,
                    look: None,
                    own_username: "mira",
                    tick,
                },
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect()
        })
        .collect()
}

#[test]
fn the_scene_shows_both_faces_the_exchange_and_the_keys() {
    let mut sheet = Sheet::fresh(Uuid::nil(), NaiveDate::from_ymd_opt(2026, 9, 24).unwrap());
    sheet.signal = 4;
    sheet.fight = Some(Fight {
        quarry: Quarry::Glyph(6),
        foe_signal: 30,
        foe_max_signal: 74,
        foe_attack: 13,
        foe_defense: 10,
        foe_bits: 268,
        foe_exp: 77,
        log: Vec::new(),
    });
    let scene = Scene {
        lines: vec![
            "a howler. the whole street hears it before it sees it.".to_string(),
            "you hit the howler for 9.".to_string(),
        ],
        latest: 1,
        over: false,
        waiting: false,
        old_signal: false,
    };
    let screen = render(&sheet, &scene, 0).join("\n");
    assert!(screen.contains("the end of the row"), "{screen}");
    assert!(screen.contains("mira  lv 1"), "{screen}");
    assert!(screen.contains("bare hands · street clothes"), "{screen}");
    assert!(screen.contains("howler"), "{screen}");
    assert!(screen.contains("signal █████░░░░░░░ 4/10"), "{screen}");
    assert!(screen.contains("30/74 ░░░░░░░█████ signal"), "{screen}");
    assert!(
        screen.contains("▸ you hit the howler for 9."),
        "the latest line is marked\n{screen}"
    );
    assert!(screen.contains("rations 10/10 · bits 50"), "{screen}");
    assert!(screen.contains("you hit the howler for 9."), "{screen}");
    assert!(screen.contains("[a] attack"), "{screen}");
    assert!(screen.contains("[r] run"), "{screen}");
    assert_eq!(
        render(&sheet, &scene, 1),
        render(&sheet, &scene, 0),
        "a glyph's scene does not move with the tick"
    );
}

fn at_the_bottom() -> (Sheet, Scene) {
    let mut sheet = Sheet::fresh(Uuid::nil(), NaiveDate::from_ymd_opt(2026, 9, 24).unwrap());
    sheet.level = 15;
    sheet.signal = 150;
    sheet.fight = Some(Fight {
        quarry: Quarry::OldSignal,
        foe_signal: OLD_SIGNAL_TIER.signal,
        foe_max_signal: OLD_SIGNAL_TIER.signal,
        foe_attack: OLD_SIGNAL_TIER.attack,
        foe_defense: OLD_SIGNAL_TIER.defense,
        foe_bits: 0,
        foe_exp: 0,
        log: Vec::new(),
    });
    let scene = Scene {
        lines: vec!["you hit the Old Signal for 21.".to_string()],
        latest: 1,
        over: false,
        waiting: false,
        old_signal: true,
    };
    (sheet, scene)
}

/// Static cells on the box's border: the top and bottom rows and the two
/// side columns, counted where the frame is drawn.
fn torn_cells(rows: &[String]) -> usize {
    let top = &rows[0];
    let bottom = &rows[rows.len() - 1];
    // The box is one column narrower than the screen and leans: its left
    // edge is wherever the top row starts.
    let left = top.chars().position(|c| c != ' ').expect("a frame");
    let right = left + 88;
    let sides: usize = rows[1..rows.len() - 1]
        .iter()
        .map(|row| {
            let cells: Vec<char> = row.chars().collect();
            let edge = |i: usize| cells.get(i).is_some_and(|c| "░▒▓".contains(*c));
            usize::from(edge(left)) + usize::from(edge(right))
        })
        .sum();
    top.chars().filter(|c| "░▒▓".contains(*c)).count()
        + bottom.chars().filter(|c| "░▒▓".contains(*c)).count()
        + sides
}

#[test]
fn the_old_signal_takes_the_screen_and_tears_the_frame() {
    let (sheet, scene) = at_the_bottom();
    let rows = render(&sheet, &scene, 7);
    let screen = rows.join("\n");
    assert!(screen.contains("the bottom of the city"), "{screen}");
    assert!(screen.contains("Old Signal"), "{screen}");
    assert!(
        screen.contains("▸ you hit the Old Signal for 21."),
        "{screen}"
    );
    assert!(screen.contains("[a] attack"), "{screen}");
    assert!(
        rows[0]
            .trim_start_matches(' ')
            .starts_with(['┌', '░', '▒', '▓']),
        "the box starts at the top of the screen\n{screen}"
    );
    assert!(
        rows[23]
            .trim_start_matches(' ')
            .starts_with(['└', '░', '▒', '▓']),
        "and ends at the bottom\n{screen}"
    );
    assert!(torn_cells(&rows) > 0, "the frame is torn\n{screen}");
    // Both signals are full, so no bar and no portrait holds static:
    // every static cell inside the frame is the screen's noise.
    let noise = rows[1..23]
        .iter()
        .flat_map(|row| row.chars().skip(3).take(84))
        .filter(|c| "░▒▓".contains(*c))
        .count();
    assert!(noise > 0, "the empty rows are static\n{screen}");
    assert_ne!(
        render(&sheet, &scene, 8),
        rows,
        "the tear and the shudder move with the tick\n{screen}"
    );
}

#[test]
fn a_finished_old_signal_scene_holds_still() {
    let (mut sheet, mut scene) = at_the_bottom();
    sheet.fight = None;
    scene.over = true;
    scene.lines.push("the Old Signal comes apart.".to_string());
    let rows = render(&sheet, &scene, 7);
    let screen = rows.join("\n");
    assert!(screen.contains("the bottom of the city"), "{screen}");
    assert!(screen.contains("[Enter] back to the street"), "{screen}");
    assert_eq!(torn_cells(&rows), 0, "the frame heals\n{screen}");
    assert_eq!(
        render(&sheet, &scene, 8),
        rows,
        "nothing moves once it is over\n{screen}"
    );
}
