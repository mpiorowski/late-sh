use std::cell::Cell;

use late_core::models::pet::{Form, PetGuest, PetMood, PetSpecies, Pick, Stage, TreatKind, Trick};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use super::state::{
    Body, Look, Moment, PET_HEIGHT, PET_WIDTH, Perch, PetFrameInputs, PetState, PetTravel,
    Plaything, Toy, visit_line,
};
use crate::app::common::{
    primitives::{draw_centered_note, truncate_cells},
    theme,
};

/// The pet's box at its smallest: the three art rows.
pub const PET_BOX_MIN_ROWS: u16 = PET_HEIGHT as u16;

/// The pets staying with this one that a box or a profile draws at most:
/// the newest.
const GUESTS_SHOWN: usize = 2;
/// The cells a guest takes of a box: its body, and room for its name.
const GUEST_CELL: usize = 14;
/// The cells between two bodies standing side by side on a profile.
const GUEST_GAP: usize = 2;

/// Where the tank is, seen from the pet's box, when the two share an edge
/// on Home. The pet goes and sits against that edge to watch.
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

/// What the pet's box touches on Home, with the side each one is
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
    /// An egg: it sits mid floor and goes nowhere.
    Nest,
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
        body: Body,
        neighbours: Neighbours,
        perch: Option<Perch>,
        tick: usize,
    ) -> Self {
        match body.form {
            Form::Egg => return PetPose::Nest,
            Form::Baby | Form::Young { .. } | Form::Adult { .. } | Form::Elder { .. } => {}
        }
        if let Some(perch) = perch {
            return PetPose::At(perch);
        }
        match body.mood {
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

/// Pet box inputs threaded through Home's tile view. The rect slot
/// receives this frame's click target (a click is a pet), the frame slot
/// what the tick needs to walk it and gate its frames.
pub struct PetView<'a> {
    pub state: &'a PetState,
    pub pet_rect_slot: Option<&'a Cell<Option<Rect>>>,
    pub frame_slot: Option<&'a Cell<Option<PetFrameInputs>>>,
}

/// The pet's box: the whole of `area` is its floor and its sky.
/// `neighbours` is the side of each tile it can go and watch, when one
/// touches this box. Returns what it recorded in the frame slot, `None`
/// when the area is too small to draw in.
pub fn draw_pet_box(
    frame: &mut Frame,
    area: Rect,
    view: &PetView<'_>,
    neighbours: Neighbours,
) -> Option<PetFrameInputs> {
    if area.height < PET_BOX_MIN_ROWS || area.width < PET_WIDTH as u16 + 2 {
        return None;
    }
    let state = view.state;
    // Guests stand in a strip on the right and the pet keeps the rest of
    // the box to itself. A box with a row to spare names each guest on its
    // bottom row, and everyone stands on the row over that; a box of three
    // rows (the stock Home) has the bodies only.
    let guests = &state.guests()[..guests_drawn(state.guests().len(), area)];
    let area = match guests.len() {
        0 => area,
        drawn => {
            let strip = (drawn * GUEST_CELL) as u16;
            let named = area.height > PET_BOX_MIN_ROWS;
            let floor = Rect {
                width: area.width - strip,
                height: area.height - u16::from(named),
                ..area
            };
            draw_guests(
                frame,
                Rect {
                    x: floor.right(),
                    width: strip,
                    ..area
                },
                guests,
                state.animation_ticks(),
                named,
            );
            floor
        }
    };
    let travel = PetTravel {
        x: (area.width as usize).saturating_sub(PET_WIDTH),
        y: (area.height as usize).saturating_sub(PET_HEIGHT),
    };
    let body = state.body();
    let tick = state.animation_ticks();
    let pose = PetPose::for_frame(body, neighbours, state.perch(), tick);
    let art = PetArt::at(body, pose, tick, travel);
    let pet_rect = draw_pet(frame, area, body, art);
    if let Some(toy) = state.plaything() {
        draw_toy(frame, area, pet_rect, toy, tick);
    }
    if let Some(slot) = view.pet_rect_slot {
        slot.set(Some(pet_rect));
    }
    let inputs = PetFrameInputs {
        travel,
        zone: area,
        neighbours,
        position: art.position,
        home: pet_position(
            PetPose::for_frame(body, neighbours, None, tick),
            tick,
            travel,
        ),
        rows: [None; 2],
        treat: None,
    };
    if let Some(slot) = view.frame_slot {
        slot.set(Some(inputs));
    }
    Some(inputs)
}

/// How many of the pets `staying` a box of `area` draws: each stands in a
/// cell of its own, and the guests never take more than half of the width.
fn guests_drawn(staying: usize, area: Rect) -> usize {
    staying
        .min(GUESTS_SHOWN)
        .min(usize::from(area.width) / 2 / GUEST_CELL)
}

/// The guests side by side in `strip`. `named`, each stands on the row
/// over the last, which names it; else on the last row, with no name.
fn draw_guests(frame: &mut Frame, strip: Rect, guests: &[PetGuest], tick: usize, named: bool) {
    let names = strip.bottom() - 1;
    let feet = strip.bottom() - u16::from(named);
    for (at, guest) in guests.iter().enumerate() {
        let x = strip.x + (at * GUEST_CELL) as u16;
        frame.render_widget(
            Paragraph::new(guest_lines(guest, tick)),
            Rect::new(
                x + ((GUEST_CELL - PET_WIDTH) / 2) as u16,
                feet - PET_HEIGHT as u16,
                PET_WIDTH as u16,
                PET_HEIGHT as u16,
            ),
        );
        if !named {
            continue;
        }
        // A cell short of the cell's width, so two names never touch.
        let name = truncate_cells(&guest_label(guest), GUEST_CELL - 1);
        frame.render_widget(
            Paragraph::new(
                Line::from(Span::styled(name, Style::default().fg(theme::TEXT_DIM()))).centered(),
            ),
            Rect::new(x, names, GUEST_CELL as u16, 1),
        );
    }
}

/// What a guest goes by under its feet: `mat's Biscuit`, or `mat's cat`
/// while it has no name.
pub fn guest_label(guest: &PetGuest) -> String {
    match &guest.name {
        Some(name) => format!("{}'s {name}", guest.username),
        None => format!("{}'s {}", guest.username, guest.species.as_str()),
    }
}

/// A guest as three rows: its own body, standing still on its best
/// behaviour, blinking on the wall clock.
fn guest_lines(guest: &PetGuest, tick: usize) -> Vec<Line<'static>> {
    portrait_lines(
        Portrait {
            species: guest.species,
            form: guest.form,
            mood: PetMood::Idle,
            carries: None,
            tricks: &[],
        },
        tick,
    )
}

