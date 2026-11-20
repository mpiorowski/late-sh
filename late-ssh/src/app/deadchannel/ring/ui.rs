//! The ring's panel, in the city's palette. With nobody in the ring: the
//! deck this runner brings, the marked runners on the wire with their
//! records and the cursor on one, and the calls waiting. With a bout
//! set: both corners, their decks, the pool and the bell's clock, and
//! this runner's bet. With the rounds on: both signals, both hands (open
//! to everybody), a fighter's own slots (nobody else's), the round that
//! last came out, and the clock. After: the result, the pool's split and
//! what this runner was paid. Pure: a function of the session's copy and
//! the clock. The city's `draw_panel` frames it and puts the ring's last
//! word at its foot.

use chrono::{DateTime, Utc};
use late_core::models::deadchannel_bout::{Bout, Side};
use late_core::models::presence::PresenceRecord;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use serde::Deserialize;
use uuid::Uuid;

use super::data::{MAX_PRESSES, PURSE_PERCENT, RAKE_PERCENT, ROUNDS, SIGNAL, SLOTS};
use super::session::{Contender, Phase, RingSession, bet_of, call_from, call_to, phase, roster};
use super::state::{self, Ending, Move, Picks, Pool, SIDES, Settlement};
use super::svc::{Board, LiveBout};
use crate::app::deadchannel::city::map::Neon;
use crate::app::deadchannel::city::ui::{INK, INK_BRIGHT, INK_DIM, INK_MUTED, action, ink, lit};
use crate::app::deadchannel::fight::cards::{Card, DRAFTS, ENERGY};
use crate::app::deadchannel::fight::data::RULES;
use crate::app::deadchannel::fight::state::Sheet;
use crate::app::deadchannel::fight::ui::{card_mark, card_style};
use crate::app::deadchannel::runner::svc::RunnerLooks;

/// Cells in a signal bar.
const BAR: usize = 20;
/// The width a corner's column is padded to in the tape.
const TAPE_COLUMN: usize = 22;
/// The width of the tape's first column: the round's name over the slots'.
const TAPE_LABEL: usize = 22;

pub(crate) struct RingView<'a> {
    /// The session's copy of the board: what this runner may see.
    pub board: &'a Board,
    /// The deck this runner brings, once read.
    pub picks: Option<Picks>,
    pub cursor: usize,
    /// The corner a bet goes on.
    pub corner: Side,
    /// The marked runners on the wire, as the cursor walks them.
    pub roster: Vec<Contender>,
    pub me: Uuid,
    /// The runner's sheet mirror: the mark, the level a stake is cut to,
    /// and the gate.
    pub sheet: Option<&'a Sheet>,
    pub now: DateTime<Utc>,
    pub word: Option<&'a str>,
}

impl<'a> RingView<'a> {
    pub(crate) fn new(
        session: &'a RingSession,
        records: &[PresenceRecord],
        looks: &RunnerLooks,
        me: Uuid,
        sheet: Option<&'a Sheet>,
    ) -> Self {
        Self {
            board: &session.board,
            picks: session.picks,
            cursor: session.cursor,
            corner: session.corner,
            roster: roster(records, looks, me),
            me,
            sheet,
            now: Utc::now(),
            word: session.word.as_deref(),
        }
    }
}

struct Palette {
    text: Style,
    dim: Style,
    muted: Style,
    head: Style,
    key: Style,
    number: Style,
}

fn palette() -> Palette {
    Palette {
        text: ink(INK),
        dim: ink(INK_DIM),
        muted: ink(INK_MUTED),
        head: ink(INK_BRIGHT).add_modifier(Modifier::BOLD),
        key: lit(Neon::Amber),
        number: lit(Neon::Amber),
    }
}

/// `[k] verb`, the way every counter writes a key.
fn hint(key: &str, verb: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!("[{key}] "), palette().key),
        Span::styled(format!("{verb}   "), action()),
    ]
}

