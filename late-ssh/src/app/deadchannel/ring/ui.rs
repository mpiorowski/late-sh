//! The ring's panel, in the city's palette, and the notice the street
//! shows while something at the ring concerns this runner. With nobody
//! in the ring: the deck this runner brings (each pick with what it does
//! here), the marked runners on the wire with their records and the
//! cursor on one, and the calls waiting. With a bout set: both corners
//! and what they drafted, a fighter's round 1 (the hand as cards, the
//! slots in the order they come out, the other hand), the pool, this
//! runner's bet and the split. With the rounds on: both signals, the
//! hands (open to everybody), a fighter's own slots (nobody else's), the
//! round that last came out told slot by slot (what each card threw,
//! what the block ate, both signals after), and the clock. After: the
//! result, the pool's split and what this runner was paid. Every phase
//! but that one closes with how the game goes: the round while a bout is
//! set or on, the whole bout at an empty ring, where a call's minutes
//! are the time to read it. Pure: a function of the session's
//! copy and the clock. The city's `draw_panel` frames the panel and puts
//! the ring's last word at its foot; its `draw_ring_notice` frames the
//! notice.
//!
//! No row is wider than the panel's 94 cells, so nothing wraps: a row
//! that wraps lands at the frame's edge and reads as a new row. The rows
//! that are columns (the hand's cards, the slots, the other hand, the
//! tape) and how the game goes fit 74 cells, the inside of the panel on
//! an 80 column terminal.

use chrono::{DateTime, Utc};
use late_core::models::deadchannel_bout::{Bout, Side};
use late_core::models::presence::PresenceRecord;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use serde::Deserialize;
use uuid::Uuid;

use super::data::{
    self, BELL_SECS, MAX_PRESSES, PURSE_PERCENT, RAKE_PERCENT, ROUND_SECS, ROUNDS, SIGNAL, SLOTS,
};
use super::session::{Contender, Phase, RingSession, bet_of, call_from, call_to, phase, roster};
use super::state::{self, Ending, Half, Move, Picks, Pool, SIDES, Settlement};
use super::svc::{Board, LiveBout};
use crate::app::deadchannel::city::map::Neon;
use crate::app::deadchannel::city::ui::{
    INK, INK_BRIGHT, INK_DIM, INK_MUTED, action, dim, glow, ink, lit,
};
use crate::app::deadchannel::fight::cards::{Card, DRAFTS, ENERGY, Effect, HAND};
use crate::app::deadchannel::fight::data::RULES;
use crate::app::deadchannel::fight::state::Sheet;
use crate::app::deadchannel::fight::ui::{card_mark, card_marked, card_neon};
use crate::app::deadchannel::runner::svc::RunnerLooks;

/// Cells in a signal bar.
const BAR: usize = 20;
/// The width a corner's name is cut or padded to beside its signal.
const NAME: usize = 20;
/// The width of one card of the hand, the road's own (`fight/ui.rs`):
/// five side by side, a cell between them and the indent, are 71 cells.
/// On an 80 column terminal the panel is 74 inside (the app's frame takes
/// two cells, the panel's margin two and its own frame two), and the
/// widths below are cut so the rows that are columns fit that.
const CARD: usize = 13;
/// The width of the label ahead of a row of the other hand or the slots.
/// The widest hand there is, five cards and their numbers a cell apart,
/// lands on 74 cells behind it.
const LABEL: usize = 10;
/// The width of a slot's number in the tape, and of what each corner did
/// in it: the two columns and both signals after them are 74 cells.
const TAPE_SLOT: usize = 5;
const TAPE_COLUMN: usize = 31;
/// The width of a laid card's name in the row of slots.
const SLOT_CELL: usize = 12;
/// The width of a pick and the card it was taken over, ahead of what the
/// pick does.
const PICK: usize = 24;

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

/// A row of keys, each `[k] verb` the way every counter writes one,
/// `indent` cells in. Nothing trails the last verb: a row that ends in
/// blanks past the frame wraps into an empty one.
fn key_row(indent: &str, hints: &[(&str, &str)]) -> Line<'static> {
    let p = palette();
    let mut spans = vec![Span::styled(indent.to_string(), p.text)];
    for (index, (key, verb)) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("   ", p.text));
        }
        spans.push(Span::styled(format!("[{key}] "), p.key));
        spans.push(Span::styled(verb.to_string(), action()));
    }
    Line::from(spans)
}