/// Whether `guest` drawn at `now` differs from itself at `before`: a
/// blink, a flick of the tail. The tick asks, so a box with a guest in it
/// pays a frame only on those edges.
pub(super) fn guest_moved(guest: &PetGuest, before: usize, now: usize) -> bool {
    guest_lines(guest, before) != guest_lines(guest, now)
}

/// A profile's pet with the pets staying with it standing beside it, as
/// three rows: the host first, then as many guests as `width` cells hold.
pub fn portrait_with_guests(
    portrait: Portrait<'_>,
    guests: &[PetGuest],
    tick: usize,
    width: usize,
) -> Vec<Line<'static>> {
    let mut rows = portrait_lines(portrait, tick);
    let room = width.saturating_sub(PET_WIDTH) / (GUEST_GAP + PET_WIDTH);
    let mut filled = PET_WIDTH;
    for guest in guests.iter().take(GUESTS_SHOWN.min(room)) {
        for (row, drawn) in rows.iter_mut().zip(guest_lines(guest, tick)) {
            let pad = (filled + GUEST_GAP).saturating_sub(row.width());
            row.spans.push(Span::raw(" ".repeat(pad)));
            row.spans.extend(drawn.spans);
        }
        filled += GUEST_GAP + PET_WIDTH;
    }
    rows
}

