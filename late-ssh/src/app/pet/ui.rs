use std::cell::Cell;

use late_core::models::pet::{PetMood, PetSpecies};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{Look, PET_HEIGHT, PET_WIDTH, Perch, PetFrameInputs, PetState, PetTravel};
use crate::app::common::theme;

/// The pet's box at its smallest: the three art rows.
pub const PET_BOX_MIN_ROWS: u16 = PET_HEIGHT as u16;

/// Where the tank is, seen from the pet's box, when the two share an edge
/// on the Zen page. The pet goes and sits against that edge to watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchSide {
    Left,
    Right,
    Above,
    Below,
}

/// What the pet goes to look at. The tank moves on its own and the pet
/// reacts to it; the bonsai does not, so it gets a quieter beat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchTarget {
    Tank,
    Bonsai,
}

/// What the pet's box touches on the Zen page, with the side each one is
/// on. The pet spends one watch window of the round on each, so with both
/// beside it the fish and the tree alternate; with one, that one takes
/// both windows, and its time at the glass never depends on what else the
/// page happens to hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Neighbours {
    pub tank: Option<WatchSide>,
    pub bonsai: Option<WatchSide>,
}

impl Neighbours {
    /// What this watch window is spent on: `second` is the second of the
    /// two windows in a round.
    fn target(self, second: bool) -> Option<(WatchTarget, WatchSide)> {
        match (self.tank, self.bonsai, second) {
            (None, None, _) => None,
            (Some(side), None, _) => Some((WatchTarget::Tank, side)),
            (None, Some(side), _) => Some((WatchTarget::Bonsai, side)),
            (Some(side), Some(_), false) => Some((WatchTarget::Tank, side)),
            (Some(_), Some(side), true) => Some((WatchTarget::Bonsai, side)),
        }
    }
}

/// What the pet is doing this frame. The stroll and the watch are wall
/// clock formulas; the perch is state (`PetState::perch`): the pet walking
/// after the cursor, or sitting under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PetPose {
    Stroll,
    Watch(WatchTarget, WatchSide),
    /// Sulking: parked mid floor, turned away.
    Sulk,
    /// Asleep: curled mid floor.
    Sleep,
    At(Perch),
}

/// Wall ticks in a minute: the shared animation clock runs at 66ms.
const TICKS_PER_MINUTE: usize = 60_000 / 66;
/// A calm pet beside a tank or a bonsai gets bored of it: it strolls for
/// twenty minutes, then watches for five, and round again on the wall
/// clock.
pub const STROLL_TICKS: usize = 20 * TICKS_PER_MINUTE;
pub const WATCH_TICKS: usize = 5 * TICKS_PER_MINUTE;
/// One leg of the round: a stroll and the watch that ends it. A round is
/// two legs, so a pet with both neighbours visits each once an hour or so.
pub const LEG_TICKS: usize = STROLL_TICKS + WATCH_TICKS;

impl PetPose {
    pub fn for_frame(
        mood: PetMood,
        neighbours: Neighbours,
        perch: Option<Perch>,
        tick: usize,
    ) -> Self {
        if let Some(perch) = perch {
            return PetPose::At(perch);
        }
        match mood {
            PetMood::Sulking => PetPose::Sulk,
            PetMood::Asleep => PetPose::Sleep,
            // Wound up: it paces rather than watches.
            PetMood::Purring | PetMood::Proud => PetPose::Stroll,
            PetMood::Chatty | PetMood::Vibing | PetMood::Idle => {
                let phase = tick % (2 * LEG_TICKS);
                let window = phase % LEG_TICKS >= STROLL_TICKS;
                match (window, neighbours.target(phase >= LEG_TICKS)) {
                    (true, Some((target, side))) => PetPose::Watch(target, side),
                    (true, None) | (false, _) => PetPose::Stroll,
                }
            }
        }
    }
}

/// Pet box inputs threaded through the Zen render view. The rect slot
/// receives this frame's click target (a click is a pet), the frame slot
/// what the tick needs to walk it and gate its frames.
pub struct PetView<'a> {
    pub state: &'a PetState,
    pub pet_rect_slot: Option<&'a Cell<Option<Rect>>>,
    pub frame_slot: Option<&'a Cell<Option<PetFrameInputs>>>,
}

/// The pet's box: the whole of `area` is its floor and its sky.
/// `neighbours` is the side of each tile it can go and watch, when one
/// touches this box.
pub fn draw_pet_box(frame: &mut Frame, area: Rect, view: &PetView<'_>, neighbours: Neighbours) {
    if area.height < PET_BOX_MIN_ROWS || area.width < PET_WIDTH as u16 + 2 {
        return;
    }
    let state = view.state;
    let travel = PetTravel {
        x: (area.width as usize).saturating_sub(PET_WIDTH),
        y: (area.height as usize).saturating_sub(PET_HEIGHT),
    };
    let pose = PetPose::for_frame(
        state.mood(),
        neighbours,
        state.perch(),
        state.animation_ticks(),
    );
    let art = PetArt::at(state.mood(), pose, state.animation_ticks(), travel);
    let pet_rect = draw_pet(frame, area, state, art);
    if let Some(slot) = view.pet_rect_slot {
        slot.set(Some(pet_rect));
    }
    if let Some(slot) = view.frame_slot {
        slot.set(Some(PetFrameInputs {
            travel,
            zone: area,
            neighbours,
            position: art.position,
        }));
    }
}