/// Seconds as `m:ss`.
fn clock(secs: i64) -> String {
    let secs = secs.max(0);
    format!("{}:{:02}", secs / 60, secs % 60)
}

pub(crate) fn panel_lines(view: &RingView<'_>) -> Vec<Line<'static>> {
    match phase(view.board.live.as_ref(), view.now) {
        Phase::Empty => empty_lines(view),
        Phase::Set { live, bell_in } => set_lines(view, live, bell_in),
        Phase::On { live, clock } => on_lines(view, live, clock),
        Phase::Over { live, clears_in } => over_lines(view, live, clears_in),
    }
}

/// A runner's record on the board.
fn record(view: &RingView<'_>, user_id: Uuid) -> String {
    match view.board.records.get(&user_id) {
        Some((won, lost)) => format!("{won} won, {lost} lost"),
        None => "no bouts yet".to_string(),
    }
}

/// Nobody in the ring: the deck, the wire, the calls.
fn empty_lines(view: &RingView<'_>) -> Vec<Line<'static>> {
    let p = palette();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let marked = view.sheet.is_some_and(|sheet| sheet.marks >= 1);
    lines.push(Line::from(Span::styled(
        "two runners, the same numbers, card against card. the street bets bits on it.",
        p.text,
    )));
    if !marked {
        lines.push(Line::from(Span::styled(
            "the ropes are for runners who put the Old Signal down. you can watch here, and bet once a bout is set.",
            p.dim,
        )));
    }
    if let Some(level) = view.sheet.and_then(Sheet::ring_gate) {
        lines.push(Line::from(Span::styled(
            format!(
                "you owe the ring a bout at level {level}. the road is shut until you fight one, win or lose."
            ),
            lit(Neon::Red),
        )));
    }
    lines.push(Line::default());

    if marked {
        lines.push(Line::from(Span::styled("the deck you bring", p.head)));
        match view.picks {
            Some(picks) => {
                let mut spans = vec![Span::styled("  ", p.text)];
                for (index, (draft, pick)) in DRAFTS.iter().zip(picks.cards()).enumerate() {
                    spans.push(Span::styled(format!("[{}] ", index + 1), p.key));
                    spans.push(card_mark(*pick));
                    spans.push(Span::styled(
                        format!(" not {}   ", draft.other(*pick).name()),
                        p.muted,
                    ));
                }
                lines.push(Line::from(spans));
                lines.push(Line::from(Span::styled(
                    "  the ten, these four over them, and the other card of every draft you inked. level and kit stay outside.",
                    p.dim,
                )));
            }
            None => lines.push(Line::from(Span::styled("  reading it...", p.muted))),
        }
        lines.push(Line::default());

        lines.push(Line::from(Span::styled("marked, on the wire", p.head)));
        if view.roster.is_empty() {
            lines.push(Line::from(Span::styled(
                "  nobody else with a mark is on the wire. the ring waits.",
                p.dim,
            )));
        }
        let cursor = view.cursor.min(view.roster.len().saturating_sub(1));
        for (index, contender) in view.roster.iter().enumerate() {
            let on = index == cursor;
            lines.push(Line::from(vec![
                Span::styled(
                    match on {
                        true => "  ▸ ",
                        false => "    ",
                    },
                    p.key,
                ),
                Span::styled(
                    format!("{:<20}", contender.username),
                    match on {
                        true => p.head,
                        false => p.text,
                    },
                ),
                Span::styled(format!("marks {:<4}", contender.marks), p.dim),
                Span::styled(record(view, contender.user_id), p.dim),
            ]));
        }
        lines.push(Line::default());
    }

    let calls = &view.board.calls;
    if !calls.is_empty() {
        lines.push(Line::from(Span::styled("calls", p.head)));
        for call in calls {
            lines.push(call_line(view, call));
        }
        lines.push(Line::default());
    }

    if marked {
        let mut keys = Vec::new();
        keys.extend(hint("↑↓", "pick"));
        keys.extend(hint("c", "call"));
        keys.extend(hint("1-4", "flip a pick"));
        if call_to(view.board, view.me).is_some() {
            keys.extend(hint("a", "take the call"));
            keys.extend(hint("x", "not tonight"));
        } else if call_from(view.board, view.me).is_some() {
            keys.extend(hint("x", "take it back"));
        }
        lines.push(Line::from(keys));
    }
    lines
}

/// One open call, as this runner reads it.
fn call_line(view: &RingView<'_>, call: &Bout) -> Line<'static> {
    let p = palette();
    let left = clock((call.expires_at - view.now).num_seconds());
    let (text, style) = match (
        call.caller_user_id == view.me,
        call.called_user_id == view.me,
    ) {
        (_, true) => (
            format!("  {} calls you. {left} to answer.", call.caller_username),
            lit(Neon::Magenta),
        ),
        (true, false) => (
            format!("  you called {}. {left} left.", call.called_username),
            p.text,
        ),
        (false, false) => (
            format!(
                "  {} calls {}. {left} left.",
                call.caller_username, call.called_username
            ),
            p.dim,
        ),
    };
    Line::from(Span::styled(text, style))
}

/// A corner's name as the panel heads it: yours says so.
fn corner_name(view: &RingView<'_>, bout: &Bout, side: Side) -> String {
    match bout.user(side) == view.me {
        true => format!("{} (you)", bout.username(side)),
        false => bout.username(side).to_string(),
    }
}

/// The deck a corner brought, the base ten left unsaid: what it drafted
/// and inked.
fn deck_spans(bout: &Bout, side: Side) -> Vec<Span<'static>> {
    let p = palette();
    let deck = match side {
        Side::Caller => &bout.caller_deck,
        Side::Called => &bout.called_deck,
    };
    let cards: Vec<Card> = deck
        .as_ref()
        .and_then(|deck| Vec::<Card>::deserialize(deck).ok())
        .unwrap_or_default();
    let mut spans = Vec::new();
    for card in cards {
        let drafted = DRAFTS.iter().any(|draft| draft.options.contains(&card));
        if drafted {
            spans.push(card_mark(card));
            spans.push(Span::styled(" ", p.text));
        }
    }
    spans
}

/// The two corners' header rows: name, record, and what they drafted.
fn corner_lines(view: &RingView<'_>, live: &LiveBout) -> Vec<Line<'static>> {
    let p = palette();
    SIDES
        .iter()
        .map(|side| {
            let mut spans = vec![
                Span::styled(
                    format!("  {:<24}", corner_name(view, &live.bout, *side)),
                    p.head,
                ),
                Span::styled(
                    format!("{:<18}", record(view, live.bout.user(*side))),
                    p.dim,
                ),
            ];
            spans.extend(deck_spans(&live.bout, *side));
            Line::from(spans)
        })
        .collect()
}

/// The pool, corner by corner.
fn pool_line(live: &LiveBout) -> Line<'static> {
    let p = palette();
    Line::from(vec![
        Span::styled("  the pool   ", p.dim),
        Span::styled(live.bout.pool_caller.to_string(), p.number),
        Span::styled(format!(" on {}   ", live.bout.caller_username), p.text),
        Span::styled(live.bout.pool_called.to_string(), p.number),
        Span::styled(format!(" on {}", live.bout.called_username), p.text),
    ])
}

/// A bout is set: the corners, the pool, the bell, the bet.
fn set_lines(view: &RingView<'_>, live: &LiveBout, bell_in: i64) -> Vec<Line<'static>> {
    let p = palette();
    let mut lines = vec![Line::from(vec![
        Span::styled("the bell in ", p.text),
        Span::styled(clock(bell_in), p.number),
        Span::styled(
            format!(". the book is open until it rings. {ROUNDS} rounds at most."),
            p.text,
        ),
    ])];
    lines.push(Line::default());
    lines.extend(corner_lines(view, live));
    lines.push(Line::default());
    lines.push(pool_line(live));

    let fighter = live.bout.side_of(view.me);
    let side = fighter.unwrap_or(view.corner);
    let bet = bet_of(live, view.me);
    match bet {
        Some(bet) => lines.push(Line::from(vec![
            Span::styled("  your bet   ", p.dim),
            Span::styled(bet.bits.to_string(), p.number),
            Span::styled(
                format!(
                    " on {}, {} of {MAX_PRESSES} presses",
                    live.bout.username(bet.side),
                    bet.presses
                ),
                p.text,
            ),
        ])),
        None => lines.push(Line::from(Span::styled("  your bet   nothing down", p.dim))),
    }
    lines.push(Line::from(Span::styled(
        format!(
            "  {RAKE_PERCENT}% of the pool burns, {PURSE_PERCENT}% goes to the winner, the rest is split among the bets on them. a draw, or bets on one corner only, hands every stake back."
        ),
        p.muted,
    )));
    lines.push(Line::default());

    if let Some(side) = fighter {
        lines.extend(round_lines(live, side));
        lines.push(Line::default());
    }

    let mut keys = Vec::new();
    if let Some(sheet) = view.sheet {
        let stake = RULES.cache(sheet.level);
        if fighter.is_none() {
            keys.extend(hint("←→", &format!("corner: {}", live.bout.username(side))));
        }
        keys.extend(hint(
            "b",
            &format!("put {stake} bits on {}", live.bout.username(side)),
        ));
    }
    if fighter.is_some() {
        keys.extend(fighter_hints());
    }
    lines.push(Line::from(keys));
    lines
}

fn fighter_hints() -> Vec<Span<'static>> {
    let mut keys = hint("1-5", "lay a card, or take it back");
    keys.extend(hint("e", "lock the round"));
    keys
}

/// A signal as a bar and a number.
fn signal_spans(signal: i32, block: i32) -> Vec<Span<'static>> {
    let p = palette();
    let full = (signal.max(0) as usize * BAR).div_ceil(SIGNAL as usize);
    let full = full.min(BAR);
    vec![
        Span::styled("█".repeat(full), lit(Neon::Green)),
        Span::styled("░".repeat(BAR - full), p.muted),
        Span::styled(format!(" {signal:>2}/{SIGNAL}"), p.number),
        match block {
            0 => Span::styled("            ", p.text),
            block => Span::styled(format!("   block {block:<3}"), lit(Neon::Cyan)),
        },
    ]
}

/// Both corners as the fight stands: signal, block, and whether the
/// round is locked.
fn standing_lines(view: &RingView<'_>, live: &LiveBout) -> Vec<Line<'static>> {
    let p = palette();
    SIDES
        .iter()
        .map(|side| {
            let corner = live.duel.corner(*side);
            let mut spans = vec![Span::styled(
                format!("  {:<24}", corner_name(view, &live.bout, *side)),
                p.head,
            )];
            spans.extend(signal_spans(corner.signal, corner.block));
            if live.duel.ending.is_none() {
                spans.push(match corner.locked {
                    true => Span::styled("locked", lit(Neon::Green)),
                    false => Span::styled("laying...", p.dim),
                });
            }
            Line::from(spans)
        })
        .collect()
}

/// A corner's hand, open to everybody. For the viewer's own corner the
/// cards carry their keys and the laid ones the slot they sit in.
fn hand_line(live: &LiveBout, side: Side, own: bool) -> Line<'static> {
    let p = palette();
    let corner = live.duel.corner(side);
    let mut spans = vec![Span::styled(
        format!("  {:<12}", live.bout.username(side)),
        p.dim,
    )];
    let mut hand = 0u8;
    while let Some(card) = corner.piles.card(usize::from(hand)) {
        let laid = corner
            .slots
            .iter()
            .position(|slot| *slot == Some(hand))
            .filter(|_| own);
        if own {
            spans.push(Span::styled(format!("[{}] ", hand + 1), p.key));
        }
        match laid {
            Some(slot) => spans.push(Span::styled(
                format!("{} in slot {}   ", card.name(), slot + 1),
                p.muted,
            )),
            None => {
                spans.push(Span::styled(card.marked(), card_style(card)));
                spans.push(Span::styled(format!(" {}   ", card.cost()), p.dim));
            }
        }
        hand += 1;
    }
    Line::from(spans)
}

/// A fighter's own round: the hand with its keys, the slots as laid, the
/// energy left.
fn round_lines(live: &LiveBout, side: Side) -> Vec<Line<'static>> {
    let p = palette();
    let corner = live.duel.corner(side);
    let mut slots = vec![Span::styled("  your slots  ", p.dim)];
    for slot in 0..SLOTS {
        let card = corner.slots[slot].and_then(|hand| corner.piles.card(usize::from(hand)));
        slots.push(Span::styled(format!("{}:", slot + 1), p.muted));
        match card {
            Some(card) => slots.push(card_mark(card)),
            None => slots.push(Span::styled(" · ", p.muted)),
        }
        slots.push(Span::styled("   ", p.text));
    }
    slots.push(Span::styled(
        format!("energy {} of {ENERGY}", live.duel.energy_left(side)),
        p.number,
    ));
    if corner.locked {
        slots.push(Span::styled("   locked", lit(Neon::Green)));
    }
    vec![hand_line(live, side, true), Line::from(slots)]
}