/// The toy on the floor of the box, unless the pet stands on it.
fn draw_toy(frame: &mut Frame, zone: Rect, pet: Rect, toy: Plaything, tick: usize) {
    let glyph = match toy.toy {
        Toy::Ball => "o",
        Toy::Mouse => match tick % 8 < 4 {
            true => "~",
            false => "-",
        },
    };
    let x = zone.x + (toy.x as u16).min(zone.width.saturating_sub(1));
    let y = zone.y + (toy.y as u16).min(zone.height.saturating_sub(1));
    let on_pet = x >= pet.x && x < pet.right() && y >= pet.y && y < pet.bottom();
    if on_pet {
        return;
    }
    frame.render_widget(
        Paragraph::new(Span::styled(
            glyph,
            Style::default().fg(theme::AMBER_GLOW()),
        )),
        Rect::new(x, y, 1, 1),
    );
}

/// What the pet waits on its owner for, or has for them, in a few words:
/// the tile's status row leads with it and its pouch row spells it out.
fn waits_on(state: &PetState) -> Option<&'static str> {
    if state.ready_to_hatch() {
        return Some("ready to hatch");
    }
    if state.offers().is_some() {
        return Some("ready to grow");
    }
    match (state.waiting(), state.visitors().is_empty()) {
        (Some(_), _) => Some("a treat waits"),
        (None, false) => Some("someone came by"),
        (None, true) => None,
    }
}

/// The row over the pet's box: its name and today's title, what it waits
/// on, its stage, its days and its mood, and what it is up to when that is more
/// than a stroll. Segments are kept left to right while they fit `width`.
pub fn status_line(state: &PetState, neighbours: Neighbours, width: u16) -> Line<'static> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let growth = state.growth();
    let body = state.body();
    let pose = PetPose::for_frame(body, neighbours, state.perch(), state.animation_ticks());
    let name = match state.title() {
        Some(title) => format!("{} {title}", state.display_name()),
        None => state.display_name(),
    };
    let mut segments: Vec<(String, Style)> = Vec::new();
    if let Some(waits) = waits_on(state) {
        segments.push((waits.to_string(), Style::default().fg(theme::AMBER_GLOW())));
    }
    if let Some((host, left)) = state.calling() {
        segments.push((format!("visiting {host}, {left} min left"), dim));
    }
    // One guest is there to see. More than that and the box may not hold
    // them all, so the row counts them.
    match state.guests().len() {
        0 | 1 => {}
        staying => segments.push((format!("{staying} guests"), dim)),
    }
    segments.push((growth.stage().as_str().to_string(), dim));
    segments.push((format!("day {}", growth.days_together), dim));
    match growth.stage() {
        Stage::Egg => {}
        Stage::Baby | Stage::Young | Stage::Adult | Stage::Elder => {
            segments.push((state.mood().as_str().to_string(), dim));
        }
    }
    match pose {
        PetPose::Watch(WatchTarget::Tank, _) => {
            segments.push(("watching the fish".to_string(), dim));
        }
        PetPose::Watch(WatchTarget::Bonsai, _) => {
            segments.push(("watching the tree".to_string(), dim));
        }
        PetPose::At(_) => segments.push(("at your cursor".to_string(), dim)),
        PetPose::Stroll | PetPose::Sulk | PetPose::Sleep | PetPose::Nest => {}
    }

    let mut used = name.width();
    let mut spans = vec![Span::styled(
        name,
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
    )];
    for (text, style) in segments {
        let cost = text.width() + 3;
        if used + cost > usize::from(width) {
            break;
        }
        used += cost;
        spans.push(Span::styled(" · ", dim));
        spans.push(Span::styled(text, style));
    }
    Line::from(spans)
}

/// The row under the pet's box, when it has something to say: the treat
/// in the pouch (a click target, as an x offset and a width inside the
/// row), or what the pet waits on.
fn pouch_row(state: &PetState, width: u16) -> Option<(Line<'static>, Option<(u16, u16)>)> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let glow = Style::default().fg(theme::AMBER_GLOW());
    let text = |text: String, style: Style| Some((Line::from(Span::styled(text, style)), None));
    if state.ready_to_hatch() {
        return text("click the egg: pick what hatches".to_string(), glow);
    }
    if state.offers().is_some() {
        return text("click it: pick how it grows".to_string(), glow);
    }
    let Some(kind) = state.waiting() else {
        return match state.visitors() {
            [] => None,
            [one] => text(visit_line(one, &state.display_name()), dim),
            many => text(format!("{} came by", many.len()), dim),
        };
    };
    let lead = "treat ";
    let chip = format!("[{}]", kind.as_str());
    let chip_width = chip.width();
    // The row is drawn centred: the target is an offset from its left edge.
    let x = usize::from(width).saturating_sub(lead.width() + chip_width) / 2 + lead.width();
    let target = (x + chip_width <= usize::from(width)).then_some((x as u16, chip_width as u16));
    Some((
        Line::from(vec![Span::styled(lead, dim), Span::styled(chip, glow)]),
        target,
    ))
}

