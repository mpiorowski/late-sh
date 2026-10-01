//! Full-screen daily pool board: the table on the left, the info panel and the
//! cue panel stacked on the right, key hints on the floor.
//!
//! ## Why the split
//!
//! A 2.25 inch ball on a 7 foot table is a thirty-fifth of its length, so at
//! any terminal width the table view is an *overview* — you can read the
//! layout off it, but you cannot aim on it. The cue panel is the other half of
//! that trade: it shows the target ball and the cue ball large, from the
//! shooter's eye, and a fraction of a degree of aim moves the sighting mark
//! there while moving nothing at all on the table.
//!
//! Both are drawn by `pool_core` (`table_ui` and `cue_ui`) into a half-block
//! `Canvas`, so this file is layout, chrome and wording only. Nothing here
//! knows any physics.
//!
//! ## Guiding the player
//!
//! Nothing about a shot is sequenced: target, spin, aim and stroke are all
//! live at once and the player may strike at any moment. What changes is which
//! of them the *pointer* is steering, and the hint row is rewritten from
//! `ShotMode::hint` to say so, on the status line beside the mode's name.
//! That string lives in `pool_core` beside the mode enum rather than here, so
//! the controls and the description of them cannot drift apart.
//!
//! The keys live in a legend under the cue panel, in the right column, and
//! nowhere else: a hint row along the bottom used to repeat half of them,
//! and a row spent saying the same thing twice is a row the cue drawing does
//! not get. A game with this many controls should not make a player memorise
//! them, so the cue drawing is capped and the legend takes the rows below.

use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use uuid::Uuid;

use crate::app::{
    common::{primitives::draw_too_small, theme},
    games::pool_core::{
        aim::{Hit, ShotLine},
        canvas::{Canvas, rgb},
        cue::{MAX_SPEED, ShotMode},
        cue_ui::{self, BACKDROP, CueView},
        rules::PoolRules,
        rules_snooker,
        table::{self, TableSpec},
        table_3d::{self, Eye},
        table_ui::{self, BallSet, Overlay, SURROUND, View},
    },
    lobby::daily::{
        board_ui::{name_for, result_banner},
        pool::DailyPoolState,
        pool_draft::{PoolCueHit, PoolDetail, PoolDraft},
        state::{DailyBoardState, DailyMatchDetail, DailyState, format_deadline},
    },
};

/// Below this the board cannot show a table and a cue panel at once, and the
/// cue panel is what makes the game playable rather than watchable.
pub const MIN_WIDTH: u16 = 112;
pub const MIN_HEIGHT: u16 = 30;

/// The right column: info panel on top, cue panel below. This is its narrowest.
const PANEL_WIDTH: u16 = 34;
/// Widest the panel gets. Past this it stops buying precision and starts
/// taking the table's room for nothing.
const MAX_PANEL_WIDTH: u16 = 60;
/// Rows the info panel takes before the cue panel gets the rest.
const INFO_ROWS: u16 = 8;
/// Readout rows under the cue drawing (aim, what it is on, spin, power).
const READOUT_ROWS: u16 = 4;
/// The key legend under the readouts: the eight rows of `LEGEND` and one
/// more for leaving the board (chat, lobby).
const LEGEND_ROWS: u16 = LEGEND.len() as u16 + 1;
/// The cue drawing stops growing here.
///
/// The balls stop growing well before this — they take a share of the panel's
/// *width*, and past the widest panel that is the binding limit. Every row
/// beyond it goes under the cue ball, which is where the cue is, and the cue is
/// the part that moves: the draw-and-push gesture that sets the stroke speed is
/// read off how far it travels, so rows spent there are precision the player
/// can actually see. At the twenty-eight this used to be, a full pull moved the
/// cue about four terminal rows and the bottom of the column sat empty.
const MAX_CUE_ROWS: u16 = 44;
/// The legend only appears once the cue drawing keeps at least this many
/// rows: a legend that squeezed the cue into a sliver would be teaching the
/// keys for a panel that can no longer be aimed on. Low enough that the
/// smallest board still gets it, since the legend is the only place the
/// keys are taught.
const MIN_CUE_ROWS_WITH_LEGEND: u16 = 8;

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    pool: &PoolDetail,
) {
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        draw_too_small(frame, area, "The pool table", MIN_WIDTH, MIN_HEIGHT);
        return;
    }
    let Ok(spec) = pool.state.spec() else {
        // A match on a table this build no longer knows. Say so rather than
        // drawing a plausible-looking rack on the wrong equipment.
        draw_too_small(frame, area, "This table", MIN_WIDTH, MIN_HEIGHT);
        return;
    };

    let rows = Layout::vertical([
        Constraint::Length(1), // status
        Constraint::Fill(1),   // table + panels
    ])
    .split(area);
    let cols = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(panel_width(area.width)),
    ])
    .split(rows[1]);

    frame.render_widget(
        Paragraph::new(status_line(daily, board, detail, pool)).alignment(Alignment::Center),
        rows[0],
    );
    // One shot is drawn, and it is not always this player's. While the other
    // side is at the table their board broadcasts what they are lining up, and
    // drawing it is the only moment a correspondence game looks like the game
    // it is modelling. The renderer takes a *share* either way, so the local
    // and the remote path are the same code and cannot drift.
    let shown = shown_shot(daily, board, detail, pool);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(table_border(daily, board, detail, pool));
    let table_area = block.inner(cols[0]);
    frame.render_widget(block, cols[0]);
    draw_table(frame, table_area, spec, pool, &shown, board);
    draw_panels(frame, cols[1], daily, board, detail, pool, &shown);
}