/// What one corner did in a beat, padded to its column.
fn move_spans(mv: Move) -> Vec<Span<'static>> {
    let p = palette();
    let (card, note) = match mv {
        Move::Empty => {
            return vec![Span::styled(format!("{:<TAPE_COLUMN$}", "held"), p.muted)];
        }
        Move::Played(card) => (card, ""),
        Move::Muted(card) => (card, " muted"),
        Move::Fizzled(card) => (card, " fizzled"),
    };
    let width = card.marked().chars().count() + note.chars().count();
    vec![
        card_mark(card),
        Span::styled(note, p.muted),
        Span::styled(" ".repeat(TAPE_COLUMN.saturating_sub(width)), p.text),
    ]
}

/// The round that last came out, slot by slot: what each corner played
/// and what got through.
fn tape_lines(live: &LiveBout) -> Vec<Line<'static>> {
    let p = palette();
    let Some(beats) = live.duel.tape.last() else {
        return Vec::new();
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!(
                "{:<TAPE_LABEL$}",
                format!("round {} came out", live.duel.tape.len())
            ),
            p.head,
        ),
        Span::styled(
            format!(
                "{:<TAPE_COLUMN$}{:<TAPE_COLUMN$}",
                live.bout.caller_username, live.bout.called_username
            ),
            p.dim,
        ),
    ])];
    for (slot, beat) in beats.iter().enumerate() {
        let mut spans = vec![Span::styled(
            format!("{:<TAPE_LABEL$}", format!("  slot {}", slot + 1)),
            p.muted,
        )];
        spans.extend(move_spans(beat[0].mv));
        spans.extend(move_spans(beat[1].mv));
        let took: Vec<String> = SIDES
            .iter()
            .zip(beat)
            .filter(|(_, half)| half.took > 0)
            .map(|(side, half)| format!("{} takes {}", live.bout.username(*side), half.took))
            .collect();
        spans.push(Span::styled(took.join(", "), lit(Neon::Red)));
        lines.push(Line::from(spans));
    }
    lines
}