/// The pet's three art rows inside `zone`, standing where the art says.
/// Returns the pet's on-screen rect (the click target).
fn draw_pet(frame: &mut Frame, zone: Rect, body: Body, art: PetArt) -> Rect {
    let (x, y) = art.position;
    let lines = art_lines(body, art, x);
    let rows = Rect::new(zone.x, zone.y + y as u16, zone.width, PET_HEIGHT as u16);
    frame.render_widget(Paragraph::new(lines), rows);

    Rect {
        x: zone.x + x as u16,
        width: (PET_WIDTH as u16).min(zone.width),
        ..rows
    }
}

/// What a profile draws of a pet: enough for its body, and the tricks it
/// shows a visitor.
#[derive(Clone, Copy, Debug)]
pub struct Portrait<'a> {
    pub species: PetSpecies,
    pub form: Form,
    pub mood: PetMood,
    pub carries: Option<TreatKind>,
    pub tricks: &'a [Trick],
}

/// Wall ticks between the tricks a portrait performs: about twenty seconds.
const PORTRAIT_TRICK_EVERY: usize = 300;

/// The pet as three rows for a profile: its body at its stage in its
/// stored mood, standing still, blinking on the wall clock, and running
/// through its tricks one at a time. No box, no walk.
pub fn portrait_lines(portrait: Portrait<'_>, tick: usize) -> Vec<Line<'static>> {
    let round = tick / PORTRAIT_TRICK_EVERY;
    let began = round * PORTRAIT_TRICK_EVERY;
    let awake = match portrait.mood {
        PetMood::Sulking | PetMood::Asleep => false,
        PetMood::Purring | PetMood::Proud | PetMood::Chatty | PetMood::Vibing | PetMood::Idle => {
            true
        }
    };
    let moment = match (awake, portrait.tricks.is_empty()) {
        (true, false) => {
            let trick = Moment::Trick(portrait.tricks[round % portrait.tricks.len()]);
            (tick - began < trick.ticks()).then_some((trick, began))
        }
        (true, true) | (false, _) => None,
    };
    let body = Body {
        species: portrait.species,
        form: portrait.form,
        mood: portrait.mood,
        ready: false,
        carries: portrait.carries,
        moment,
    };
    let pose = match (portrait.form.stage(), portrait.mood) {
        (Stage::Egg, _) => PetPose::Nest,
        (_, PetMood::Sulking) => PetPose::Sulk,
        (_, PetMood::Asleep) => PetPose::Sleep,
        (
            _,
            PetMood::Purring | PetMood::Proud | PetMood::Chatty | PetMood::Vibing | PetMood::Idle,
        ) => PetPose::At(Perch {
            x: 0,
            y: 0,
            look: Look::Ahead,
        }),
    };
    let art = PetArt::at(body, pose, tick, PetTravel { x: 0, y: 0 });
    art_lines(body, art, 0)
}

/// A pet of `species` in `form`, idle and standing still: its three rows,
/// seven cells each. What the care box shows of each animal an egg can
/// hatch into and of each drawing a pet can grow into.
pub(super) fn still_rows(species: PetSpecies, form: Form) -> [String; 3] {
    let (crown, face, floor) = form_rows(
        species,
        form,
        "o.o",
        mouth(Mouth::Mood(PetMood::Idle), species),
        false,
    );
    [crown, face, floor]
}

/// The glyph of the thing a pet carries on the day it ate a `kind` treat,
/// drawn in the cell after its feet.
pub fn carried_glyph(kind: TreatKind) -> char {
    match kind {
        TreatKind::Puzzle => '#',
        TreatKind::Arcade => '*',
        TreatKind::Match => '&',
        TreatKind::Table => '$',
        TreatKind::Door => '!',
    }
}

/// The three rows, `pad` cells in from the left, in the mood's colour.
fn art_lines(body: Body, art: PetArt, pad: usize) -> Vec<Line<'static>> {
    let stage = body.form.stage();
    let color = match stage {
        Stage::Egg => theme::AMBER(),
        Stage::Baby | Stage::Young | Stage::Adult | Stage::Elder => mood_color(body.mood),
    };
    let pad = " ".repeat(pad);
    let tail = art.tail;
    let (crown, face, floor) = form_rows(
        body.species,
        body.form,
        art.eyes,
        mouth(art.mouth, body.species),
        art.rock,
    );
    // The eighth column: the tail beside the body, and on the floor the
    // thing it carries. A baby is a head on the floor, so its tail is
    // there too and it carries nothing: its title alone says what it ate.
    let column: [String; 3] = match (stage, body.carries) {
        (Stage::Egg, _) => [String::new(), String::new(), String::new()],
        (Stage::Baby, _) => [" ".to_string(), " ".to_string(), tail[1].to_string()],
        (Stage::Young | Stage::Adult | Stage::Elder, None) => {
            [tail[0].to_string(), tail[1].to_string(), String::new()]
        }
        (Stage::Young | Stage::Adult | Stage::Elder, Some(kind)) => [
            tail[0].to_string(),
            tail[1].to_string(),
            carried_glyph(kind).to_string(),
        ],
    };
    vec![
        Line::from(Span::styled(
            format!("{pad}{crown}{}", column[0]),
            Style::default().fg(color),
        )),
        Line::from(Span::styled(
            format!("{pad}{face}{}", column[1]),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("{pad}{floor}{}", column[2]),
            Style::default().fg(color),
        )),
    ]
}

