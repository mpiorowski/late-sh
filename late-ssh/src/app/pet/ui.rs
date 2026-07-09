use std::cell::Cell;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use late_core::models::pet::PET_SPECIES_DOG;

use super::state::{PetMood, PetNeedStatus, PetState};
use crate::app::common::theme;

/// Constant height of the pet strip that sits above the chat composer.
/// Stable chrome: the strip never grows or shrinks between states.
pub const PET_STRIP_HEIGHT: u16 = 3;

/// Pet strip inputs threaded through the chat render views. The rect slots
/// receive this frame's clickable targets (pet and food bowl both feed, water
/// bowl waters) so mouse hit-testing in `app::input` can route clicks.
pub struct PetStripView<'a> {
    pub state: &'a PetState,
    /// Pet food left in the Shop inventory. A meal costs one, so at zero the
    /// food bowl asks the user to restock instead of showing an empty dish.
    pub pet_food_quantity: i32,
    pub pet_rect_slot: Option<&'a Cell<Option<Rect>>>,
    pub food_bowl_rect_slot: Option<&'a Cell<Option<Rect>>>,
    pub water_bowl_rect_slot: Option<&'a Cell<Option<Rect>>>,
}

const BOWL_WIDTH: u16 = 9;
/// food bowl + gap + water bowl + right pad
const BOWLS_ZONE_WIDTH: u16 = BOWL_WIDTH + 1 + BOWL_WIDTH + 1;

/// Three-row strip above the composer: the pet wanders the left zone while
/// the food and water bowls sit pinned on the right. The bowls double as
/// status (a full bowl is done, an empty amber/red one is due) and as the
/// click targets for feeding/watering; clicking the pet feeds it too.
pub fn draw_pet_strip(frame: &mut Frame, area: Rect, view: &PetStripView<'_>) {
    if area.height < PET_STRIP_HEIGHT || area.width < BOWLS_ZONE_WIDTH + 12 {
        return;
    }
    let state = view.state;
    let needs = state.needs();

    let wander_zone = Rect {
        width: area.width - BOWLS_ZONE_WIDTH,
        ..area
    };
    let food_area = Rect {
        x: wander_zone.right(),
        width: BOWL_WIDTH,
        ..area
    };
    let water_area = Rect {
        x: food_area.right() + 1,
        width: BOWL_WIDTH,
        ..area
    };

    let pet_rect = draw_wandering_pet(frame, wander_zone, state);
    if let Some(slot) = view.pet_rect_slot {
        slot.set(pet_rect);
    }

    // Only nag about an empty pantry on a day the pet can still eat. Once fed,
    // the meal is spent until tomorrow and the amber label would be a false
    // alarm. Feeding also forces the bowl to `Done`, so a `?` never coexists
    // with a full dish.
    let needs_restock = view.pet_food_quantity <= 0 && !state.fed_today();
    draw_bowl(frame, food_area, '*', "/feed", needs.food, needs_restock);
    draw_bowl(frame, water_area, '~', "/water", needs.water, false);
    if let Some(slot) = view.food_bowl_rect_slot {
        slot.set(Some(food_area));
    }
    if let Some(slot) = view.water_bowl_rect_slot {
        slot.set(Some(water_area));
    }

    // Action feedback ("watered!", "fed! strolling", "buy pet food first")
    // sits right-aligned on the strip's bottom row, next to the bowls.
    if let Some(feedback) = state.action_feedback {
        let row = Rect {
            y: area.y + 2,
            height: 1,
            ..wander_zone
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

/// The pet's three art rows inside `zone`, wandering horizontally. Returns
/// the pet's on-screen rect (a second feed click target), or `None` while it
/// is off strolling through the whole app via the roaming overlay.
fn draw_wandering_pet(frame: &mut Frame, zone: Rect, state: &PetState) -> Option<Rect> {
    if state.roaming_active() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "strolling",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            )))
            .centered(),
            Rect {
                y: zone.y + 1,
                height: 1,
                ..zone
            },
        );
        return None;
    }

    let mood = state.mood();
    let color = mood_color(mood);
    let tick = state.animation_ticks();
    let activity = pet_activity(mood);

    // The pet wanders the whole zone width, picking a fresh spot each leg.
    let travel = (zone.width as usize).saturating_sub(PET_WIDTH);
    let x = wander_x(tick, activity, travel);
    let pad = " ".repeat(x);

    let blink = activity > 0 && tick % 64 < 3;
    let eyes = if blink { "-.-" } else { mood.eyes() };
    let tail = tail(activity, tick);
    let is_dog = state.species == PET_SPECIES_DOG;
    // Cat: pointy ears `/\_/\` going up. Dog: floppy ears `\,_,/` drooping
    // outward at the sides. Same 5-char crown so the face row aligns.
    let ears = if is_dog { " \\,_,/ " } else { " /\\_/\\ " };
    let mouth_row = if is_dog {
        format!(" \\_{}_/ ", mouth(mood, true))
    } else {
        format!(" > {} < ", mouth(mood, false))
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
    frame.render_widget(Paragraph::new(lines), zone);

    Some(Rect {
        x: zone.x + x as u16,
        y: zone.y,
        width: (PET_WIDTH as u16).min(zone.width),
        height: PET_STRIP_HEIGHT,
    })
}