/// Seconds as `m:ss`.
fn clock(secs: i64) -> String {
    let secs = secs.max(0);
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// The panel's rows. `short_by` is how many the screen is short of all
/// of them (`city/ui.rs::draw_panel`): the panel then gives up how the
/// game goes, whole, since half an explanation is worse than none and
/// the guide has all of it. An empty ring still short with that gone
/// folds the deck to one row, so a call and its keys stay on the screen;
/// what is still too tall is cut at its foot.
pub(crate) fn panel_lines(view: &RingView<'_>, short_by: usize) -> Vec<Line<'static>> {
    let (mut lines, rules) = match phase(view.board.live.as_ref(), view.now) {
        Phase::Empty => {
            let rules = rules_lines(true);
            (empty_lines(view, short_by > rules.len()), rules)
        }
        Phase::Set { live, bell_in } => (set_lines(view, live, bell_in), rules_lines(false)),
        Phase::On { live, clock } => (on_lines(view, live, clock), rules_lines(false)),
        Phase::Over { live, clears_in } => return over_lines(view, live, clears_in),
    };
    if short_by == 0 {
        lines.extend(rules);
    }
    lines
}

/// A runner's record on the board.
fn record(view: &RingView<'_>, user_id: Uuid) -> String {
    match view.board.records.get(&user_id) {
        Some((won, lost)) => format!("{won} won, {lost} lost"),
        None => "no bouts yet".to_string(),
    }
}

/// `text` cut to `width` cells at most, the cut marked: a name is up to
/// 32 characters, and a row that names two of them in full is wider than
/// the panel.
fn cut(text: &str, width: usize) -> String {
    match text.chars().count() > width {
        true => {
            let mut cut: String = text.chars().take(width.saturating_sub(1)).collect();
            cut.push('…');
            cut
        }
        false => text.to_string(),
    }
}

/// `text` cut or padded to exactly `width` cells, so a long name never
/// pushes the column after it.
fn fit(text: &str, width: usize) -> String {
    format!("{:<width$}", cut(text, width))
}

/// A corner's name inside a sentence.
fn said(bout: &Bout, side: Side) -> String {
    cut(bout.username(side), NAME)
}

/// `text` as a row of the panel, two cells in, its `‹card›` marks drawn
/// as cards.
fn prose(text: &str, style: Style) -> Line<'static> {
    let mut spans = vec![Span::styled("  ", style)];
    spans.extend(card_marked(text, style));
    Line::from(spans)
}

/// What a draft card does at the ring, where the other side of the table
/// is a corner and not a glyph: the road's own line (`Card::rule`) for
/// every card but the two that name the glyph.
fn ring_rule(card: Card) -> &'static str {
    match card {
        Card::Sever => "a strike. hits for 2.5 strikes once they are at half",
        Card::Mute => "two energy. cancels the card in their slot across from it",
        Card::Strike
        | Card::Block
        | Card::Surge
        | Card::Wipe
        | Card::Jab
        | Card::Brace
        | Card::Siphon
        | Card::Riposte
        | Card::Bulwark
        | Card::Burn
        | Card::Ground
        | Card::Static => card.rule(),
    }
}