/// Whether the board is mid-shot: one on the wire, one being re-simulated, or
/// one playing out. The one question four different parts of the board ask, so
/// they cannot answer it differently.
///
/// A **replay** counts. Nothing can be done with the board while one is
/// rolling, and the player asked for it, so it reads exactly like a shot.
fn rolling(board: &DailyBoardState, pool: &PoolDetail) -> bool {
    pool.is_busy(board.pool_shot_pending())
}

/// The frame around the table, which says whose board it is and whether
/// anything can be done with it.
///
/// Two questions, two channels, and the border is **always drawn** so the
/// table never changes size underneath the answer:
///
/// - **Hue** is whose turn it is: green yours, red theirs.
/// - **Brightness** is whether the board is live: bright while somebody is at
///   the table, dim while the balls are still rolling and nobody can act.
///
/// A spectator is neither player, so they get the neutral border: nothing
/// about the match is *theirs*, and colouring it as if it were would be a lie
/// they cannot act on.
fn table_border(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    pool: &PoolDetail,
) -> Style {
    let rolling = rolling(board, pool);
    let colour = if !detail.is_active() || board.spectating {
        theme::BORDER_DIM()
    } else if detail.row.turn_user_id == Some(daily.user_id()) {
        theme::SUCCESS()
    } else {
        theme::ERROR()
    };
    let style = Style::default().fg(colour);
    if rolling {
        style.add_modifier(Modifier::DIM)
    } else {
        style
    }
}

/// The overview. Records the drawn rect so the mouse hit test can turn a click
/// back into table coordinates.
/// The panel takes a share of the board rather than a fixed column count, so
/// the one part of the screen that exists to be looked at closely grows with
/// the terminal instead of staying the size it was at the minimum. The table
/// keeps the larger half at every size.
fn panel_width(width: u16) -> u16 {
    (width / 4).clamp(PANEL_WIDTH, MAX_PANEL_WIDTH)
}

/// The shot the board should be drawing: this player's own while it is their
/// turn, otherwise whatever the other side last broadcast.
///
/// Falls back to the local draft when nothing has arrived, so a board that
/// nobody is composing on still shows a sensible cue rather than an empty
/// panel. A spectator is neither player and gets the broadcast if there is one.
fn shown_shot(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    pool: &PoolDetail,
) -> PoolDraft {
    let mine =
        !board.spectating && detail.is_active() && detail.row.turn_user_id == Some(daily.user_id());
    let share = match pool.watching {
        Some(theirs) if !mine => theirs,
        _ => pool.draft.share(),
    };
    PoolDraft::watching(share)
}