/// The pet's three art rows inside `zone`, standing where the art says.
/// Returns the pet's on-screen rect (the click target).
fn draw_pet(frame: &mut Frame, zone: Rect, state: &PetState, art: PetArt) -> Rect {
    let (x, y) = art.position;
    let lines = art_lines(state.species, state.mood(), art, x);
    let rows = Rect::new(zone.x, zone.y + y as u16, zone.width, PET_HEIGHT as u16);
    frame.render_widget(Paragraph::new(lines), rows);

    Rect {
        x: zone.x + x as u16,
        width: (PET_WIDTH as u16).min(zone.width),
        ..rows
    }
}

/// The pet as three rows for a profile: its species in its stored mood,
/// standing still, blinking on the wall clock. No box, no walk.
pub fn portrait_lines(species: PetSpecies, mood: PetMood, tick: usize) -> Vec<Line<'static>> {
    let pose = match mood {
        PetMood::Sulking => PetPose::Sulk,
        PetMood::Asleep => PetPose::Sleep,
        PetMood::Purring | PetMood::Proud | PetMood::Chatty | PetMood::Vibing | PetMood::Idle => {
            PetPose::At(Perch {
                x: 0,
                y: 0,
                look: Look::Ahead,
            })
        }
    };
    let art = PetArt::at(mood, pose, tick, PetTravel { x: 0, y: 0 });
    art_lines(species, mood, art, 0)
}

/// The three rows, `pad` cells in from the left, in the mood's colour.
fn art_lines(species: PetSpecies, mood: PetMood, art: PetArt, pad: usize) -> Vec<Line<'static>> {
    let color = mood_color(mood);
    let pad = " ".repeat(pad);
    let tail = art.tail;
    let (crown, face, floor) = species_rows(species, art.eyes, mouth(art.mouth, species));
    vec![
        Line::from(Span::styled(
            format!("{pad}{crown}{}", tail[0]),
            Style::default().fg(color),
        )),
        Line::from(Span::styled(
            format!("{pad}{face}{}", tail[1]),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("{pad}{floor}"),
            Style::default().fg(color),
        )),
    ]
}

/// The three rows before the tail column, seven cells each so the face
/// aligns across species. Cat: pointy ears going up. Dog: floppy ears
/// drooping outward. Bird: a crest, a beak instead of a mouth, and feet.
fn species_rows(species: PetSpecies, eyes: &str, mouth: char) -> (String, String, String) {
    match species {
        PetSpecies::Cat => (
            " /\\_/\\ ".to_string(),
            format!("( {eyes} )"),
            format!(" > {mouth} < "),
        ),
        PetSpecies::Dog => (
            " \\,_,/ ".to_string(),
            format!("( {eyes} )"),
            format!(" \\_{mouth}_/ "),
        ),
        PetSpecies::Bird => (
            "  ,^,  ".to_string(),
            format!(" ({eyes}{mouth} "),
            "  ^ ^  ".to_string(),
        ),
    }
}

/// What the mouth says this frame. The mood mouths are the pet's own; the
/// watch mouths belong to whatever it is looking at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mouth {
    Mood(PetMood),
    /// Watching, quietly.
    Hush,
    /// A fish just swam past.
    Gasp,
    /// Leaning in to smell the leaves.
    Sniff,
}

/// Every tick-dependent piece of the pet's art, computed once so the draw
/// and the tick-side gate (`frame_changed`) can never disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PetArt {
    pub position: (usize, usize),
    eyes: &'static str,
    tail: [&'static str; 2],
    mouth: Mouth,
}