/// How the card game goes, at the foot of the panel on a screen with the
/// room for it, a blank row ahead of it. With a bout set or on it is the
/// round, for whoever is
/// looking at one. At an empty ring it is the whole bout, the bell and
/// the rounds first: the minutes a call waits are when there is time to
/// read it. The example's numbers are the ring's own.
fn rules_lines(whole_bout: bool) -> Vec<Line<'static>> {
    let p = palette();
    let powers = data::powers();
    let mut rules = Vec::new();
    let head = match whole_bout {
        true => {
            rules.push(format!(
                "taking a call sets the bell {} minutes out. the street bets until then.",
                BELL_SECS / 60
            ));
            rules.push(format!(
                "{ROUNDS} rounds at most, {ROUND_SECS} seconds each. every round is a fresh hand of {HAND}."
            ));
            "how a bout goes"
        }
        false => "how a round goes",
    };
    rules.extend([
        format!(
            "each corner lays up to {SLOTS} cards for {ENERGY} energy, in the order they come out."
        ),
        "hands are open. slots stay hidden until both lock or the clock runs out.".to_string(),
        "then slot 1 meets slot 1, 2 meets 2: guards go up, then both hits land.".to_string(),
        format!(
            "a ‹block› holding {} meets a ‹strike› hitting {}: {} gets through.",
            powers.block,
            powers.strike,
            powers.strike - powers.block
        ),
        "block left over stays up. a hit that lands puts a ‹static› in your deck.".to_string(),
        format!("a signal at 0 is down. past round {ROUNDS} the higher signal takes it."),
    ]);
    let mut lines = vec![Line::default(), Line::from(Span::styled(head, p.head))];
    lines.extend(rules.iter().map(|rule| prose(rule, p.dim)));
    lines
}

/// Nobody in the ring: the deck, the wire, the calls. `folded` is the
/// deck on a screen too short for it: the four picks on one row, what
/// each does left to the guide.
fn empty_lines(view: &RingView<'_>, folded: bool) -> Vec<Line<'static>> {
    let p = palette();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let marked = view.sheet.is_some_and(|sheet| sheet.marks >= 1);
    lines.push(Line::from(Span::styled(
        "two runners, the same numbers, card against card. the street bets on it.",
        p.text,
    )));
    if !marked {
        lines.push(Line::from(Span::styled(
            "the ropes are for runners who put the Old Signal down.",
            p.dim,
        )));
        lines.push(Line::from(Span::styled(
            "you can watch here, and bet once a bout is set.",
            p.dim,
        )));
    }
    if let Some(level) = view.sheet.and_then(Sheet::ring_gate) {
        lines.push(Line::from(Span::styled(
            format!("you owe the ring a bout at level {level}."),
            lit(Neon::Red),
        )));
        lines.push(Line::from(Span::styled(
            "the road is shut until you fight one, win or lose.",
            lit(Neon::Red),
        )));
    }
    lines.push(Line::default());

    if marked {
        lines.push(Line::from(Span::styled("the deck you bring", p.head)));
        match (view.picks, folded) {
            (Some(picks), true) => {
                let mut row = vec![Span::styled(" ", p.text)];
                for (index, pick) in picks.cards().iter().enumerate() {
                    row.push(Span::styled(format!(" [{}] ", index + 1), p.key));
                    row.push(card_mark(*pick));
                }
                lines.push(Line::from(row));
            }
            (Some(picks), false) => {
                for (index, (draft, pick)) in DRAFTS.iter().zip(picks.cards()).enumerate() {
                    let other = format!(" not {}", draft.other(*pick).name());
                    let name = pick.marked().chars().count() + other.chars().count();
                    lines.push(Line::from(vec![
                        Span::styled(format!("  [{}] ", index + 1), p.key),
                        card_mark(*pick),
                        Span::styled(other, p.muted),
                        Span::styled(" ".repeat(PICK.saturating_sub(name)), p.text),
                        Span::styled(ring_rule(*pick), p.dim),
                    ]));
                }
                lines.push(prose(
                    "the ten, these four over them, and the other card of each draft you inked.",
                    p.dim,
                ));
                lines.push(prose("level and kit stay outside.", p.dim));
            }
            (None, true | false) => {
                lines.push(Line::from(Span::styled("  reading it...", p.muted)))
            }
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
                    format!("{} ", fit(&contender.username, NAME)),
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
        let mut keys = vec![("↑↓", "pick"), ("c", "call"), ("1-4", "flip a pick")];
        if call_to(view.board, view.me).is_some() {
            keys.extend([("a", "take the call"), ("x", "not tonight")]);
        } else if call_from(view.board, view.me).is_some() {
            keys.push(("x", "take it back"));
        }
        keys.push(("?", "guide"));
        lines.push(key_row("", &keys));
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

/// The two corners before the bell, two rows each: the name and the
/// record, then what they drafted and inked, on a row of its own because
/// eight cards and a long name do not fit one.
fn corner_lines(view: &RingView<'_>, live: &LiveBout) -> Vec<Line<'static>> {
    let p = palette();
    let mut lines = Vec::new();
    for side in SIDES {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", fit(&corner_name(view, &live.bout, side), NAME)),
                p.head,
            ),
            Span::styled(record(view, live.bout.user(side)), p.dim),
        ]));
        let mut deck = vec![Span::styled("    drafted  ", p.muted)];
        deck.extend(deck_spans(&live.bout, side));
        lines.push(Line::from(deck));
    }
    lines
}

/// The pool, corner by corner.
fn pool_line(live: &LiveBout) -> Line<'static> {
    let p = palette();
    Line::from(vec![
        Span::styled("  the pool   ", p.dim),
        Span::styled(live.bout.pool_caller.to_string(), p.number),
        Span::styled(format!(" on {}   ", said(&live.bout, Side::Caller)), p.text),
        Span::styled(live.bout.pool_called.to_string(), p.number),
        Span::styled(format!(" on {}", said(&live.bout, Side::Called)), p.text),
    ])
}