fn draw_table(
    frame: &mut Frame,
    area: Rect,
    spec: &TableSpec,
    pool: &PoolDetail,
    shot: &PoolDraft,
    board: &DailyBoardState,
) {
    // The room, not the cloth: the table keeps its aspect, so the leftover of
    // whichever dimension it does not fill belongs to the floor around it.
    let mut canvas = Canvas::new(area.width, area.height, SURROUND);

    // Mid-shot the table shows the sampled timeline instead of the settled
    // rack, and the aiming marks come off: they describe a shot that has
    // already been played.
    //
    // While a shot is still being re-simulated the board shows the rack as it
    // was *before* it — the settled rack is the answer to a shot nobody has
    // watched yet, and showing it for a tick and then rewinding is what made
    // the played shot flash up before it played out.
    let (frames, aiming) = match (&pool.playback, board.pool_shot_pending()) {
        (Some(playback), _) => (playback.frame(), false),
        (None, true) => (
            pool.frames_before_shot()
                .unwrap_or_else(|| shot.frames(&pool.state)),
            false,
        ),
        (None, false) => (shot.frames(&pool.state), true),
    };
    // One set of marks for both views, so a click on either lands on the
    // same shot. The legal set is the striker's whoever is looking: what the
    // watcher sees dimmed is what the shooter may not hit.
    let marks = if aiming {
        Overlay {
            line: shot.line(&pool.state),
            legal: BallSet::from_ids(&pool.state.legal_targets()),
            called_pocket: shot.called_pocket,
        }
    } else {
        Overlay::default()
    };

    // The eye view is the same rack seen from behind the cue ball. Recorded
    // rather than recomputed by the input path, for the same reason the
    // overview is: one mapping, inverted, so a click cannot land somewhere the
    // picture did not put it.
    let eye = board
        .pool_eye
        .then(|| eye_for(pool, shot, spec, &canvas))
        .flatten();
    board.pool_eye_geometry.set(eye);
    match eye {
        Some(eye) => {
            table_3d::draw(&mut canvas, spec, &spec.geometry(), &eye, &frames, &marks);
        }
        None => {
            let view = View::fit(spec, &canvas);
            table_ui::draw(&mut canvas, spec, &spec.geometry(), &view, &frames, &marks);
        }
    }
    board.target_geometry.set(Some(area));
    frame.render_widget(Paragraph::new(canvas.to_lines()), area);
}

/// Where to stand for the eye view, or `None` when there is no cue ball to
/// stand behind.
///
/// While a shot plays out the eye stays where the *shooter* stood: behind the
/// cue ball as it was before the strike, looking down the line they hit.
/// Following the ball would be a camera that lurches around the table for the
/// whole shot, where staying put is what watching a shot actually looks like.
fn eye_for(pool: &PoolDetail, shot: &PoolDraft, spec: &TableSpec, canvas: &Canvas) -> Option<Eye> {
    // Read off the timeline being shown rather than off the last recorded
    // shot: a replay plays several shots in a row, and only the timeline knows
    // which of them is on screen right now.
    if let Some(playback) = &pool.playback {
        let (cue, azimuth) = playback.timeline.cue_launch()?;
        return Some(Eye::behind(cue, azimuth, spec, canvas));
    }
    let cue = shot.cue_ball(&pool.state)?;
    Some(Eye::behind(cue, shot.azimuth, spec, canvas))
}

/// Turn a click inside the recorded table rect into a spot on the cloth.
///
/// Inverts exactly what `draw_table` did — including *which view* it drew.
/// Cell to canvas pixel first (two pixel rows per terminal row, and the click
/// lands on the upper one), then pixel to metres through the same mapping the
/// renderer used: the overview's `View`, or the eye's ray if that is what is
/// on screen. `None` from the eye means the click was above the horizon, which
/// is the room and not the table.
pub(crate) fn table_point_at(
    spec: &TableSpec,
    area: Rect,
    eye: Option<Eye>,
    x: u16,
    y: u16,
) -> Option<[f64; 2]> {
    let px = (x.saturating_sub(area.x)) as f64 + 0.5;
    let py = (y.saturating_sub(area.y)) as f64 * 2.0 + 0.5;
    match eye {
        Some(eye) => eye.to_table(px, py),
        None => {
            let view = View::fit_area(spec, area.width, area.height.saturating_mul(2));
            Some(view.to_table((px, py)))
        }
    }
}