/// Three-row bowl: fill + base + slash-command label. The bowl carries the
/// status on its own: a full green bowl is done, an empty amber/red one still
/// needs care. `needs_restock` is the food bowl's out-of-pantry state, where
/// the dish shows `?` rather than an empty bowl because there is nothing to
/// pour until the Shop restocks. Water is never gated, so it passes `false`.
fn draw_bowl(
    frame: &mut Frame,
    area: Rect,
    fill: char,
    label: &'static str,
    status: PetNeedStatus,
    needs_restock: bool,
) {
    let color = status_color(status);
    let inside = match (status, needs_restock) {
        (PetNeedStatus::Done, _) => fill.to_string().repeat(7),
        (_, false) => " ".repeat(7),
        (_, true) => "   ?   ".to_string(),
    };
    let label_style = if needs_restock {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::ITALIC)
    } else if status.is_missing() {
        Style::default().fg(color).add_modifier(Modifier::ITALIC)
    } else {
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC)
    };
    let lines = vec![
        Line::from(Span::styled(
            format!("({inside})"),
            Style::default().fg(color),
        ))
        .centered(),
        Line::from(Span::styled(" \\_____/ ", Style::default().fg(color))).centered(),
        Line::from(Span::styled(label, label_style)).centered(),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn status_color(status: PetNeedStatus) -> Color {
    match status {
        PetNeedStatus::Done => theme::SUCCESS(),
        PetNeedStatus::Due => theme::AMBER(),
        PetNeedStatus::Overdue => theme::ERROR(),
    }
}

pub fn draw_roaming_pet(frame: &mut Frame, area: Rect, state: &PetState) {
    if !state.roaming_active() || area.width < 12 || area.height < 5 {
        return;
    }

    let tick = state.animation_ticks();
    let (lines, width) = if state.species == PET_SPECIES_DOG {
        if (tick / 8).is_multiple_of(2) {
            ([r" \,_,/ ", r"( o.o )", r" /___\ "], 7)
        } else {
            ([r" \,_,/ ", r"( o.o )", r" _/ \_ "], 7)
        }
    } else if (tick / 8).is_multiple_of(2) {
        ([r" /\_/\ ", r"( o.o )", r" > ^ < "], 7)
    } else {
        ([r" /\_/\ ", r"( o.o )", r" > - < "], 7)
    };

    let max_x = (area.width as usize).saturating_sub(width);
    let max_y = (area.height as usize).saturating_sub(lines.len());
    let x = stroll_axis(tick, max_x, 150, 0);
    let y = stroll_axis(tick, max_y, 210, 17);
    let style = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let rendered = lines
        .into_iter()
        .map(|line| Line::from(Span::styled(line, style)))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(rendered),
        Rect::new(area.x + x as u16, area.y + y as u16, width as u16, 3),
    );
}

/// Body width including the tail column, used to keep the wander on-screen.
const PET_WIDTH: usize = 8;

/// Pseudo-random horizontal wander across the strip. The pet picks a fresh
/// column each leg and strolls to it, so legs land anywhere edge-to-edge;
/// livelier moods change their mind sooner. A still (sad) pet parks mid-zone.
fn wander_x(tick: usize, activity: u8, travel: usize) -> usize {
    if travel == 0 {
        return 0;
    }
    if activity == 0 {
        return travel / 2;
    }
    // Ticks per wander leg. Lower activity ambles more slowly.
    let leg = match activity {
        3 => 60,
        2 => 100,
        _ => 180,
    };
    let seg = tick / leg;
    let into = (tick % leg) as i64;
    let from = wander_target(seg, travel) as i64;
    let to = wander_target(seg + 1, travel) as i64;
    let pos = from + (to - from) * into / leg as i64;
    pos.clamp(0, travel as i64) as usize
}

/// Deterministic pseudo-random destination column for one wander leg. Adjacent
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

/// How busy the pet looks, 0 (still) to 3 (bouncy). Drives the wander pace and
/// how often the tail flicks.
fn pet_activity(mood: PetMood) -> u8 {
    match mood {
        PetMood::Happy => 3,
        PetMood::Content => 2,
        PetMood::Hungry | PetMood::Thirsty => 1,
        PetMood::Sad => 0,
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

fn mouth(mood: PetMood, is_dog: bool) -> char {
    if is_dog {
        return match mood {
            PetMood::Happy => 'd',
            PetMood::Content => 'u',
            PetMood::Hungry => 'o',
            PetMood::Thirsty => 'v',
            PetMood::Sad => '_',
        };
    }
    match mood {
        PetMood::Happy => 'w',
        PetMood::Content => '^',
        PetMood::Hungry => 'o',
        PetMood::Thirsty => 'u',
        PetMood::Sad => '_',
    }
}

fn mood_color(mood: PetMood) -> Color {
    match mood {
        PetMood::Happy => theme::AMBER_GLOW(),
        PetMood::Content => theme::TEXT_BRIGHT(),
        PetMood::Hungry | PetMood::Thirsty => theme::AMBER(),
        PetMood::Sad => theme::TEXT_DIM(),
    }
}