/// The three rows before the tail column, seven cells each so the face
/// aligns across species and forms.
///
/// The egg is a shell, and rocks (`rock`) once it has its days. A baby is
/// a head with the start of ears. A young one grows the ears (or the
/// crest) its owner picked, full size on a small head, over small feet. An
/// adult's head widens and it stands on the feet picked for it. An elder
/// wears `{ }` and the face picked for it; the bird's head stays small and
/// its elder pick is which way its wings point.
fn form_rows(
    species: PetSpecies,
    form: Form,
    eyes: &str,
    mouth: char,
    rock: bool,
) -> (String, String, String) {
    match form {
        Form::Egg => (
            "  ,-,  ".to_string(),
            match rock {
                true => " / ' \\ ".to_string(),
                false => " /   \\ ".to_string(),
            },
            " \\___/ ".to_string(),
        ),
        Form::Baby => (
            "       ".to_string(),
            match species {
                PetSpecies::Cat => "  ^ ^  ",
                PetSpecies::Dog => "  u u  ",
                PetSpecies::Bird => "   ,   ",
            }
            .to_string(),
            small_face(species, eyes, mouth),
        ),
        Form::Young { top } => (
            top_row(species, top).to_string(),
            small_face(species, eyes, mouth),
            match species {
                PetSpecies::Cat => format!("  >{mouth}<  "),
                PetSpecies::Dog => format!("  \\{mouth}/  "),
                PetSpecies::Bird => "  ^ ^  ".to_string(),
            },
        ),
        Form::Adult { top, feet } => (
            top_row(species, top).to_string(),
            match species {
                PetSpecies::Cat | PetSpecies::Dog => format!("( {eyes} )"),
                PetSpecies::Bird => small_face(species, eyes, mouth),
            },
            feet_row(species, feet, mouth),
        ),
        Form::Elder { top, feet, face } => (
            top_row(species, top).to_string(),
            elder_face(species, face, eyes, mouth),
            feet_row(species, feet, mouth),
        ),
    }
}