/// The book while it is open: the pool, this runner's bet, the key for
/// one more press, and how the pool is split.
fn book_lines(view: &RingView<'_>, live: &LiveBout) -> Vec<Line<'static>> {
    let p = palette();
    let mut lines = vec![
        Line::from(Span::styled("the book", p.head)),
        pool_line(live),
    ];
    lines.push(match bet_of(live, view.me) {
        Some(bet) => Line::from(vec![
            Span::styled("  your bet   ", p.dim),
            Span::styled(bet.bits.to_string(), p.number),
            Span::styled(
                format!(
                    " on {}, {} of {MAX_PRESSES} presses",
                    said(&live.bout, bet.side),
                    bet.presses
                ),
                p.text,
            ),
        ]),
        None => Line::from(Span::styled("  your bet   nothing down", p.dim)),
    });
    if let Some(sheet) = view.sheet {
        let fighter = live.bout.side_of(view.me);
        let side = fighter.unwrap_or(view.corner);
        let stake = RULES.cache(sheet.level);
        let corner = format!("corner: {}", said(&live.bout, side));
        let press = format!("put {stake} bits on {}", said(&live.bout, side));
        lines.push(match fighter {
            Some(_) => key_row("  ", &[("b", &press)]),
            None => key_row("  ", &[("←→", &corner), ("b", &press)]),
        });
    }
    lines.push(prose(
        &format!(
            "bets on the winner split the pool, less {RAKE_PERCENT}% burned and {PURSE_PERCENT}% to the winner."
        ),
        p.muted,
    ));
    lines.push(prose(
        "a draw, or bets on one corner only, hands every stake back.",
        p.muted,
    ));
    lines
}

/// A bout is set: the bell, the corners, a fighter's round 1, the book.
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
    // A fighter's round sits over the book: a screen too short for the
    // panel cuts it at its foot, and the keys of a round on the clock are
    // the last thing to lose.
    if let Some(side) = live.bout.side_of(view.me) {
        lines.extend(round_lines(
            live,
            side,
            "round 1, yours to lay before the bell",
        ));
        lines.push(Line::default());
    }
    lines.extend(book_lines(view, live));
    lines
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
                format!("  {} ", fit(&corner_name(view, &live.bout, *side), NAME)),
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

/// What `card` would do played now from `side`'s corner: the numbers
/// are `Card::effect`'s on the table as it stands, so a card never
/// prints one the machine would not play.
fn effect_now(live: &LiveBout, side: Side, card: Card) -> Effect {
    let corner = live.duel.corner(side);
    card.effect(
        &data::powers(),
        &corner.board(live.duel.corner(side.other())),
    )
}

/// What a card does, in the two rows under its name: the road's words
/// (`fight/ui.rs::card_text`) for every card but the mute, which here
/// cancels the card across from it and not a glyph's move.
fn card_text(card: Card, effect: &Effect) -> [String; 2] {
    match card {
        Card::Mute => ["cancels".to_string(), "across".to_string()],
        Card::Strike
        | Card::Block
        | Card::Surge
        | Card::Wipe
        | Card::Jab
        | Card::Brace
        | Card::Siphon
        | Card::Riposte
        | Card::Bulwark
        | Card::Burn
        | Card::Ground
        | Card::Sever
        | Card::Static => crate::app::deadchannel::fight::ui::card_text(card, effect),
    }
}

/// The one number a chip of the other hand has room for: what the card
/// hits for, or holds, or the energy it hands back.
fn chip_number(effect: &Effect) -> String {
    match (effect.damage, effect.block, effect.energy) {
        (0, 0, 0) => String::new(),
        (0, 0, energy) => format!(" +{energy}"),
        (0, block, _) => format!(" {block}"),
        (damage, _, _) => format!(" {damage}"),
    }
}

/// A corner's hand as one row of chips, open to everybody: each card
/// with its number, a cell apart, under `label`.
fn hand_chips(live: &LiveBout, side: Side, label: &str) -> Line<'static> {
    let p = palette();
    let corner = live.duel.corner(side);
    let mut spans = vec![Span::styled(format!("  {}", fit(label, LABEL)), p.dim)];
    for hand in 0..HAND {
        spans.push(Span::styled(" ", p.text));
        match corner.piles.card(hand) {
            Some(card) => {
                spans.push(card_mark(card));
                spans.push(Span::styled(
                    chip_number(&effect_now(live, side, card)),
                    p.text,
                ));
            }
            None => spans.push(Span::styled("·", p.muted)),
        }
    }
    Line::from(spans)
}