/// The rounds are on.
fn on_lines(view: &RingView<'_>, live: &LiveBout, left: Option<i64>) -> Vec<Line<'static>> {
    let p = palette();
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("round {} of {ROUNDS}", live.duel.round), p.head),
        Span::styled("      the clock ", p.dim),
        match left {
            Some(left) => Span::styled(clock(left), p.number),
            None => Span::styled("is up", lit(Neon::Red)),
        },
    ])];
    lines.push(Line::default());
    lines.extend(standing_lines(view, live));
    lines.push(Line::default());

    let fighter = live.bout.side_of(view.me);
    lines.push(Line::from(Span::styled(
        "the hands, open; the slots, not",
        p.head,
    )));
    for side in SIDES {
        match fighter == Some(side) {
            true => lines.extend(round_lines(live, side)),
            false => lines.push(hand_line(live, side, false)),
        }
    }
    // The keys sit under the hands, not at the foot: a screen too short
    // for the panel cuts it there, and a round on the clock can lose the
    // tape and the pool but never its keys.
    if fighter.is_some() {
        lines.push(Line::from(fighter_hints()));
    }
    lines.push(Line::default());
    lines.extend(tape_lines(live));
    lines.push(Line::default());
    lines.push(pool_line(live));
    lines
}

/// How the bout ended, in the panel's own words.
fn ending_text(live: &LiveBout, ending: Ending) -> String {
    let name = |side: Side| live.bout.username(side);
    let duel = &live.duel;
    match ending {
        Ending::Knockout(winner) => format!(
            "{} put {} down in round {}.",
            name(winner),
            name(winner.other()),
            duel.round
        ),
        Ending::Points(winner) => format!(
            "{} took it on points, {} to {}.",
            name(winner),
            duel.corner(winner).signal,
            duel.corner(winner.other()).signal
        ),
        Ending::Draw => "a draw. nobody takes it.".to_string(),
    }
}