fn draw_panels(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    pool: &PoolDetail,
    shot: &PoolDraft,
) {
    // No frame of its own: the table's border already draws the seam, and a
    // second line beside it was a column the cue drawing did not get. One
    // column of air keeps the text off the table.
    let inner = Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(1),
        ..area
    };

    let (cue_rows, legend_rows) = column_split(inner.height);
    let rows = Layout::vertical([
        Constraint::Length(INFO_ROWS),
        Constraint::Length(cue_rows),
        Constraint::Length(legend_rows),
        Constraint::Fill(1),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(info_lines(daily, board, detail, pool)),
        rows[0],
    );
    draw_cue_panel(frame, rows[1], board, pool, shot);
    if legend_rows > 0 {
        let chat = board.shows_chat(detail);
        let keys = legend_keys(daily, board, detail, pool);
        let lines: Vec<Line<'static>> = legend_rows_for(keys, chat)
            .into_iter()
            .map(legend_row)
            .collect();
        frame.render_widget(Paragraph::new(lines), rows[2]);
    }
}

/// Which keys act on this board right now. The same gate the input layer
/// applies (`pool_draft_mut`): a key the legend teaches must be a key that
/// does something, or the legend is lying to exactly the player who is
/// reading it to learn.
fn legend_keys(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    pool: &PoolDetail,
) -> LegendKeys {
    if board.spectating || !detail.is_active() {
        return LegendKeys::Watching;
    }
    let mine = detail.row.turn_user_id == Some(daily.user_id());
    let rolling = rolling(board, pool);
    if mine && !rolling {
        LegendKeys::AtTheTable
    } else {
        LegendKeys::Waiting
    }
}

/// How the right column below the info panel is shared between the cue panel
/// (drawing plus readouts) and the key legend: rows for each.
///
/// The cue panel takes what is left after the info panel and, when there is
/// room for both, the legend; it stops growing at `MAX_CUE_ROWS`. The legend
/// is all or nothing, because half a key map teaches nothing.
pub(crate) fn column_split(height: u16) -> (u16, u16) {
    let below_info = height.saturating_sub(INFO_ROWS);
    let with_legend = below_info.saturating_sub(LEGEND_ROWS);
    if with_legend >= READOUT_ROWS + MIN_CUE_ROWS_WITH_LEGEND {
        (with_legend.min(READOUT_ROWS + MAX_CUE_ROWS), LEGEND_ROWS)
    } else {
        (below_info.min(READOUT_ROWS + MAX_CUE_ROWS), 0)
    }
}

/// Every key that plays the game, two to a row. The status line says what the
/// *mouse* is doing; this is the keyboard, all of it, so nothing has to be
/// memorised. The row for leaving the board is built beside it, because one
/// of its keys depends on the match having a chat.
pub(crate) const LEGEND: [[(&str, &str); 2]; 8] = [
    [("h l", "aim 1°"), ("[ ]", "ball")],
    [("H L", "aim 0.1°"), ("'", "lowest ball")],
    [("a", "mouse aim"), ("m", "ball in hand")],
    // The mouse's answer to `h l` against `H L`: hold a modifier while it
    // moves and the same sweep is worth four times as much, or a tenth.
    [("shift", "fast aim"), ("ctrl", "fine aim")],
    [("e", "spin"), ("p", "call pocket")],
    [("x s w", "stroke"), ("v", "eye view")],
    [("c", "reset"), ("r R", "replay")],
    [("Esc", "back"), ("X", "resign")],
];

/// The legend's exit row: the keys that leave the board rather than play on
/// it. Chat only where the match has one, or the key would teach a lie.
pub(crate) fn exit_row(chat: bool) -> [(&'static str, &'static str); 2] {
    if chat {
        [("i", "chat"), ("Q", "lobby")]
    } else {
        [("Q", "lobby"), ("", "")]
    }
}

/// Who the legend is for, which is to say which keys do anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LegendKeys {
    /// This player's shot, nothing rolling: every control.
    AtTheTable,
    /// A player waiting on the other side, or on a shot playing out: the
    /// camera and resigning are all that answer.
    Waiting,
    /// A spectator, or a finished match: the camera and the way out.
    Watching,
}

/// The legend's rows for `keys`, the exit row last.
pub(crate) fn legend_rows_for(
    keys: LegendKeys,
    chat: bool,
) -> Vec<[(&'static str, &'static str); 2]> {
    match keys {
        LegendKeys::AtTheTable => LEGEND.into_iter().chain([exit_row(chat)]).collect(),
        LegendKeys::Waiting => vec![
            [("v", "eye view"), ("r R", "replay")],
            [("X", "resign"), ("", "")],
            exit_row(chat),
        ],
        LegendKeys::Watching => vec![[("v", "eye view"), ("r R", "replay")], exit_row(chat)],
    }
}