/// How one card of a fighter's own hand stands in the round.
#[derive(Clone, Copy)]
enum Lay {
    /// Laid in this slot, from 0.
    Laid(usize),
    /// In the hand, and the energy left covers it.
    Open,
    /// In the hand, and it cannot be laid: the energy is short, or the
    /// round is locked.
    Shut,
}

/// A fighter's own hand as five cards side by side, each in its place:
/// the number to press in the corner, the name on the frame, what it
/// does inside, and on the bottom edge its cost as pips or, once laid,
/// the slot it sits in, the frame lit. A card the round cannot take is
/// drawn dark. Every card is the same width whatever is laid, so nothing
/// moves under the fingers.
fn hand_cards(live: &LiveBout, side: Side) -> Vec<Line<'static>> {
    let p = palette();
    let corner = live.duel.corner(side);
    let energy = live.duel.energy_left(side);
    let inside = CARD - 4;
    let mut rows: [Vec<Span<'static>>; 4] =
        std::array::from_fn(|_| vec![Span::styled(" ", p.text)]);
    for hand in 0..HAND {
        for row in &mut rows {
            row.push(Span::styled(" ", p.text));
        }
        let Some(card) = corner.piles.card(hand) else {
            let gap = " ".repeat(CARD - 2);
            rows[0].push(Span::styled(format!("╭{gap}╮"), p.muted));
            rows[1].push(Span::styled(" ".repeat(CARD), p.text));
            rows[2].push(Span::styled(" ".repeat(CARD), p.text));
            rows[3].push(Span::styled(format!("╰{gap}╯"), p.muted));
            continue;
        };
        let laid = corner
            .slots
            .iter()
            .position(|slot| slot.map(usize::from) == Some(hand));
        let lay = match (laid, corner.locked || card.cost() > energy) {
            (Some(slot), _) => Lay::Laid(slot),
            (None, false) => Lay::Open,
            (None, true) => Lay::Shut,
        };
        let (frame, name, body, key) = match (lay, card_neon(card)) {
            (Lay::Laid(_), Some(neon)) => (
                lit(Neon::Amber),
                lit(neon),
                ink(INK_BRIGHT),
                lit(Neon::Amber),
            ),
            (Lay::Laid(_), None) => (lit(Neon::Amber), p.dim, p.dim, lit(Neon::Amber)),
            (Lay::Open, Some(neon)) => (dim(neon), lit(neon), ink(INK_BRIGHT), lit(Neon::Amber)),
            (Lay::Open, None) => (p.dim, p.dim, p.muted, lit(Neon::Amber)),
            (Lay::Shut, _) => (p.muted, p.muted, p.muted, p.muted),
        };
        let label = card.name();
        rows[0].extend([
            Span::styled("╭ ", frame),
            Span::styled((hand + 1).to_string(), key),
            Span::styled(" ", frame),
            Span::styled(label, name),
            Span::styled(format!(" {}╮", "─".repeat(CARD - 6 - label.len())), frame),
        ]);
        let [first, second] = card_text(card, &effect_now(live, side, card));
        for (row, line) in [(1, first), (2, second)] {
            rows[row].extend([
                Span::styled("│ ", frame),
                Span::styled(format!("{line:<inside$}"), body),
                Span::styled(" │", frame),
            ]);
        }
        let foot = match (lay, card.cost()) {
            (Lay::Laid(slot), _) => format!("slot {}", slot + 1),
            (Lay::Open | Lay::Shut, 0) => "free".to_string(),
            (Lay::Open | Lay::Shut, cost) => vec!["█"; usize::from(cost)].join(" "),
        };
        let foot_style = match lay {
            Lay::Laid(_) => lit(Neon::Amber),
            Lay::Open => glow(Neon::Amber),
            Lay::Shut => p.muted,
        };
        let edge = "─".repeat(CARD - 4 - foot.chars().count());
        rows[3].extend([
            Span::styled("╰ ", frame),
            Span::styled(foot, foot_style),
            Span::styled(format!(" {edge}╯"), frame),
        ]);
    }
    rows.into_iter().map(Line::from).collect()
}

