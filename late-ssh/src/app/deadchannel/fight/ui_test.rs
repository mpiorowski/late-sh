use chrono::NaiveDate;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use uuid::Uuid;

use super::{SceneView, corrupt, draw_scene};
use crate::app::deadchannel::fight::session::Scene;
use crate::app::deadchannel::fight::state::{Fight, Sheet};

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

#[test]
fn the_scene_shows_both_faces_the_exchange_and_the_keys() {
    let mut sheet = Sheet::fresh(Uuid::nil(), NaiveDate::from_ymd_opt(2026, 9, 24).unwrap());
    sheet.signal = 4;
    sheet.fight = Some(Fight {
        kind: 6,
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
    };
    let backend = TestBackend::new(90, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            draw_scene(
                frame,
                frame.area(),
                SceneView {
                    sheet: Some(&sheet),
                    scene: &scene,
                    look: None,
                    own_username: "mira",
                },
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut screen = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            screen.push_str(buffer[(x, y)].symbol());
        }
        screen.push('\n');
    }
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
}