/// One row of the legend: two keys with their labels, in fixed columns so
/// the rows line up.
fn legend_row(row: [(&'static str, &'static str); 2]) -> Line<'static> {
    let key = Style::default().fg(theme::AMBER());
    let label = Style::default().fg(theme::TEXT_DIM());
    let mut spans = Vec::new();
    for (index, (k, what)) in row.into_iter().enumerate() {
        let width = if index == 0 { 6 } else { 4 };
        spans.push(Span::styled(format!("{k:<width$}"), key));
        spans.push(Span::styled(format!("{what:<10}"), label));
    }
    Line::from(spans)
}

/// Game, seats, groups, and what is left on the table.
fn info_lines(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    pool: &PoolDetail,
) -> Vec<Line<'static>> {
    let state = &pool.state;
    let me = daily.user_id();
    let dim = Style::default().fg(theme::TEXT_DIM());
    let text = Style::default().fg(theme::TEXT());

    // The game and its table share a row: two things a player reads once,
    // and a row apiece was a row off the cue drawing.
    let mut lines = vec![Line::from(vec![
        Span::styled(
            match state.rules {
                PoolRules::EightBall => "Eight-ball",
                PoolRules::NineBall => "Nine-ball",
                PoolRules::Snooker => "Snooker",
            },
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" · {}", state.table), dim),
    ])];
    lines.push(Line::from(""));

    for seat in 0u8..2 {
        let user = state.user_of(seat);
        let name = if user == me {
            "you".to_string()
        } else {
            name_for(board, user)
        };
        // Snooker's score *is* the state of the frame, so it goes where the
        // eight-ball groups go — the same line, for the same reason.
        let group = if state.rules.scores() {
            format!(" · {}", state.scores[seat as usize])
        } else {
            match state.groups {
                Some(groups) => format!(" · {}", group_label(groups[seat as usize])),
                None => String::new(),
            }
        };
        let at_table = detail.is_active() && state.turn == seat;
        lines.push(Line::from(Span::styled(
            format!("{} {name}{group}", if at_table { "▸" } else { " " }),
            if at_table {
                Style::default().fg(theme::AMBER())
            } else {
                text
            },
        )));
    }
    lines.push(Line::from(""));

    let targets = state.legal_targets();
    lines.push(Line::from(on_line(state, &targets)));
    lines.extend(last_shot_lines(pool, board.pool_shot_pending()));
    lines
}

