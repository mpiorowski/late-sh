use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{CatMood, CatState};
use crate::app::common::theme;

/// Compact three-row cat for the sidebar rail. Mood reads through how lively
/// the cat is, not the label alone: an active cat roams the rail and flicks
/// its tail up; a drained one holds still with the tail drooped. The smile
/// (mouth) and tint shift with mood too.
pub fn draw_cat_inline(frame: &mut Frame, area: Rect, state: &CatState) {
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

    let mut lines: Vec<Line<'_>> = vec![
        Line::from(Span::styled(
            format!("{pad} /\\_/\\ {}", tail[0]),
            Style::default().fg(color),
        )),
        Line::from(Span::styled(
            format!("{pad}( {eyes} ){}", tail[1]),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("{pad} > {} < ", mouth(mood)),
            Style::default().fg(color),
        )),
    ];

    if area.height >= 4 {
        let mut footer: Vec<Span<'_>> = vec![Span::styled(
            mood.label(),
            Style::default().fg(theme::TEXT_DIM()),
        )];
        if let Some(fb) = state.action_feedback {
            footer.push(Span::raw("  "));
            footer.push(Span::styled(
                fb,
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::ITALIC),
            ));
        } else {
            footer.push(Span::raw("  "));
            footer.push(Span::styled(
                state.next_need_hint(),
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::ITALIC),
            ));
        }
        lines.push(Line::from(footer));
    }

    frame.render_widget(Paragraph::new(lines), area);
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

/// How busy the cat looks, 0 (still) to 3 (bouncy). Drives the wander pace and
/// how often the tail flicks.
fn cat_activity(mood: CatMood) -> u8 {
    match mood {
        CatMood::Happy => 3,
        CatMood::Content | CatMood::Hungry | CatMood::Thirsty => 2,
        CatMood::Bored => 1,
        CatMood::Sad => 0,
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

fn mouth(mood: CatMood) -> char {
    match mood {
        CatMood::Happy => 'w',
        CatMood::Content => '^',
        CatMood::Bored => '.',
        CatMood::Hungry => 'o',
        CatMood::Thirsty => 'u',
        CatMood::Sad => '_',
    }
}

fn mood_color(mood: CatMood) -> Color {
    match mood {
        CatMood::Happy => theme::AMBER_GLOW(),
        CatMood::Content => theme::TEXT_BRIGHT(),
        CatMood::Bored => theme::AMBER_DIM(),
        CatMood::Hungry | CatMood::Thirsty => theme::AMBER(),
        CatMood::Sad => theme::TEXT_DIM(),
    }
}
