use chrono::NaiveDate;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use uuid::Uuid;

use rand::{SeedableRng, rngs::StdRng};

use super::{PickerView, SceneView, corrupt, draw_picker, draw_scene};
use crate::app::deadchannel::fight::cards::Card;
use crate::app::deadchannel::fight::data::{MAX_LEVEL, RATIONS_PER_DAY, RULES, exp_to_seek};
use crate::app::deadchannel::fight::road::{Mark, Node, Trace};
use crate::app::deadchannel::fight::session::{Picker, Scene};
use crate::app::deadchannel::fight::sim::Threat;
use crate::app::deadchannel::fight::state::{Pick, Quarry, Sheet};

fn september(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, day).unwrap()
}

/// A level-`level` runner in a kit level with it, in a fight with `pick`,
/// the hand dealt by the test.
fn in_a_fight(level: i32, pick: Pick, hand: [Card; 5]) -> Sheet {
    let mut sheet = Sheet::fresh(Uuid::nil(), september(24));
    sheet.level = level;
    sheet.weapon_tier = level;
    sheet.armor_tier = level;
    sheet.signal = sheet.max_signal();
    sheet.engage(&RULES, pick, &mut StdRng::seed_from_u64(5));
    let fight = sheet.fight.as_mut().expect("a fight");
    fight.piles.hand = hand.into_iter().map(Some).collect();
    sheet
}

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
fn the_scene_shows_both_faces_the_glyphs_next_move_the_hand_and_the_keys() {
    // The howler gathers, then comes down.
    let mut sheet = in_a_fight(
        7,
        Pick::Fair,
        [
            Card::Strike,
            Card::Block,
            Card::Surge,
            Card::Wipe,
            Card::Static,
        ],
    );
    sheet.signal = 28;
    let fight = sheet.fight.as_mut().expect("a fight");
    fight.foe_signal = fight.foe_max_signal / 2;
    fight.block = 4;
    fight.energy = 2;
    let powers = sheet.powers(sheet.fight.as_ref().expect("a fight"));
    let scene = Scene {
        lines: vec![
            "a howler. the whole street hears it before it sees it.".to_string(),
            "your cable whip hits the howler for 9.".to_string(),
        ],
        latest: 1,
        over: false,
        waiting: false,
        old_signal: false,
        failed: false,
    };
    let screen = render(&sheet, &scene, 0).join("\n");
    assert!(screen.contains("the end of the row"), "{screen}");
    assert!(screen.contains("mira  lv 7"), "{screen}");
    assert!(screen.contains("cable whip · faraday coat"), "{screen}");
    assert!(screen.contains("block 4"), "{screen}");
    assert!(screen.contains("lv 7  howler"), "{screen}");
    assert!(screen.contains("signal █████░░░░░░░ 28/70"), "{screen}");
    assert!(
        screen.contains(&format!("▸ gathering. {} next turn", powers.hit * 2)),
        "the glyph's move is on show\n{screen}"
    );
    assert!(screen.contains("then heavy"), "{screen}");
    assert!(
        screen.contains("▸ your cable whip hits the howler for 9."),
        "the latest line is marked\n{screen}"
    );
    // The hand: every slot numbered, every card with what it does.
    assert!(
        screen.contains("energy ██ ██ ░░"),
        "two of three left, and the spent one is a different shape\n{screen}"
    );
    assert!(screen.contains("deck 5 · discard 0 · static 1"), "{screen}");
    for card in [
        "╭ 1 strike ─╮",
        "╭ 2 block ──╮",
        "╭ 3 surge ──╮",
        "╭ 4 wipe ───╮",
        "╭ 5 static ─╮",
    ] {
        assert!(screen.contains(card), "{card}\n{screen}");
    }
    assert!(
        screen.contains(&format!("hit {}", powers.strike)),
        "{screen}"
    );
    assert!(
        screen.contains(&format!("hit {}", powers.surge)),
        "{screen}"
    );
    assert!(
        screen.contains(&format!("hold {}", powers.block)),
        "{screen}"
    );
    assert!(screen.contains("till hit"), "{screen}");
    assert!(screen.contains("-static"), "{screen}");
    assert!(screen.contains("step 0/10 · bits 50"), "{screen}");
    for key in ["[1-5] play", "[e] end turn", "[a] auto turn", "[r] run"] {
        assert!(screen.contains(key), "{key}\n{screen}");
    }
    assert_eq!(
        render(&sheet, &scene, 1),
        render(&sheet, &scene, 0),
        "a glyph that is not coming down does not move with the tick"
    );

    // A slot already played keeps its corners and loses its number.
    sheet.fight.as_mut().expect("a fight").piles.play(0);
    let played = render(&sheet, &scene, 0).join("\n");
    assert!(!played.contains("╭ 1 strike"), "{played}");
    assert!(
        played.contains("╭ 2 block ──╮"),
        "the slots hold still\n{played}"
    );
}

