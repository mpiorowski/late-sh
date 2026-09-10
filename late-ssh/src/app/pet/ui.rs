use std::cell::Cell;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use late_core::models::pet::PET_SPECIES_DOG;

use super::state::{PetMood, PetState};
use crate::app::common::theme;

/// Constant height of the pet strip that sits above the chat composer: the
/// pet's box at its smallest. Stable chrome: the strip never grows or
/// shrinks between states.
pub const PET_STRIP_HEIGHT: u16 = 3;

/// How far the pet can travel inside its box, in cells, on each axis. The
/// draw records it so the tick-side animation gate (`frame_changed`) can
/// evaluate the same frame math the next draw will use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PetTravel {
    pub x: usize,
    pub y: usize,
}

/// Where the tank is, seen from the pet's box, when the two share an edge
/// on the Zen page. The pet goes and sits against that edge to watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchSide {
    Left,
    Right,
    Above,
    Below,
}

/// What the pet is doing this frame: sulking on the floor (hungry), out
/// for a stroll (fed), or fed and parked against the tank, watching the
/// fish. Hunger wins over the fish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PetPose {
    Sulk,
    Stroll,
    Watch(WatchSide),
}

/// Wall ticks in a minute: the shared animation clock runs at 66ms.
const TICKS_PER_MINUTE: usize = 60_000 / 66;
/// A fed pet beside a tank gets bored of the glass: it strolls for twenty
/// minutes, then watches for five, and round again on the wall clock.
pub const STROLL_TICKS: usize = 20 * TICKS_PER_MINUTE;
pub const WATCH_TICKS: usize = 5 * TICKS_PER_MINUTE;

impl PetPose {
    pub fn for_frame(mood: PetMood, watching: Option<WatchSide>, tick: usize) -> Self {
        match (mood, watching) {
            (PetMood::Sad, _) => PetPose::Sulk,
            (PetMood::Happy, None) => PetPose::Stroll,
            (PetMood::Happy, Some(side)) => {
                if tick % (STROLL_TICKS + WATCH_TICKS) >= STROLL_TICKS {
                    PetPose::Watch(side)
                } else {
                    PetPose::Stroll
                }
            }
        }
    }
}

/// What the last draw of the box used, recorded for the tick-side gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PetFrameInputs {
    pub travel: PetTravel,
    pub watching: Option<WatchSide>,
}

/// Pet box inputs threaded through the chat and Zen render views. The rect
/// slots receive this frame's clickable targets (the pet and the bowl both
/// feed) so mouse hit-testing in `app::input` can route clicks.
pub struct PetView<'a> {
    pub state: &'a PetState,
    pub pet_rect_slot: Option<&'a Cell<Option<Rect>>>,
    pub bowl_rect_slot: Option<&'a Cell<Option<Rect>>>,
    pub travel_slot: Option<&'a Cell<Option<PetFrameInputs>>>,
}

const BOWL_WIDTH: u16 = 10;
/// bowl + right pad
const BOWL_ZONE_WIDTH: u16 = BOWL_WIDTH + 1;
/// Body width including the tail column, used to keep the pet inside its box.
const PET_WIDTH: usize = 8;
const PET_HEIGHT: usize = PET_STRIP_HEIGHT as usize;

/// The pet's box: a fed pet roams the whole of it, a hungry one sits still on
/// the floor; the bowl is pinned bottom-right and doubles as status (full
/// and green once fed, empty and amber until then) and as the click target.
/// The Home strip is this box at three rows, so it roams sideways only; the
/// Zen tile is the same box at whatever size the tile has. `watching` is
/// the side the tank is on when a tank tile touches this box (Zen only).
pub fn draw_pet_box(
    frame: &mut Frame,
    area: Rect,
    view: &PetView<'_>,
    watching: Option<WatchSide>,
) {
    if area.height < PET_STRIP_HEIGHT || area.width < BOWL_ZONE_WIDTH + PET_WIDTH as u16 + 4 {
        return;
    }
    let state = view.state;

    let roam_zone = Rect {
        width: area.width - BOWL_ZONE_WIDTH,
        ..area
    };
    let bowl_area = Rect {
        x: roam_zone.right(),
        y: area.bottom() - PET_STRIP_HEIGHT,
        width: BOWL_WIDTH,
        height: PET_STRIP_HEIGHT,
    };
    let travel = PetTravel {
        x: (roam_zone.width as usize).saturating_sub(PET_WIDTH),
        y: (roam_zone.height as usize).saturating_sub(PET_HEIGHT),
    };

    let pose = PetPose::for_frame(state.mood(), watching, state.animation_ticks());
    let pet_rect = draw_pet(frame, roam_zone, state, pose, travel);
    if let Some(slot) = view.pet_rect_slot {
        slot.set(Some(pet_rect));
    }
    if let Some(slot) = view.travel_slot {
        slot.set(Some(PetFrameInputs { travel, watching }));
    }

    draw_bowl(frame, bowl_area, state.fed_today());
    if let Some(slot) = view.bowl_rect_slot {
        slot.set(Some(bowl_area));
    }

    // Action feedback ("fed!", "fed! +100 chips", "already fed today") sits
    // right-aligned on the box's floor row, next to the bowl.
    if let Some(feedback) = state.action_feedback.as_deref() {
        let row = Rect {
            y: area.bottom() - 1,
            height: 1,
            ..roam_zone
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{feedback}  "),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            )))
            .right_aligned(),
            row,
        );
    }
}

