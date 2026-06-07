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

/// Compact three-row cat for the sidebar rail. Mood reads through how lively
/// the cat is, not the label alone: an active cat roams the rail and flicks
/// its tail up; a drained one holds still with the tail drooped. The smile
/// (mouth) and tint shift with mood too.
pub fn draw_cat_inline(frame: &mut Frame, area: Rect, state: &PetState) {
    if area.height < 3 || area.width < 8 {
        return;
    }

    let mood = state.mood();
    let color = mood_color(mood);
    let tick = state.animation_ticks();
    let activity = cat_activity(mood);

    // The cat wanders the whole rail width, picking a fresh spot each leg.
    let travel = (area.width as usize).saturating_sub(CAT_WIDTH);
    let pad = " ".repeat(wander_x(tick, activity, travel));

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

    let mut lines: Vec<Line<'_>> = if state.roaming_active() {
        let label = if is_dog {
            "dog strolling"
        } else {
            "cat strolling"
        };
        vec![Line::from(Span::styled(
            label,
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ))]
    } else {
        vec![
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
        ]
    };

    if area.height >= 4 {
        let mut footer: Vec<Span<'_>> = vec![Span::styled(
            mood.label(),
            Style::default().fg(theme::TEXT_DIM()),
        )];
        footer.push(Span::raw("  "));
        footer.push(Span::styled(
            "c care",
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::ITALIC),
        ));
        lines.push(Line::from(footer));
    }

    frame.render_widget(Paragraph::new(lines), area);
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
const CAT_WIDTH: usize = 8;

/// Pseudo-random horizontal wander across the rail. The cat picks a fresh
/// column each leg and strolls to it, so legs land anywhere edge-to-edge;
/// livelier moods change their mind sooner. A still (sad) cat parks mid-rail.
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

/// How busy the cat looks, 0 (still) to 3 (bouncy). Drives the wander pace and
/// how often the tail flicks.
fn cat_activity(mood: PetMood) -> u8 {
    match mood {
        PetMood::Happy => 3,
        PetMood::Content => 2,
        PetMood::Bored | PetMood::Hungry | PetMood::Thirsty => 1,
        PetMood::Sad => 0,
    }
}

/// Tail glyphs for `[top row, body row]`. A still cat lets the tail droop;
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
            PetMood::Bored => '.',
            PetMood::Hungry => 'o',
            PetMood::Thirsty => 'v',
            PetMood::Sad => '_',
        };
    }
    match mood {
        PetMood::Happy => 'w',
        PetMood::Content => '^',
        PetMood::Bored => '.',
        PetMood::Hungry => 'o',
        PetMood::Thirsty => 'u',
        PetMood::Sad => '_',
    }
}

fn mood_color(mood: PetMood) -> Color {
    match mood {
        PetMood::Happy => theme::AMBER_GLOW(),
        PetMood::Content => theme::TEXT_BRIGHT(),
        PetMood::Bored => theme::AMBER_DIM(),
        PetMood::Hungry | PetMood::Thirsty => theme::AMBER(),
        PetMood::Sad => theme::TEXT_DIM(),
    }
}