/// The round's energy as pips, the way the road draws a turn's: spent
/// energy is a different shape, not only a darker one. A burn carries
/// the round past its three, and the row grows to show it.
fn energy_spans(left: u8) -> Vec<Span<'static>> {
    let p = palette();
    let mut spans = vec![Span::styled("energy ", p.dim)];
    for pip in 0..ENERGY.max(left) {
        spans.push(match pip < left {
            true => Span::styled("██", lit(Neon::Amber)),
            false => Span::styled("░░", p.muted),
        });
        spans.push(Span::styled(" ", p.text));
    }
    spans
}

/// A fighter's own round under `head`: the energy left, the hand as
/// cards, the slots in the order they come out, the other corner's open
/// hand, and the keys, or what a locked round waits for. The same rows
/// whatever is laid.
fn round_lines(live: &LiveBout, side: Side, head: &str) -> Vec<Line<'static>> {
    let p = palette();
    let corner = live.duel.corner(side);
    let mut top = vec![Span::styled(format!("{head}   "), p.head)];
    top.extend(energy_spans(live.duel.energy_left(side)));
    let mut lines = vec![Line::from(top)];
    lines.extend(hand_cards(live, side));

    let mut slots = vec![Span::styled(format!("  {:<LABEL$} ", "comes out"), p.dim)];
    for slot in 0..SLOTS {
        let card = corner.slots[slot].and_then(|hand| corner.piles.card(usize::from(hand)));
        slots.push(Span::styled(format!("{} ", slot + 1), p.key));
        match card {
            Some(card) => {
                let pad = SLOT_CELL.saturating_sub(card.marked().chars().count());
                slots.push(card_mark(card));
                slots.push(Span::styled(" ".repeat(pad), p.text));
            }
            None => slots.push(Span::styled(format!("{:<SLOT_CELL$}", "·"), p.muted)),
        }
    }
    lines.push(Line::from(slots));
    lines.push(hand_chips(live, side.other(), "their hand"));
    lines.push(match corner.locked {
        true => Line::from(Span::styled(
            "  locked. it comes out when both corners are, or when the clock runs out.",
            lit(Neon::Green),
        )),
        false => key_row(
            "  ",
            &[
                ("1-5", "lay a card, or take it back"),
                ("e", "lock the round"),
                ("?", "guide"),
            ],
        ),
    });
    lines
}