/// What the drawing `form`'s latest pick chose is called, for the care
/// box. An egg and a baby have picked nothing.
pub(super) fn pick_name(species: PetSpecies, form: Form) -> &'static str {
    match form {
        Form::Egg | Form::Baby => "",
        Form::Young { top } => match (species, top) {
            (PetSpecies::Cat, Pick::One) => "pointed",
            (PetSpecies::Cat, Pick::Two) => "straight",
            (PetSpecies::Dog, Pick::One) => "floppy",
            (PetSpecies::Dog, Pick::Two) => "upright",
            (PetSpecies::Bird, Pick::One) => "tuft",
            (PetSpecies::Bird, Pick::Two) => "horns",
        },
        Form::Adult { feet, .. } => match (species, feet) {
            (PetSpecies::Cat, Pick::One) => "neat paws",
            (PetSpecies::Cat, Pick::Two) => "big paws",
            (PetSpecies::Dog, Pick::One) => "tucked",
            (PetSpecies::Dog, Pick::Two) => "paws out",
            (PetSpecies::Bird, Pick::One) => "small feet",
            (PetSpecies::Bird, Pick::Two) => "long legs",
        },
        Form::Elder { face, .. } => match (species, face) {
            (PetSpecies::Cat | PetSpecies::Dog, Pick::One) => "plain",
            (PetSpecies::Cat, Pick::Two) => "whiskers",
            (PetSpecies::Dog, Pick::Two) => "freckles",
            (PetSpecies::Bird, Pick::One) => "wings down",
            (PetSpecies::Bird, Pick::Two) => "wings up",
        },
    }
}

/// What sits between the brackets of a face. The bird's beak is between
/// its eyes; the cat's and the dog's mouths are on the row below.
fn face_cells(species: PetSpecies, eyes: &str, mouth: char) -> String {
    match species {
        PetSpecies::Cat | PetSpecies::Dog => eyes.to_string(),
        PetSpecies::Bird => format!("{}{mouth}{}", &eyes[..1], &eyes[2..]),
    }
}

/// The small head of a baby and a young one, and of a bird until it is an
/// elder.
fn small_face(species: PetSpecies, eyes: &str, mouth: char) -> String {
    format!(" ({}) ", face_cells(species, eyes, mouth))
}

/// The pick made on becoming young: the cat's ears pointed or straight,
/// the dog's floppy or upright, the bird's crest a tuft or horns.
fn top_row(species: PetSpecies, top: Pick) -> &'static str {
    match (species, top) {
        (PetSpecies::Cat, Pick::One) => " /\\_/\\ ",
        (PetSpecies::Cat, Pick::Two) => " |\\_/| ",
        (PetSpecies::Dog, Pick::One) => " \\,_,/ ",
        (PetSpecies::Dog, Pick::Two) => " /|_|\\ ",
        (PetSpecies::Bird, Pick::One) => "  \\|/  ",
        (PetSpecies::Bird, Pick::Two) => " ^   ^ ",
    }
}

/// The pick made on becoming adult: the cat's paws neat or big, the dog's
/// tucked or out, the bird's feet small or on long legs.
fn feet_row(species: PetSpecies, feet: Pick, mouth: char) -> String {
    match (species, feet) {
        (PetSpecies::Cat, Pick::One) => format!(" > {mouth} < "),
        (PetSpecies::Cat, Pick::Two) => format!(" d {mouth} b "),
        (PetSpecies::Dog, Pick::One) => format!(" \\_{mouth}_/ "),
        (PetSpecies::Dog, Pick::Two) => format!(" w_{mouth}_w "),
        (PetSpecies::Bird, Pick::One) => "  \" \"  ".to_string(),
        (PetSpecies::Bird, Pick::Two) => "  J L  ".to_string(),
    }
}