/// It is over: the result, the split, this runner's pay.
fn over_lines(view: &RingView<'_>, live: &LiveBout, clears_in: i64) -> Vec<Line<'static>> {
    let p = palette();
    let Some(ending) = live.duel.ending else {
        return Vec::new();
    };
    let mut lines = vec![Line::from(Span::styled(
        ending_text(live, ending),
        lit(Neon::Magenta).add_modifier(Modifier::BOLD),
    ))];
    lines.push(Line::default());
    lines.extend(standing_lines(view, live));
    lines.push(Line::default());
    lines.extend(tape_lines(live));
    lines.push(Line::default());

    let pool = Pool {
        caller: live.bout.pool_caller,
        called: live.bout.pool_called,
    };
    let settlement = state::settle(pool, ending);
    let total = pool.total();
    lines.push(match (total, settlement) {
        (0, _) => Line::from(Span::styled("  nobody bet on it.", p.dim)),
        (_, Settlement::Refund) => Line::from(Span::styled(
            format!("  the pool was {total} bits. every stake goes back."),
            p.text,
        )),
        (
            _,
            Settlement::Paid {
                winner,
                purse,
                split,
            },
        ) => Line::from(Span::styled(
            format!(
                "  the pool was {total} bits: {} burned, {purse} to {}, {split} to the bets on them.",
                total - purse - split,
                live.bout.username(winner),
            ),
            p.text,
        )),
    });
    if let Some(bet) = bet_of(live, view.me) {
        let paid = state::pay(pool, settlement, bet.side, bet.bits);
        lines.push(Line::from(vec![
            Span::styled(
                format!("  your {} on {} ", bet.bits, live.bout.username(bet.side)),
                p.text,
            ),
            match (paid, settlement) {
                (_, Settlement::Refund) => Span::styled("comes back.", p.text),
                (0, Settlement::Paid { .. }) => Span::styled("is gone.", lit(Neon::Red)),
                (paid, Settlement::Paid { .. }) => {
                    Span::styled(format!("pays {paid}."), lit(Neon::Green))
                }
            },
        ]));
    }
    if live.bout.side_of(view.me).is_some() {
        lines.push(Line::from(Span::styled(
            "  you fought. the ring is done with you: the road is open.",
            lit(Neon::Green),
        )));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        format!("the ring clears in {}.", clock(clears_in)),
        p.dim,
    )));
    lines
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