/// What one corner's slot came to, in words: the card, what it put up,
/// what it threw and how much of that the other block ate, what it
/// mended. `before` is the corner's signal and block going into the
/// beat. The block put up is read off the tape: what stands after, less
/// what stood before, plus what the other's hit ate of it. The tape cuts
/// a deed longer than its column, which only one that hits into a block
/// and mends is.
fn deed(half: Half, other: Half, (signal_before, block_before): (i32, i32)) -> String {
    let card = match half.mv {
        Move::Empty => return "nothing laid".to_string(),
        Move::Fizzled(card) => return format!("{} fizzled, no energy", card.marked()),
        Move::Muted(card) => return format!("{} cancelled", card.marked()),
        Move::Played(card) => card,
    };
    let mut did: Vec<String> = Vec::new();
    let eaten = other.hit - half.took;
    let held = half.block - block_before + eaten;
    if held > 0 {
        did.push(format!("holds {held}"));
    }
    if half.hit > 0 {
        did.push(match (other.took, half.hit - other.took) {
            (_, 0) => format!("hits {}", half.hit),
            (0, _) => format!("hits {}, blocked", half.hit),
            (_, blocked) => format!("hits {}, {blocked} blocked", half.hit),
        });
    }
    let mended = half.signal - (signal_before - half.took).max(0);
    if mended > 0 {
        did.push(format!("mends {mended}"));
    }
    match card {
        Card::Mute => did.push(match other.mv {
            Move::Muted(theirs) => format!("cancels {}", theirs.marked()),
            Move::Empty | Move::Fizzled(_) | Move::Played(_) => "cancels nothing".to_string(),
        }),
        Card::Burn => did.push("for energy".to_string()),
        Card::Static => did.push("thrown out".to_string()),
        Card::Strike
        | Card::Block
        | Card::Surge
        | Card::Wipe
        | Card::Jab
        | Card::Brace
        | Card::Siphon
        | Card::Riposte
        | Card::Bulwark
        | Card::Ground
        | Card::Sever => {}
    }
    format!("{} {}", card.marked(), did.join(", "))
}

/// A signal after a beat, colored by where the beat moved it: red down,
/// green up, grey where it stood.
fn signal_span(after: i32, before: i32) -> Span<'static> {
    let style = match after.cmp(&before) {
        std::cmp::Ordering::Less => lit(Neon::Red),
        std::cmp::Ordering::Greater => lit(Neon::Green),
        std::cmp::Ordering::Equal => palette().dim,
    };
    Span::styled(format!("{after:>2}"), style)
}