/// The pick made on becoming elder: the cat's face plain or whiskered,
/// the dog's plain or freckled, the bird's wings down or up.
fn elder_face(species: PetSpecies, face: Pick, eyes: &str, mouth: char) -> String {
    let cells = face_cells(species, eyes, mouth);
    match (species, face) {
        (PetSpecies::Cat | PetSpecies::Dog, Pick::One) => format!("{{ {cells} }}"),
        (PetSpecies::Cat, Pick::Two) => format!("{{={cells}=}}"),
        (PetSpecies::Dog, Pick::Two) => format!("{{:{cells}:}}"),
        (PetSpecies::Bird, Pick::One) => format!("/{{{cells}}}\\"),
        (PetSpecies::Bird, Pick::Two) => format!("\\{{{cells}}}/"),
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
    /// Eating: open and shut.
    Chew(bool),
    /// In the middle of a trick or a welcome: a grin.
    Grin,
}

/// Every tick-dependent piece of the pet's art, computed once so the draw
/// and the tick-side gate (`frame_changed`) can never disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PetArt {
    pub position: (usize, usize),
    eyes: &'static str,
    tail: [&'static str; 2],
    mouth: Mouth,
    /// The egg leans this frame.
    rock: bool,
}

impl PetArt {
    fn at(body: Body, pose: PetPose, tick: usize, travel: PetTravel) -> Self {
        let mood = body.mood;
        let activity = pet_activity(mood, pose);
        let blink = activity > 0 && tick % 64 < 3;
        let tail = tail(activity, tick);
        let position = pet_position(pose, tick, travel);
        let art = match pose {
            // A shell: no eyes, no tail. It rocks every second or so once
            // it has its days.
            PetPose::Nest => PetArt {
                position,
                eyes: "   ",
                tail: [" ", " "],
                mouth: Mouth::Hush,
                rock: body.ready && tick % 16 < 8,
            },
            PetPose::Sulk | PetPose::Sleep | PetPose::Stroll => PetArt {
                position,
                eyes: if blink { "-.-" } else { mood_eyes(mood) },
                tail,
                mouth: Mouth::Mood(mood),
                rock: false,
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
                rock: false,
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
                    rock: false,
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
                    rock: false,
                }
            }
        };
        match (pose, body.moment) {
            (PetPose::Nest, _) | (_, None) => art,
            (
                PetPose::Stroll
                | PetPose::Watch(..)
                | PetPose::Sulk
                | PetPose::Sleep
                | PetPose::At(_),
                Some((moment, began)),
            ) => art.in_moment(moment, tick.saturating_sub(began), travel),
        }
    }

    /// The art `age` wall ticks into `moment`: the moment takes the face,
    /// and a hop or a dance moves the body a cell inside its travel.
    fn in_moment(self, moment: Moment, age: usize, travel: PetTravel) -> Self {
        let (x, y) = self.position;
        let up = (x, y.saturating_sub(1));
        let sway = match (age / 6) % 4 {
            0 => (x.saturating_sub(1), y),
            2 => ((x + 1).min(travel.x), y),
            _ => (x, y),
        };
        let wag = match age % 8 < 4 {
            true => [")", "/"],
            false => [" ", "~"],
        };
        match moment {
            Moment::Eating(_) => PetArt {
                eyes: "^.^",
                mouth: Mouth::Chew(age % 8 < 4),
                ..self
            },
            Moment::Hatched => PetArt {
                eyes: if age < 20 { "O.O" } else { "^.^" },
                mouth: Mouth::Grin,
                ..self
            },
            Moment::Welcome { .. } => PetArt {
                position: if (age / 8) % 2 == 0 { up } else { (x, y) },
                eyes: "^.^",
                tail: wag,
                mouth: Mouth::Grin,
                ..self
            },
            Moment::Trick(Trick::Spin) => PetArt {
                eyes: match (age / 5) % 4 {
                    0 => ">.>",
                    1 => "...",
                    2 => "<.<",
                    _ => "^.^",
                },
                mouth: Mouth::Grin,
                ..self
            },
            Moment::Trick(Trick::Bow) => PetArt {
                eyes: if age % 30 < 18 { "u.u" } else { "^.^" },
                mouth: Mouth::Hush,
                ..self
            },
            Moment::Trick(Trick::Hop) => PetArt {
                position: if (age / 6) % 2 == 0 { up } else { (x, y) },
                eyes: "^.^",
                mouth: Mouth::Grin,
                ..self
            },
            Moment::Trick(Trick::Dance) => PetArt {
                position: sway,
                eyes: if (age / 6) % 2 == 0 { "^.^" } else { "-.-" },
                tail: wag,
                mouth: Mouth::Grin,
                ..self
            },
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
        PetPose::Sulk | PetPose::Sleep | PetPose::Nest => (travel.x / 2, travel.y),
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
/// for the given body, neighbours, perch, and travel: a stroll step, a
/// blink, a tail flick, a gasp or sniff edge, the walk to and from the
/// glass, a frame of a moment (a treat eaten, a trick), an egg rocking. It
/// compares the same `PetArt` the draw uses, so the render gate only pays
/// frames on ticks where the box actually changes; a sleeping pet and an
/// egg still short of its days are fully static.
pub fn frame_changed(
    body: Body,
    neighbours: Neighbours,
    perch: Option<Perch>,
    tick: usize,
    travel: PetTravel,
) -> bool {
    let prev = tick.wrapping_sub(1);
    let now = PetPose::for_frame(body, neighbours, perch, tick);
    let before = PetPose::for_frame(body, neighbours, perch, prev);
    PetArt::at(body, now, tick, travel) != PetArt::at(body, before, prev, travel)
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
        PetPose::Sleep | PetPose::Nest => 0,
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

/// The mouth glyph. The bird's is its beak, drawn between its eyes and
/// always the same: birds do not smile.
fn mouth(mouth: Mouth, species: PetSpecies) -> char {
    match (species, mouth) {
        (PetSpecies::Bird, _) => 'v',
        (PetSpecies::Dog, Mouth::Mood(PetMood::Purring | PetMood::Proud | PetMood::Vibing)) => 'd',
        (PetSpecies::Cat, Mouth::Mood(PetMood::Purring | PetMood::Proud | PetMood::Vibing)) => 'w',
        (_, Mouth::Mood(PetMood::Chatty)) => 'o',
        (_, Mouth::Mood(PetMood::Sulking)) => '_',
        (_, Mouth::Mood(PetMood::Asleep)) => 'z',
        (_, Mouth::Mood(PetMood::Idle)) => '.',
        (_, Mouth::Hush) => '.',
        (_, Mouth::Gasp) => 'o',
        (_, Mouth::Sniff) => 'v',
        (_, Mouth::Chew(true)) => 'o',
        (_, Mouth::Chew(false)) => '-',
        (PetSpecies::Dog, Mouth::Grin) => 'd',
        (PetSpecies::Cat, Mouth::Grin) => 'w',
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

/// Home's pet tile: the box at tile size, the status row on top and the
/// pouch row underneath when there is room (the pouch only when it has
/// something to say), and the whole rest of the tile to roam. `neighbours`
/// names the side a tank and a bonsai are on; a calm pet goes and watches
/// them. With the pet put away the tile says how to bring it back.
pub fn draw_tile(frame: &mut Frame, area: Rect, pet: Option<&PetView<'_>>, neighbours: Neighbours) {
    let Some(view) = pet else {
        draw_centered_note(frame, area, &["your pet is put away", "back with /pet out"]);
        return;
    };
    if area.height < PET_BOX_MIN_ROWS {
        return;
    }
    let mut box_area = area;
    let mut status = None;
    if area.height >= PET_BOX_MIN_ROWS + 2 {
        let row = Rect::new(area.x, area.y, area.width, 1);
        frame.render_widget(
            Paragraph::new(status_line(view.state, neighbours, area.width).centered()),
            row,
        );
        status = Some(row);
        box_area = Rect::new(area.x, area.y + 1, area.width, area.height - 1);
    }
    let mut pouch = None;
    let mut treat = None;
    if box_area.height >= PET_BOX_MIN_ROWS + 2
        && let Some((line, target)) = pouch_row(view.state, area.width)
    {
        let row = Rect::new(area.x, box_area.bottom() - 1, area.width, 1);
        frame.render_widget(Paragraph::new(line.centered()), row);
        pouch = Some(row);
        treat = target.map(|(x, width)| Rect::new(row.x + x, row.y, width, 1));
        box_area.height -= 1;
    }
    let Some(inputs) = draw_pet_box(frame, box_area, view, neighbours) else {
        return;
    };
    if let Some(slot) = view.frame_slot {
        slot.set(Some(PetFrameInputs {
            rows: [status, pouch],
            treat,
            ..inputs
        }));
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