impl PetArt {
    fn at(mood: PetMood, pose: PetPose, tick: usize, travel: PetTravel) -> Self {
        let activity = pet_activity(mood, pose);
        let blink = activity > 0 && tick % 64 < 3;
        let tail = tail(activity, tick);
        let position = pet_position(pose, tick, travel);
        match pose {
            PetPose::Sulk | PetPose::Sleep | PetPose::Stroll => PetArt {
                position,
                eyes: if blink { "-.-" } else { mood_eyes(mood) },
                tail,
                mouth: Mouth::Mood(mood),
            },
            PetPose::At(perch) => PetArt {
                position,
                eyes: match perch.look {
                    Look::Left => "<.<",
                    Look::Right => ">.>",
                    Look::Ahead => {
                        if blink {
                            "-.-"
                        } else {
                            mood_eyes(mood)
                        }
                    }
                },
                tail,
                mouth: Mouth::Mood(mood),
            },
            // Wide eyes on the glass; every so often a fish swims past
            // and the pet gasps at it for a few ticks.
            PetPose::Watch(WatchTarget::Tank, _) => {
                let gasp = tick % 96 < 6;
                PetArt {
                    position,
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
            // Nothing darts about in a tree: the same rapt eyes on a
            // slower beat, and the pet leans in for a smell of the leaves
            // instead of gasping at them.
            PetPose::Watch(WatchTarget::Bonsai, _) => {
                let sniff = tick % 240 < 12;
                PetArt {
                    position,
                    eyes: if sniff {
                        "^.^"
                    } else if blink {
                        "-.-"
                    } else {
                        "o.o"
                    },
                    tail,
                    mouth: if sniff { Mouth::Sniff } else { Mouth::Hush },
                }
            }
        }
    }
}

/// Where the pet stands this tick, as (column, row) offsets inside its roam
/// zone. A strolling pet wanders the whole box: each axis picks fresh
/// destinations on its own cadence, so the path wanders instead of tracing
/// a diagonal. A sulking or sleeping pet parks on the floor, mid-box. A
/// watching pet sits still against the edge the tank is behind, on the
/// floor when the tank is beside it. A perched pet is where the state says.
fn pet_position(pose: PetPose, tick: usize, travel: PetTravel) -> (usize, usize) {
    match pose {
        PetPose::Sulk | PetPose::Sleep => (travel.x / 2, travel.y),
        PetPose::Stroll => (
            stroll_axis(tick, travel.x, 60, 0),
            stroll_axis(tick, travel.y, 90, 17),
        ),
        PetPose::Watch(_, WatchSide::Left) => (0, travel.y),
        PetPose::Watch(_, WatchSide::Right) => (travel.x, travel.y),
        PetPose::Watch(_, WatchSide::Above) => (travel.x / 2, 0),
        PetPose::Watch(_, WatchSide::Below) => (travel.x / 2, travel.y),
        PetPose::At(perch) => (perch.x.min(travel.x), perch.y.min(travel.y)),
    }
}

/// True when the pet art drawn at `tick` differs from the art at `tick - 1`
/// for the given mood, neighbours, perch, and travel: a stroll step, a
/// blink, a tail flick, a gasp or sniff edge, or the walk to and from the
/// glass. It compares the same `PetArt` the draw uses, so the render gate
/// only pays frames on ticks where the box actually changes; a sleeping pet
/// is fully static.
pub fn frame_changed(
    mood: PetMood,
    neighbours: Neighbours,
    perch: Option<Perch>,
    tick: usize,
    travel: PetTravel,
) -> bool {
    let prev = tick.wrapping_sub(1);
    let now = PetPose::for_frame(mood, neighbours, perch, tick);
    let before = PetPose::for_frame(mood, neighbours, perch, prev);
    PetArt::at(mood, now, tick, travel) != PetArt::at(mood, before, prev, travel)
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

/// How busy the pet looks: 0 (still), 1 (rapt, the tail sways slowly), 2
/// (content), or 3 (bouncy). Drives whether it blinks and how often the
/// tail flicks.
fn pet_activity(mood: PetMood, pose: PetPose) -> u8 {
    match pose {
        PetPose::Sleep => 0,
        // Sulking: still, but awake enough to blink.
        PetPose::Sulk => 1,
        PetPose::Watch(..) => 1,
        PetPose::Stroll | PetPose::At(_) => match mood {
            PetMood::Proud => 3,
            PetMood::Purring | PetMood::Chatty | PetMood::Vibing => 2,
            PetMood::Idle => 1,
            // Never strolls or perches in these; listed so a new mood has
            // to choose.
            PetMood::Sulking | PetMood::Asleep => 0,
        },
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

fn mood_eyes(mood: PetMood) -> &'static str {
    match mood {
        PetMood::Purring => "^.^",
        PetMood::Proud => "*.*",
        PetMood::Sulking => "T_T",
        PetMood::Chatty => "o.o",
        PetMood::Vibing => "~.~",
        PetMood::Asleep => "-.-",
        PetMood::Idle => "o.o",
    }
}

/// The mouth glyph. The bird's is its beak, always pointed: birds do not
/// smile.
fn mouth(mouth: Mouth, species: PetSpecies) -> char {
    match (species, mouth) {
        (PetSpecies::Bird, _) => '>',
        (PetSpecies::Dog, Mouth::Mood(PetMood::Purring | PetMood::Proud | PetMood::Vibing)) => 'd',
        (PetSpecies::Cat, Mouth::Mood(PetMood::Purring | PetMood::Proud | PetMood::Vibing)) => 'w',
        (_, Mouth::Mood(PetMood::Chatty)) => 'o',
        (_, Mouth::Mood(PetMood::Sulking)) => '_',
        (_, Mouth::Mood(PetMood::Asleep)) => 'z',
        (_, Mouth::Mood(PetMood::Idle)) => '.',
        (_, Mouth::Hush) => '.',
        (_, Mouth::Gasp) => 'o',
        (_, Mouth::Sniff) => 'v',
    }
}

fn mood_color(mood: PetMood) -> Color {
    match mood {
        PetMood::Purring | PetMood::Proud => theme::AMBER_GLOW(),
        PetMood::Chatty | PetMood::Vibing | PetMood::Idle => theme::AMBER(),
        PetMood::Sulking => theme::TEXT_DIM(),
        PetMood::Asleep => theme::TEXT_FAINT(),
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