/// What the last shot did: the free ball it left, the foul it was, or its
/// label. Empty while that is still the shot's own news to break.
///
/// Until a fresh shot has finished playing the board is still showing the
/// rack it was played on, or the balls on their way, and a "last: 8 down ·
/// rack over" beside either would be the result arriving before the shot that
/// earned it (`PoolDetail::withholds_result`).
fn last_shot_lines(pool: &PoolDetail, shot_pending: bool) -> Vec<Line<'static>> {
    let state = &pool.state;
    let mut lines = Vec::new();
    if pool.withholds_result(shot_pending) {
        return lines;
    }
    if state.free_ball {
        lines.push(Line::from(Span::styled(
            "free ball: anything counts".to_string(),
            Style::default().fg(theme::SUCCESS()),
        )));
    }
    if let Some(foul) = state.last_foul {
        lines.push(Line::from(Span::styled(
            format!("last: {}", foul.label()),
            Style::default().fg(theme::ERROR()),
        )));
    } else if let Some(shot) = state.shots.last() {
        lines.push(Line::from(Span::styled(
            format!("last: {}", shot.label),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines
}

/// What the line is on, in the language of the game: the ball and the cut;
/// the rail; a pocket the cue ball is headed straight for. It never says
/// where the object ball ends up: judging that is the shot.
pub(crate) fn target_label(state: &DailyPoolState, line: Option<&ShotLine>) -> String {
    let Some(line) = line else {
        return "on: nothing".to_string();
    };
    match line.hit {
        Hit::Ball { id, .. } => {
            let name = ball_name(state, id);
            let Some(object) = line.object else {
                return format!("on: {name}");
            };
            let degrees = object.cut.abs().to_degrees();
            let cut = if degrees < 0.5 {
                "full ball".to_string()
            } else if object.cut > 0.0 {
                format!("cut {degrees:.0}° right")
            } else {
                format!("cut {degrees:.0}° left")
            };
            format!("on: {name} · {cut}")
        }
        Hit::Cushion { .. } => match line.target() {
            Some(id) => format!("on: the rail, past {}", ball_name(state, id)),
            None => "on: the rail".to_string(),
        },
        Hit::Pocket { index, .. } => format!("on: the {} pocket", table::pocket_name(index)),
        Hit::Nothing { .. } => "on: nothing".to_string(),
    }
}

/// A ball as the striker would name it: its number, or in snooker its colour
/// and what it scores, which is the one thing a small ball has no room to
/// print.
fn ball_name(state: &DailyPoolState, id: u8) -> String {
    match state.rules {
        PoolRules::EightBall | PoolRules::NineBall => format!("the {id}"),
        PoolRules::Snooker if rules_snooker::is_red(id) => "a red (1)".to_string(),
        PoolRules::Snooker => format!(
            "the {} ({})",
            rules_snooker::name(id),
            rules_snooker::value(id)
        ),
    }
}

/// What the other player is doing, in the third person. `ShotMode::label` is
/// written for the person holding the cue ("aiming", "ready"), which reads
/// wrong about somebody else.
fn lining_up(mode: crate::app::games::pool_core::cue::ShotMode) -> &'static str {
    use crate::app::games::pool_core::cue::ShotMode;
    match mode {
        ShotMode::Idle => "at the table",
        ShotMode::Aim => "lining it up",
        ShotMode::Spin => "setting spin",
        ShotMode::Place => "placing the cue ball",
        ShotMode::Stroke(_) => "on the stroke",
    }
}

/// What the striker is on, in the language of the game being played.
///
/// A pool player is on a list of numbers and reads them as numbers. A snooker
/// player is on "a red" or "the colours" — fifteen ids on one line would be
/// noise, and none of those balls has a number printed on it anyway.
///
/// **Every ball named here is written in that ball's own colour**, through
/// `table_ui::text_colour`, which is the same hue the table paints it with
/// lifted only where it would be unreadable as text. On a table drawn a
/// hundred columns wide a ball is a few pixels and its number a few more, so
/// the list is where a player actually reads which ball is which — and a list
/// of plain digits made them look it up twice.
fn on_line(state: &DailyPoolState, targets: &[u8]) -> Vec<Span<'static>> {
    let text = Style::default().fg(theme::TEXT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    // Raw RGB rather than a theme colour: the table beside it paints the same
    // ball with exactly these numbers whatever the theme is, and a list whose
    // colours drift from the balls they name is worse than no colour at all.
    let ball = |id: u8| Style::default().fg(rgb(table_ui::text_colour(id)));
    let mut spans = vec![Span::styled("on: ", text)];
    if targets.is_empty() {
        spans.push(Span::styled("nothing", text));
        return spans;
    }
    if state.rules.scores() {
        if state.free_ball {
            spans.push(Span::styled("any ball", text));
            return spans;
        }
        if targets.iter().copied().all(rules_snooker::is_red) {
            spans.push(Span::styled("a red", ball(targets[0])));
            spans.push(Span::styled(format!(" ({} up)", targets.len()), dim));
            return spans;
        }
        if targets.len() > 1 {
            // The colour of the striker's choosing: name every one still up,
            // each in its own, which is the list they are choosing from.
            for (index, id) in targets.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::styled(" ", text));
                }
                spans.push(Span::styled(rules_snooker::name(*id), ball(*id)));
            }
            return spans;
        }
        spans.push(Span::styled(
            rules_snooker::name(targets[0]),
            ball(targets[0]),
        ));
        spans.push(Span::styled(
            format!(" ({})", rules_snooker::value(targets[0])),
            dim,
        ));
        return spans;
    }
    for (index, id) in targets.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" ", text));
        }
        spans.push(Span::styled(id.to_string(), ball(*id)));
    }
    spans
}

fn group_label(group: crate::app::games::pool_core::rules::Group) -> &'static str {
    use crate::app::games::pool_core::rules::Group;
    match group {
        Group::Solids => "solids",
        Group::Stripes => "stripes",
    }
}