/// A terminal with no room for cards still shows the whole hand, as one
/// row of chips.
#[test]
fn a_short_terminal_gets_the_hand_as_chips() {
    let sheet = in_a_fight(
        3,
        Pick::Fair,
        [
            Card::Strike,
            Card::Block,
            Card::Surge,
            Card::Wipe,
            Card::Static,
        ],
    );
    let scene = Scene {
        lines: vec!["a drift.".to_string()],
        latest: 1,
        over: false,
        waiting: false,
        old_signal: false,
        failed: false,
    };
    let backend = TestBackend::new(90, 17);
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
                    tick: 0,
                },
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let screen: String = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect();
    let powers = sheet.powers(sheet.fight.as_ref().expect("a fight"));
    assert!(
        screen.contains(&format!("[1 strike {}]", powers.strike)),
        "{screen}"
    );
    assert!(screen.contains("[5 static]"), "{screen}");
    assert!(!screen.contains("╭ 1"), "no room for cards\n{screen}");
    assert!(screen.contains("[e] end turn"), "{screen}");
}

fn at_the_bottom() -> (Sheet, Scene) {
    let mut sheet = Sheet::fresh(Uuid::nil(), september(24));
    sheet.level = MAX_LEVEL;
    sheet.exp = exp_to_seek(0);
    sheet.signal = 150;
    sheet.engage(&RULES, Pick::Fair, &mut StdRng::seed_from_u64(5));
    assert_eq!(
        sheet.fight.as_ref().map(|fight| fight.quarry),
        Some(Quarry::OldSignal)
    );
    let scene = Scene {
        lines: vec!["you hit the Old Signal for 21.".to_string()],
        latest: 1,
        over: false,
        waiting: false,
        old_signal: true,
        failed: false,
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
    assert!(screen.contains("[a] auto turn"), "{screen}");
    assert!(
        screen.contains("╭ 1 "),
        "the hand is on the boss's screen too\n{screen}"
    );
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
    // every static cell in the rows over the exchange is the screen's
    // noise.
    let noise = rows[6..10]
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
    assert!(screen.contains("[Enter] back to the road"), "{screen}");
    assert_eq!(torn_cells(&rows), 0, "the frame heals\n{screen}");
    assert_eq!(
        render(&sheet, &scene, 8),
        rows,
        "nothing moves once it is over\n{screen}"
    );
}

/// The road drawn on a 90 by `height` terminal, as one block of text.
fn render_road(sheet: &Sheet, picker: &Picker, height: u16, word: Option<&str>) -> String {
    let backend = TestBackend::new(90, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            draw_picker(
                frame,
                frame.area(),
                PickerView {
                    sheet: Some(sheet),
                    picker,
                    look: None,
                    own_username: "mira",
                    tick: 0,
                    word,
                },
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect()
}

/// The lane of the first `node` on `step` (1 is the first) of the road.
fn lane_of(sheet: &Sheet, step: usize, node: Node) -> Option<u8> {
    sheet.todays_road().steps[step - 1]
        .iter()
        .position(|here| *here == node)
        .map(|lane| lane as u8)
}

#[test]
fn the_road_shows_the_map_and_the_glyph_under_the_cursor_with_its_threat() {
    let mut sheet = Sheet::fresh(Uuid::nil(), september(24));
    sheet.level = 2;
    sheet.signal = 20;
    sheet.bits = 0;
    sheet.road.static_cards = 2;
    let picker = Picker {
        lane: 1,
        fair: Some(Threat::Grim),
        lower: Some(Threat::Easy),
        bright: Some(Threat::Risky),
    };
    let (_, _, hiss) = RULES.foe(2);

    let text = render_road(&sheet, &picker, 30, None);

    assert!(text.contains(" the road "), "{text}");
    assert!(text.contains("thu 24 sep · road #"), "{text}");
    assert!(text.contains("mira"), "{text}");
    assert!(text.contains("· static 2"), "{text}");
    // The map: the ten steps numbered, the first step's three glyphs,
    // the cursor's in brackets with the way in marked.
    assert!(
        text.contains("1      2      3      4      5      6      7      8      9     10"),
        "{text}"
    );
    assert!(
        text.contains("▸ [▚]·····+"),
        "the way in, and the rail\n{text}"
    );
    assert!(
        text.contains("── ▚ glyph  ▓ bright  + rest  $ cache  ─"),
        "{text}"
    );
    // Under it, the glyph on that lane.
    assert!(text.contains("▸ [f]"), "{text}");
    assert!(
        text.contains("hiss  lv 2   the glyph of your level"),
        "{text}"
    );
    assert!(text.contains("grim"), "{text}");
    assert!(
        text.contains(&format!("signal {} · hits for", hiss.signal)),
        "{text}"
    );
    assert!(text.contains("it hits > hits > noise"), "its moves\n{text}");
    assert!(
        text.contains(&format!("pays {} bits · {} exp", hiss.bits, hiss.exp)),
        "{text}"
    );
    assert!(text.contains("[Enter] fight"), "{text}");
    assert!(text.contains("[g] flicker, half pay easy"), "{text}");
    assert!(text.contains("esc back"), "{text}");

    // The same panel fits a terminal with no rows to spare: the lanes
    // close up, nothing is dropped.
    let short = render_road(&sheet, &picker, 21, None);
    for needle in [" the road ", "[▚]", "▸ [f]", "[Enter] fight", "esc back"] {
        assert!(short.contains(needle), "{needle}\n{short}");
    }

    // At level 1 there is nothing below the flicker to offer.
    sheet.level = 1;
    sheet.signal = 10;
    let first = render_road(&sheet, &picker, 30, None);
    assert!(first.contains("flicker  lv 1"), "{first}");
    assert!(!first.contains("[g]"), "{first}");
}

#[test]
fn the_road_shows_a_bright_glyph_a_rest_and_a_cache_under_the_cursor() {
    let mut sheet = Sheet::fresh(Uuid::nil(), september(24));
    sheet.level = 4;
    sheet.signal = 12;
    sheet.road.static_cards = 3;
    let road = sheet.todays_road();
    let before = |sheet: &Sheet, node: Node| {
        let step = (1..=RATIONS_PER_DAY as usize)
            .find(|step| lane_of(sheet, *step, node).is_some())
            .expect("the road has one");
        let mut at = sheet.clone();
        at.rations_left = RATIONS_PER_DAY - (step as i32 - 1);
        let lane = lane_of(sheet, step, node).expect("found above");
        // The steps before it, walked down the same lane.
        at.road.path = (1..step)
            .map(|_| Trace {
                lane,
                mark: Mark::Won,
            })
            .collect();
        (
            at,
            Picker {
                lane,
                fair: Some(Threat::Even),
                lower: Some(Threat::Easy),
                bright: Some(Threat::Risky),
            },
        )
    };
    assert!(
        road.steps
            .iter()
            .flatten()
            .filter(|node| **node == Node::Bright)
            .count()
            == 2
    );

    let (at, picker) = before(&sheet, Node::Bright);
    let bright = render_road(&at, &picker, 30, None);
    assert!(bright.contains("▸ [b]"), "{bright}");
    assert!(bright.contains("bright stray signal  lv 4"), "{bright}");
    assert!(bright.contains("risky"), "{bright}");
    assert!(bright.contains("· a crystal"), "{bright}");
    assert!(bright.contains("[▓]"), "{bright}");

    let (at, picker) = before(&sheet, Node::Rest);
    let rest = render_road(&at, &picker, 30, Some("+3 signal. 12/40."));
    assert!(rest.contains("a doorway out of the rain"), "{rest}");
    assert!(rest.contains("[h] mend    +14 signal, to 26/40"), "{rest}");
    assert!(
        rest.contains("[c] clear   3 static cards out of your deck"),
        "{rest}"
    );
    assert!(rest.contains("[+]"), "{rest}");
    assert!(
        rest.contains("+3 signal. 12/40."),
        "the last step's word\n{rest}"
    );

    let (at, picker) = before(&sheet, Node::Cache);
    let cache = render_road(&at, &picker, 30, None);
    assert!(cache.contains("a cache"), "{cache}");
    assert!(
        cache.contains(&format!("[t] take    {} bits", RULES.cache(4))),
        "{cache}"
    );
    assert!(cache.contains("[$]"), "{cache}");
}

#[test]
fn a_road_that_is_over_is_the_days_card() {
    let mut sheet = Sheet::fresh(Uuid::nil(), september(24));
    sheet.level = 3;
    sheet.signal = 0;
    sheet.rations_left = 7;
    sheet.kills_today = 2;
    sheet.road.path = vec![
        Trace {
            lane: 1,
            mark: Mark::Won,
        },
        Trace {
            lane: 1,
            mark: Mark::Cached,
        },
        Trace {
            lane: 2,
            mark: Mark::Fell,
        },
    ];
    let picker = Picker {
        lane: 1,
        fair: None,
        lower: None,
        bright: None,
    };

    let down = render_road(&sheet, &picker, 30, None);

    assert!(down.contains("your signal is down"), "{down}");
    assert!(
        down.contains("2 glyphs down · signal 0/30 · step 3 of 10"),
        "{down}"
    );
    assert!(down.contains("[s] copy the day's card"), "{down}");
    assert!(down.contains("[Enter] back to the street"), "{down}");
    assert!(!down.contains("[f]"), "{down}");
    assert!(
        !down.contains("[▚]"),
        "no cursor on a road that is over\n{down}"
    );
    // The walk is on the map: along the middle lane, then bent down to
    // where the signal dropped.
    assert!(down.contains("▚──────$───╮"), "{down}");
    assert!(down.contains("╰──░"), "{down}");

    // Walked to the end, it says so instead.
    sheet.signal = 9;
    sheet.rations_left = 0;
    let walked = render_road(&sheet, &picker, 30, None);
    assert!(walked.contains("the road is walked"), "{walked}");
}

/// The drafted cards on the table: each prints the number it would land
/// for played now, a free card says so where the cost pips go, and a burn
/// grows the energy row past three.
#[test]
fn the_drafted_cards_print_what_they_would_do_now() {
    let mut sheet = in_a_fight(
        7,
        Pick::Fair,
        [
            Card::Jab,
            Card::Riposte,
            Card::Ground,
            Card::Static,
            Card::Mute,
        ],
    );
    let fight = sheet.fight.as_mut().expect("a fight");
    fight.block = 5;
    fight.energy = 5;
    let powers = sheet.powers(sheet.fight.as_ref().expect("a fight"));
    let scene = Scene {
        lines: vec!["a howler.".to_string()],
        latest: 1,
        over: false,
        waiting: false,
        old_signal: false,
        failed: false,
    };
    let screen = render(&sheet, &scene, 0).join("\n");
    for card in [
        "╭ 1 jab ────╮",
        "╭ 2 riposte ╮",
        "╭ 3 ground ─╮",
        "╭ 5 mute ───╮",
    ] {
        assert!(screen.contains(card), "{card}\n{screen}");
    }
    assert!(
        screen.contains("energy ██ ██ ██ ██ ██"),
        "five energy is five pips\n{screen}"
    );
    assert!(screen.contains("╰ free ─────╯"), "{screen}");
    for text in [
        format!("hit {}", (powers.strike + 1) / 2),
        // A block's worth and the five standing.
        format!("hit {}", powers.block + 5),
        // A strike, and one more for the static card beside it.
        format!("hit {}", powers.strike * 2),
        "+block up".to_string(),
        "x static".to_string(),
        "= nothing".to_string(),
    ] {
        assert!(screen.contains(&text), "{text}\n{screen}");
    }

    // A mute played: the glyph's move reads as nothing.
    sheet.fight.as_mut().expect("a fight").muted = true;
    // The howler opens gathering, and a charge is no move to mute.
    assert!(
        render(&sheet, &scene, 0).join("\n").contains("▸ gathering."),
        "{screen}"
    );
    sheet.fight.as_mut().expect("a fight").turn = 1;
    let muted = render(&sheet, &scene, 0).join("\n");
    assert!(muted.contains("▸ muted. nothing lands"), "{muted}");
}

/// A draft owed takes the road panel: the two cards with a key each and
/// what they do, in place of the node under the cursor, and the cards
/// already taken along the bottom of the frame.
#[test]
fn a_draft_owed_takes_the_road_panel() {
    let mut sheet = Sheet::fresh(Uuid::nil(), september(24));
    sheet.level = 6;
    sheet.signal = 60;
    sheet.cards = vec![Card::Siphon];
    let picker = Picker {
        lane: 1,
        fair: Some(Threat::Easy),
        lower: Some(Threat::Easy),
        bright: Some(Threat::Even),
    };
    for height in [24, 40] {
        let screen = render_road(&sheet, &picker, height, None);
        for text in [
            "a new card",
            "level 6. it takes the place of a block, until the mark",
            "[1] riposte hits for a block's worth plus all the block you have up",
            "[2] bulwark two energy. holds two blocks and a half",
            "[1] [2] take one",
            "drafted: siphon",
        ] {
            assert!(screen.contains(text), "{text} at {height} rows\n{screen}");
        }
        assert!(
            !screen.contains("[Enter] fight"),
            "no step is on offer until the card is taken\n{screen}"
        );
    }

    // The pick made, the node is back.
    sheet.cards.push(Card::Bulwark);
    let screen = render_road(&sheet, &picker, 24, None);
    assert!(screen.contains("[Enter] fight"), "{screen}");
    assert!(screen.contains("drafted: siphon · bulwark"), "{screen}");
}