/// The pet's three art rows inside `zone`, standing where `pet_position`
/// puts it this tick. Returns the pet's on-screen rect (the second feed
/// click target).
fn draw_pet(
    frame: &mut Frame,
    zone: Rect,
    state: &PetState,
    pose: PetPose,
    travel: PetTravel,
) -> Rect {
    let mood = state.mood();
    let color = mood_color(mood);
    let tick = state.animation_ticks();
    let art = PetArt::at(pose, tick, travel);

    let (x, y) = art.position;
    let pad = " ".repeat(x);

    let eyes = art.eyes;
    let tail = art.tail;
    let is_dog = state.species == PET_SPECIES_DOG;
    // Cat: pointy ears `/\_/\` going up. Dog: floppy ears `\,_,/` drooping
    // outward at the sides. Same 5-char crown so the face row aligns.
    let ears = if is_dog { " \\,_,/ " } else { " /\\_/\\ " };
    let mouth_row = if is_dog {
        format!(" \\_{}_/ ", mouth(art.mouth, true))
    } else {
        format!(" > {} < ", mouth(art.mouth, false))
    };

    let lines = vec![
        Line::from(Span::styled(
            format!("{pad}{ears}{}", tail[0]),
            Style::default().fg(color),
        )),
        Line::from(Span::styled(
            format!("{pad}( {eyes} ){}", tail[1]),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("{pad}{mouth_row}"),
            Style::default().fg(color),
        )),
    ];
    let rows = Rect::new(zone.x, zone.y + y as u16, zone.width, PET_HEIGHT as u16);
    frame.render_widget(Paragraph::new(lines), rows);

    Rect {
        x: zone.x + x as u16,
        width: (PET_WIDTH as u16).min(zone.width),
        ..rows
    }
}

/// Three-row bowl: fill + base + slash-command label. The bowl carries the
/// status on its own: full and green once fed, empty and amber until then.
fn draw_bowl(frame: &mut Frame, area: Rect, fed: bool) {
    let (color, inside, label_style) = if fed {
        (
            theme::SUCCESS(),
            "*".repeat(7),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )
    } else {
        (
            theme::AMBER(),
            " ".repeat(7),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::ITALIC),
        )
    };
    let lines = vec![
        Line::from(Span::styled(
            format!("({inside})"),
            Style::default().fg(color),
        ))
        .centered(),
        Line::from(Span::styled(" \\_____/ ", Style::default().fg(color))).centered(),
        Line::from(Span::styled("/pet feed", label_style)).centered(),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

/// What the mouth says this frame. The mood mouths are the pet's own; the
/// watch mouths are the fish's doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mouth {
    Mood(PetMood),
    /// Watching, quietly.
    Hush,
    /// A fish just swam past.
    Gasp,
}

/// Every tick-dependent piece of the pet's art, computed once so the draw
/// and the tick-side gate (`frame_changed`) can never disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PetArt {
    position: (usize, usize),
    eyes: &'static str,
    tail: [&'static str; 2],
    mouth: Mouth,
}

impl PetArt {
    fn at(pose: PetPose, tick: usize, travel: PetTravel) -> Self {
        let activity = pet_activity(pose);
        let blink = activity > 0 && tick % 64 < 3;
        let tail = tail(activity, tick);
        match pose {
            PetPose::Sulk => PetArt {
                position: pet_position(pose, tick, travel),
                eyes: PetMood::Sad.eyes(),
                tail,
                mouth: Mouth::Mood(PetMood::Sad),
            },
            PetPose::Stroll => PetArt {
                position: pet_position(pose, tick, travel),
                eyes: if blink { "-.-" } else { PetMood::Happy.eyes() },
                tail,
                mouth: Mouth::Mood(PetMood::Happy),
            },
            PetPose::Watch(_) => {
                // Wide eyes on the glass; every so often a fish swims past
                // and the pet gasps at it for a few ticks.
                let gasp = tick % 96 < 6;
                PetArt {
                    position: pet_position(pose, tick, travel),
                    eyes: if gasp {
                        "O.O"
                    } else if blink {
                        "-.-"
                    } else {
                        "o.o"
                    },
                    tail,
                    mouth: if gasp { Mouth::Gasp } else { Mouth::Hush },
                }
            }
        }
    }
}