/// The shooter's-eye view plus its three readouts.
///
/// Records what the panel drew where, so a click on it becomes the thing that
/// part of the panel stands for: the target ball and sighting line arm the
/// aim, the cue ball's face places the tip, the cue arms the stroke.
/// `cue_ui::draw` hands the geometry back for exactly this, so the hit test
/// never reconstructs the panel's own layout.
fn draw_cue_panel(
    frame: &mut Frame,
    area: Rect,
    board: &DailyBoardState,
    pool: &PoolDetail,
    shot: &PoolDraft,
) {
    if area.height <= READOUT_ROWS {
        board.cue_geometry.set(None);
        return;
    }
    let rows =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(READOUT_ROWS)]).split(area);

    let draft = shot;
    let line = draft.line(&pool.state);
    let mut canvas = Canvas::new(rows[0].width, rows[0].height, BACKDROP);
    // Legal off the ball the cue ball will actually *touch*, matching the ring
    // the table view draws round the same ball — an aim sighted past a ball
    // into the rail behind it contacts nothing and is nobody's foul yet.
    let legal = pool.state.legal_targets();
    let view = CueView {
        target: line.and_then(|line| line.target()),
        target_fault: line
            .and_then(|line| line.first_ball())
            .is_some_and(|id| !legal.contains(&id)),
        distance: line
            .map(|line| {
                let at = line.hit.at();
                (at[0] - line.from[0]).hypot(at[1] - line.from[1])
            })
            .unwrap_or(1.0),
        aim_offset: line
            .and_then(|line| line.sighted)
            .map_or(0.0, |(_, offset)| offset),
        tip: draft.tip,
        power: draft.power(),
        mode: draft.mode,
        // From the moment the stroke registers until the shot has finished
        // playing. No timer: the shot's own lifetime is the window, which is
        // both the honest one and the one that cannot drift out of step.
        follow_through: rolling(board, pool),
    };
    let panel = cue_ui::draw(&mut canvas, &view);
    frame.render_widget(Paragraph::new(canvas.to_lines()), rows[0]);
    board.cue_geometry.set(Some(PoolCueHit {
        area: rows[0],
        panel,
    }));

    let dim = Style::default().fg(theme::TEXT_DIM());
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(cue_ui::aim_label(draft.azimuth), dim)),
            Line::from(Span::styled(target_label(&pool.state, line.as_ref()), dim)),
            Line::from(Span::styled(cue_ui::spin_label(draft.tip), dim)),
            Line::from(Span::styled(
                cue_ui::power_label(draft.power(), draft.mode.band(), MAX_SPEED),
                dim,
            )),
        ]),
        rows[1],
    );
}