/// The round that last came out, slot by slot: what each corner's card
/// did and both signals once the slot was over, so the bars above read
/// as the sum of the rows.
fn tape_lines(view: &RingView<'_>, live: &LiveBout) -> Vec<Line<'static>> {
    let p = palette();
    let tape = &live.duel.tape;
    let Some(beats) = tape.last() else {
        return Vec::new();
    };
    // Where both corners stood going into the round: the last beat of
    // the round before it, or the opening bell.
    let earlier = tape
        .len()
        .checked_sub(2)
        .and_then(|round| tape[round].last());
    let mut before = match earlier {
        Some(beat) => beat.map(|half| (half.signal, half.block)),
        None => [(SIGNAL, 0); 2],
    };
    let mut lines = vec![
        Line::from(Span::styled(
            format!("round {} came out, slot by slot", tape.len()),
            p.head,
        )),
        Line::from(vec![
            Span::styled(" ".repeat(TAPE_SLOT), p.text),
            Span::styled(
                SIDES
                    .map(|side| {
                        format!(
                            "{}  ",
                            fit(&corner_name(view, &live.bout, side), TAPE_COLUMN - 2)
                        )
                    })
                    .concat(),
                p.dim,
            ),
            Span::styled("signal", p.dim),
        ]),
    ];
    for (slot, beat) in beats.iter().enumerate() {
        let mut spans = vec![Span::styled(
            format!("{:<TAPE_SLOT$}", format!("  {}", slot + 1)),
            p.key,
        )];
        for seat in 0..2 {
            let half = beat[seat];
            let text = cut(&deed(half, beat[1 - seat], before[seat]), TAPE_COLUMN - 2);
            let style = match half.mv {
                Move::Played(_) => p.text,
                Move::Empty | Move::Fizzled(_) | Move::Muted(_) => p.muted,
            };
            let pad = TAPE_COLUMN - text.chars().count();
            spans.extend(card_marked(&text, style));
            spans.push(Span::styled(" ".repeat(pad), p.text));
        }
        spans.push(signal_span(beat[0].signal, before[0].0));
        spans.push(Span::styled(" · ", p.muted));
        spans.push(signal_span(beat[1].signal, before[1].0));
        lines.push(Line::from(spans));
        before = beat.map(|half| (half.signal, half.block));
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

    // A fighter's hand and its keys sit over the tape and the pool: a
    // screen too short for the panel cuts it at its foot, and a round on
    // the clock can lose the tape but never its keys.
    match live.bout.side_of(view.me) {
        Some(side) => lines.extend(round_lines(live, side, "your round")),
        None => {
            lines.push(Line::from(Span::styled(
                "the hands, open to everybody: a number is what the card hits for, or holds",
                p.head,
            )));
            for side in SIDES {
                lines.push(hand_chips(live, side, live.bout.username(side)));
            }
        }
    }
    lines.push(Line::default());
    let tape = tape_lines(view, live);
    if !tape.is_empty() {
        lines.extend(tape);
        lines.push(Line::default());
    }
    lines.push(pool_line(live));
    lines
}

/// How the bout ended, in the panel's own words.
fn ending_text(live: &LiveBout, ending: Ending) -> String {
    let name = |side: Side| said(&live.bout, side);
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
    lines.extend(tape_lines(view, live));
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
                said(&live.bout, winner),
            ),
            p.text,
        )),
    });
    if let Some(bet) = bet_of(live, view.me) {
        let paid = state::pay(pool, settlement, bet.side, bet.bits);
        lines.push(Line::from(vec![
            Span::styled(
                format!("  your {} on {} ", bet.bits, said(&live.bout, bet.side)),
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

/// What the street says of the ring to a runner who is not at it.
pub(crate) struct Notice {
    pub told: Line<'static>,
    /// What stepping in is for, the verb beside the city's key.
    pub verb: &'static str,
}

/// The ring's news for the street: a call to this runner or their own
/// call out, a bout set or on (theirs said as theirs), the result while
/// it is still on the board. `None` when the ring holds nothing to tell
/// them. Other runners' calls are not news: nothing is happening yet.
pub(crate) fn notice(view: &RingView<'_>) -> Option<Notice> {
    let p = palette();
    let urgent = lit(Neon::Magenta);
    let board = view.board;
    let names = |live: &LiveBout| {
        format!(
            "{} against {}",
            said(&live.bout, Side::Caller),
            said(&live.bout, Side::Called)
        )
    };
    let (text, style, verb) = match phase(board.live.as_ref(), view.now) {
        Phase::Empty => match (call_to(board, view.me), call_from(board, view.me)) {
            (Some(call), _) => (
                format!(
                    "{} calls you to the ring. {} to answer.",
                    call.caller_username,
                    clock((call.expires_at - view.now).num_seconds())
                ),
                urgent,
                "answer",
            ),
            (None, Some(call)) => (
                format!(
                    "you called {}. {} left.",
                    call.called_username,
                    clock((call.expires_at - view.now).num_seconds())
                ),
                p.text,
                "step in",
            ),
            (None, None) => return None,
        },
        Phase::Set { live, bell_in } => match live.bout.side_of(view.me) {
            Some(side) => (
                format!(
                    "your bout with {}. the bell in {}.",
                    said(&live.bout, side.other()),
                    clock(bell_in)
                ),
                urgent,
                "step in",
            ),
            None => (
                format!(
                    "{}. the bell in {}, the book is open.",
                    names(live),
                    clock(bell_in)
                ),
                p.text,
                "watch, and bet",
            ),
        },
        Phase::On { live, clock: left } => {
            let round = format!("round {} of {ROUNDS}", live.duel.round);
            match (live.bout.side_of(view.me), left) {
                (Some(_), Some(left)) => (
                    format!("your bout is on. {round}, {} on the clock.", clock(left)),
                    urgent,
                    "step in",
                ),
                (Some(_), None) => (
                    format!("your bout is on. {round}, the clock is up."),
                    urgent,
                    "step in",
                ),
                (None, _) => (format!("{}, {round}.", names(live)), p.text, "watch"),
            }
        }
        Phase::Over { live, .. } => (ending_text(live, live.duel.ending?), p.text, "see it"),
    };
    Some(Notice {
        told: Line::from(Span::styled(text, style)),
        verb,
    })
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
