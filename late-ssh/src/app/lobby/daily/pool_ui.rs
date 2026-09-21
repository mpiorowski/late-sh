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
        ball::CUE,
        canvas::Canvas,
        cue::{MAX_SPEED, ShotMode},
        cue_ui::{self, BACKDROP, CueView},
        rules::PoolRules,
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
/// The key legend under the readouts: the seven rows of `LEGEND` and one
/// more for leaving the board (chat, lobby).
const LEGEND_ROWS: u16 = LEGEND.len() as u16 + 1;
/// The cue drawing stops growing here. The balls take a share of the
/// panel's width as well as its height, and at the widest panel the width
/// is the limit from about here on, so rows past it are better spent on
/// the legend.
const MAX_CUE_ROWS: u16 = 28;
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
    let rolling = pool.playback.is_some() || pool.shot_in_flight;
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
    let (frames, aiming) = match &pool.playback {
        Some(playback) => (playback.frame(), false),
        None => (shot.frames(&pool.state), true),
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
    if pool.playback.is_some() {
        let played = pool.state.shots.last()?;
        let before = pool.state.prev_rack.as_ref()?;
        let cue = before
            .get(CUE)
            .filter(|ball| ball.potted.is_none())
            .map(|ball| ball.pos)
            .or(played.shot.place)?;
        return Some(Eye::behind(cue, played.shot.azimuth, spec, canvas));
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
        let chat = !board.spectating && detail.row.chat_room_id.is_some();
        frame.render_widget(Paragraph::new(legend_lines(chat)), rows[2]);
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
pub(crate) const LEGEND: [[(&str, &str); 2]; 7] = [
    [("h l", "aim 1°"), ("[ ]", "ball")],
    [("H L", "aim 0.1°"), ("{ }", "pot line")],
    [("a", "mouse aim"), ("'", "lowest ball")],
    [("e", "spin"), ("m", "ball in hand")],
    [("x s w", "stroke"), ("p", "call pocket")],
    [("c", "reset"), ("v", "eye view")],
    [("Esc", "back"), ("r", "resign")],
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

fn legend_lines(chat: bool) -> Vec<Line<'static>> {
    LEGEND
        .into_iter()
        .chain([exit_row(chat)])
        .map(legend_row)
        .collect()
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
    lines.push(Line::from(Span::styled(on_line(state, &targets), text)));
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
            dim,
        )));
    }
    lines
}

/// What the line is on, in the language of the game: the ball and the cut,
/// with the pocket when the object ball's own leg ends in one; the rail; a
/// pocket the cue ball is headed straight for.
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
            match object.hit {
                Hit::Pocket { index, .. } => {
                    format!("on: {name} · {cut} · {}", table::pocket_name(index))
                }
                Hit::Ball { .. } | Hit::Cushion { .. } | Hit::Nothing { .. } => {
                    format!("on: {name} · {cut}")
                }
            }
        }
        Hit::Cushion { .. } => match line.target() {
            Some(id) => format!("on: the rail, past {}", ball_name(state, id)),
            None => "on: the rail".to_string(),
        },
        Hit::Pocket { index, .. } => format!("on: the {} pocket", table::pocket_name(index)),
        Hit::Nothing { .. } => "on: nothing".to_string(),
    }
}

/// A ball as the striker would name it: its number, or its colour in snooker.
fn ball_name(state: &DailyPoolState, id: u8) -> String {
    if state.rules.scores() {
        match snooker_name(id) {
            "a red" => "a red".to_string(),
            colour => format!("the {colour}"),
        }
    } else {
        format!("the {id}")
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
fn on_line(state: &DailyPoolState, targets: &[u8]) -> String {
    use crate::app::games::pool_core::rules_snooker;
    if targets.is_empty() {
        return "on: nothing".to_string();
    }
    if state.rules.scores() {
        if state.free_ball {
            return "on: any ball".to_string();
        }
        if targets.iter().copied().all(rules_snooker::is_red) {
            return format!("on: a red ({} up)", targets.len());
        }
        if targets.len() > 1 {
            return "on: a colour".to_string();
        }
        return format!(
            "on: {} ({})",
            snooker_name(targets[0]),
            rules_snooker::value(targets[0])
        );
    }
    format!(
        "on: {}",
        targets
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    )
}

/// Snooker balls have names rather than numbers.
fn snooker_name(id: u8) -> &'static str {
    use crate::app::games::pool_core::rules_snooker as s;
    match id {
        s::YELLOW => "yellow",
        s::GREEN => "green",
        s::BROWN => "brown",
        s::BLUE => "blue",
        s::PINK => "pink",
        s::BLACK => "black",
        _ => "a red",
    }
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
    let view = CueView {
        target: line.and_then(|line| line.target()),
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
        follow_through: pool.shot_in_flight || pool.playback.is_some(),
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
            "Resign this match? Press r again to confirm.",
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
    if pool.playback.is_some() || pool.shot_in_flight {
        return Line::from(vec![
            Span::styled(
                "▶ the shot is playing",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "   nothing to do but watch",
                Style::default().fg(theme::TEXT_DIM()),
            ),
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
pub(crate) fn shooter_spans(mode: ShotMode, prompt: Option<Prompt>) -> Vec<Span<'static>> {
    let mut spans = vec![
        Span::styled(
            format!("   {}", mode.label()),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
        Span::styled(
            format!(" · {}", mode.hint()),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ];
    if let Some(prompt) = prompt {
        spans.push(prompt.span());
    }
    spans
}

#[cfg(test)]
#[path = "pool_ui_test.rs"]
mod pool_ui_test;