fn status_line(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    pool: &PoolDetail,
) -> Line<'static> {
    if board.resign_confirm {
        return Line::from(Span::styled(
            "Resign this match? Press X again to confirm.",
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        ));
    }
    // Watching, not playing — and this comes **before** the result, because
    // the shot that ends a rack is exactly the one where announcing early is
    // wrong. A pot is not a win until the rest of the shot has played out: the
    // cue ball can still follow the money ball down, and then the shot is a
    // foul and the rack belongs to the other player. Calling it while the
    // balls are still rolling gives away an answer the table has not reached,
    // and half the time gives away the wrong one.
    if rolling(board, pool) {
        let (heading, aside) = if pool.replaying {
            ("▶ replaying", "   r again to stop")
        } else {
            ("▶ the shot is playing", "   nothing to do but watch")
        };
        return Line::from(vec![
            Span::styled(
                heading,
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(aside, Style::default().fg(theme::TEXT_DIM())),
        ]);
    }

    if !detail.is_active() {
        let (heading, subtitle, color) = result_banner(daily, board, detail);
        return Line::from(Span::styled(
            format!("{heading} · {subtitle}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }

    let my_turn = detail.row.turn_user_id == Some(daily.user_id());
    let mut spans = vec![Span::styled(
        if my_turn {
            "Your shot".to_string()
        } else {
            format!(
                "Waiting for {}",
                name_for(board, detail.row.turn_user_id.unwrap_or(Uuid::nil()))
            )
        },
        Style::default()
            .fg(if my_turn {
                theme::AMBER()
            } else {
                theme::TEXT_DIM()
            })
            .add_modifier(Modifier::BOLD),
    )];
    // Not your shot, but somebody is at the table: say what they are doing,
    // because the table and the panel are already drawing it and a player who
    // is not told will read a moving cue as their own board misbehaving.
    if !my_turn && let Some(theirs) = pool.watching {
        spans.push(Span::styled(
            format!("   they are {}", lining_up(theirs.mode)),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if my_turn && !board.spectating {
        spans.extend(shooter_spans(pool.draft.mode, prompt_for(pool)));
    }
    if let Some(deadline) = detail.row.turn_deadline_at {
        spans.push(Span::styled(
            format!("   {} on the clock", format_deadline(deadline, Utc::now())),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    Line::from(spans)
}

/// Something the rules want from the shooter before the shot, said on the
/// status line. At most one at a time, and the order is the order of urgency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Prompt {
    /// The cue ball is off the table and has to be set down.
    MustPlace,
    /// Ball in hand granted, cue ball still up: an offer, not a demand.
    InHand,
    /// The eight needs a pocket named, and none is yet.
    CallPocket,
    /// A pocket is named.
    Calling(u8),
}

impl Prompt {
    /// Every prompt, for the test that lays the status line out at its
    /// longest.
    #[cfg(test)]
    pub(crate) const ALL: [Self; 4] = [
        Self::MustPlace,
        Self::InHand,
        Self::CallPocket,
        Self::Calling(0),
    ];

    fn span(self) -> Span<'static> {
        match self {
            Self::MustPlace => Span::styled(
                "   click the cloth to set the cue ball down",
                Style::default().fg(theme::ERROR()),
            ),
            Self::InHand => Span::styled(
                "   ball in hand · m to move it",
                Style::default().fg(theme::AMBER()),
            ),
            // Naming one is not optional: the server refuses an uncalled shot
            // on the eight, so say how to name it.
            Self::CallPocket => Span::styled(
                "   call a pocket: click it, or p",
                Style::default().fg(theme::AMBER()),
            ),
            Self::Calling(index) => Span::styled(
                format!("   calling the {}", table::pocket_name(index)),
                Style::default().fg(theme::AMBER()),
            ),
        }
    }
}

fn prompt_for(pool: &PoolDetail) -> Option<Prompt> {
    if pool.state.must_place() && pool.draft.place.is_none() {
        Some(Prompt::MustPlace)
    } else if pool.state.ball_in_hand.is_some() && pool.draft.place.is_none() {
        Some(Prompt::InHand)
    } else if pool.state.requires_call() {
        match pool.draft.called_pocket {
            Some(index) => Some(Prompt::Calling(index)),
            None => Some(Prompt::CallPocket),
        }
    } else {
        None
    }
}

/// The shooter's part of the status line: the armed mode, what the mouse
/// does in it, and whatever the rules are asking for. Pure, so the test can
/// lay out the longest case and measure it.
///
/// The two halves are not allowed to say the same thing. While the cue ball
/// is being carried the mode's own hint is the placing instruction, so the
/// prompts about placing it are dropped; and while the cue ball is off the
/// table and *not* being carried, the prompt is the one thing to do, so the
/// mode's hint about aiming would be a contradiction and is dropped instead.
pub(crate) fn shooter_spans(mode: ShotMode, prompt: Option<Prompt>) -> Vec<Span<'static>> {
    let prompt = match prompt {
        Some(Prompt::MustPlace | Prompt::InHand) if mode == ShotMode::Place => None,
        Some(
            prompt @ (Prompt::MustPlace | Prompt::InHand | Prompt::CallPocket | Prompt::Calling(_)),
        ) => Some(prompt),
        None => None,
    };
    let hint = match prompt {
        Some(Prompt::MustPlace) => None,
        Some(Prompt::InHand | Prompt::CallPocket | Prompt::Calling(_)) | None => Some(mode.hint()),
    };
    let mut spans = vec![Span::styled(
        format!("   {}", mode.label()),
        Style::default().fg(theme::TEXT_BRIGHT()),
    )];
    if let Some(hint) = hint {
        spans.push(Span::styled(
            format!(" · {hint}"),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if let Some(prompt) = prompt {
        spans.push(prompt.span());
    }
    spans
}

#[cfg(test)]
#[path = "pool_ui_test.rs"]
mod pool_ui_test;