/// Where the pet stands this tick, as (column, row) offsets inside its roam
/// zone. A fed pet strolls the whole box: each axis picks fresh destinations
/// on its own cadence, so the path wanders instead of tracing a diagonal.
/// A hungry pet parks on the floor, mid-box. A watching pet sits still
/// against the edge the tank is behind, on the floor when the tank is
/// beside it.
fn pet_position(pose: PetPose, tick: usize, travel: PetTravel) -> (usize, usize) {
    match pose {
        PetPose::Sulk => (travel.x / 2, travel.y),
        PetPose::Stroll => (
            stroll_axis(tick, travel.x, 60, 0),
            stroll_axis(tick, travel.y, 90, 17),
        ),
        PetPose::Watch(WatchSide::Left) => (0, travel.y),
        PetPose::Watch(WatchSide::Right) => (travel.x, travel.y),
        PetPose::Watch(WatchSide::Above) => (travel.x / 2, 0),
        PetPose::Watch(WatchSide::Below) => (travel.x / 2, travel.y),
    }
}

/// True when the pet art drawn at `tick` differs from the art at `tick - 1`
/// for the given mood, neighbour, and travel: a stroll step, a blink, a
/// tail flick, a gasp edge, or the walk to and from the glass. It compares
/// the same `PetArt` the draw uses, so the render gate only pays frames on
/// ticks where the box actually changes; a parked (sad) pet is fully static.
pub fn frame_changed(
    mood: PetMood,
    watching: Option<WatchSide>,
    tick: usize,
    travel: PetTravel,
) -> bool {
    let prev = tick.wrapping_sub(1);
    let now = PetPose::for_frame(mood, watching, tick);
    let before = PetPose::for_frame(mood, watching, prev);
    PetArt::at(now, tick, travel) != PetArt::at(before, prev, travel)
}

/// Deterministic pseudo-random destination for one stroll leg. Adjacent
/// legs chain (this leg's end is the next leg's start) so motion never jumps.
fn wander_target(seg: usize, travel: usize) -> usize {
    let mut h = (seg as u64)
        .wrapping_add(1)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    (h % (travel as u64 + 1)) as usize
}

/// One axis of the stroll: `leg` ticks per destination, `salt` decorrelates
/// the axes so x and y never pick the same sequence.
fn stroll_axis(tick: usize, travel: usize, leg: usize, salt: usize) -> usize {
    if travel == 0 {
        return 0;
    }
    let seg = tick / leg + salt;
    let into = (tick % leg) as i64;
    let from = wander_target(seg, travel) as i64;
    let to = wander_target(seg + 1, travel) as i64;
    (from + (to - from) * into / leg as i64).clamp(0, travel as i64) as usize
}

/// How busy the pet looks: 0 (still), 1 (rapt, the tail sways slowly), or
/// 3 (bouncy). Drives whether it blinks and how often the tail flicks.
fn pet_activity(pose: PetPose) -> u8 {
    match pose {
        PetPose::Stroll => 3,
        PetPose::Watch(_) => 1,
        PetPose::Sulk => 0,
    }
}

/// Tail glyphs for `[top row, body row]`. A still pet lets the tail droop;
/// otherwise it rests straight and flicks up on a cadence set by activity.
fn tail(activity: u8, tick: usize) -> [&'static str; 2] {
    if activity == 0 {
        return [" ", "\\"]; // drooped, limp
    }
    let period = match activity {
        3 => 14,
        2 => 34,
        _ => 60,
    };
    if tick % period >= period - 4 {
        [")", "/"] // flicked up
    } else {
        [" ", "~"] // resting, straight out
    }
}

fn mouth(mouth: Mouth, is_dog: bool) -> char {
    match (mouth, is_dog) {
        (Mouth::Mood(PetMood::Happy), true) => 'd',
        (Mouth::Mood(PetMood::Happy), false) => 'w',
        (Mouth::Mood(PetMood::Sad), _) => '_',
        (Mouth::Hush, _) => '.',
        (Mouth::Gasp, _) => 'o',
    }
}

fn mood_color(mood: PetMood) -> Color {
    match mood {
        PetMood::Happy => theme::AMBER_GLOW(),
        PetMood::Sad => theme::TEXT_DIM(),
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
