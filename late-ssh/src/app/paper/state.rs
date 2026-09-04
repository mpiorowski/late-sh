//! The Late Edition's session state and the pure layout: from an edition's
//! rows plus this reader's rail order to the modal's lines. No I/O here,
//! and no palette either: lines carry `PaperInk`, and `ui.rs` picks the
//! colours inside the draw. `svc.rs` owns the requests and the tick.

use std::collections::HashSet;

use late_core::models::app_flag::AppFlag;
use late_core::models::paper::{PaperEdition, PaperRoomPage, PaperSectionKind, PaperStatus};
use tokio::sync::{broadcast, oneshot};
use uuid::Uuid;

use super::svc::{PaperEvent, PaperService, PaperTrigger};

/// Rooms the reader is not in that make the paper: the top few by
/// activity, bumped rooms first. A cap, so the paper stays a paper and
/// not the whole site.
pub(crate) const PAPER_ELSEWHERE_LIMIT: usize = 3;

/// The paper's per-session state, owned by `App`.
pub(crate) struct PaperState {
    pub(super) service: PaperService,
    pub(super) rx: broadcast::Receiver<PaperEvent>,
    pub(crate) modal: Option<PaperModal>,
    /// Armed at boot for returning readers with the tweak on; fires once
    /// the splash is down.
    pub(super) login_pop_pending: bool,
    /// The trigger whose result this session still wants. Closing the
    /// "at the press" modal clears it, so a late answer is dropped.
    pub(super) awaiting: Option<PaperTrigger>,
    /// A ready paper that arrived while the login announcements were up.
    pub(super) pending_modal: Option<PaperModal>,
    pub(super) pending_flag_writes: Vec<PendingFlagWrite>,
}

impl PaperState {
    pub(crate) fn modal_visible(&self) -> bool {
        self.modal.is_some()
    }

    /// Esc on the modal: closes it, and if it was still at the press,
    /// forgets the request so the answer is dropped when it lands.
    pub(crate) fn close_modal(&mut self) {
        if self.modal.take().is_some_and(|modal| modal.at_the_press) {
            self.awaiting = None;
        }
    }
}

/// An admin's flag write in flight, answered with a banner in tick.
pub(super) struct PendingFlagWrite {
    pub flag: AppFlag,
    pub enabled: bool,
    pub done: &'static str,
    pub rx: oneshot::Receiver<anyhow::Result<()>>,
}

/// What `/paper` asked for. The open is for everyone; the switches are
/// admin-only, refused with a banner for anyone else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaperCommand {
    /// `/paper`: today's edition, from the rows.
    Open,
    /// `/paper on`: the presses run (the kill switch row).
    On,
    /// `/paper off`: stop the presses everywhere; `/paper` banners.
    Off,
    /// `/paper outside on`: print the Outside page from the next sweep
    /// (the seeded state).
    OutsideOn,
    /// `/paper outside off`: drop it, for the day it reads like slop.
    OutsideOff,
    /// `/paper print`: sweep now instead of waiting for the interval.
    Print,
    /// `/paper preview`: lay out tomorrow's edition from today's messages
    /// so far, in memory and for the caller only, so a column can be read
    /// without waiting for midnight and without touching the real rows.
    Preview,
    /// `/paper reset`: drop today's rows and the caller's login stamp, so
    /// both the print and the pop can be seen again.
    Reset,
}

impl PaperCommand {
    pub(crate) fn admin_only(self) -> bool {
        match self {
            Self::Open => false,
            Self::On
            | Self::Off
            | Self::OutsideOn
            | Self::OutsideOff
            | Self::Print
            | Self::Preview
            | Self::Reset => true,
        }
    }
}

/// `None`: not a `/paper` line. `Some(None)`: `/paper` with junk after it.
pub(crate) fn parse_paper_command(body: &str) -> Option<Option<PaperCommand>> {
    let rest = body.trim().strip_prefix("/paper")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let words: Vec<&str> = rest.split_whitespace().collect();
    Some(match words.as_slice() {
        [] => Some(PaperCommand::Open),
        ["on"] => Some(PaperCommand::On),
        ["off"] => Some(PaperCommand::Off),
        ["outside", "on"] => Some(PaperCommand::OutsideOn),
        ["outside", "off"] => Some(PaperCommand::OutsideOff),
        ["print"] => Some(PaperCommand::Print),
        ["preview"] => Some(PaperCommand::Preview),
        ["reset"] => Some(PaperCommand::Reset),
        _ => None,
    })
}

/// The modal: a title, styled lines, a scroll offset.
#[derive(Clone, Debug)]
pub(crate) struct PaperModal {
    pub title: String,
    pub lines: Vec<PaperLine>,
    pub scroll_offset: u16,
    /// Still waiting for `/paper`'s answer; Esc drops the request.
    pub at_the_press: bool,
}

impl PaperModal {
    /// The spinner state after `/paper`, up until the rows arrive.
    pub(crate) fn at_the_press() -> Self {
        Self {
            title: " The Late Edition ".to_string(),
            lines: vec![
                PaperLine::new(),
                vec![PaperSpan::new("  at the press…", PaperInk::Meta)],
            ],
            scroll_offset: 0,
            at_the_press: true,
        }
    }

    pub(crate) fn edition(layout: PaperLayout<'_>) -> Self {
        Self {
            title: format!(
                " The Late Edition · {} ",
                layout.edition.edition.format("%a %b %-d")
            ),
            lines: lay_out(layout),
            scroll_offset: 0,
            at_the_press: false,
        }
    }

    pub(crate) fn scroll(&mut self, delta: i16) {
        let next = i32::from(self.scroll_offset) + i32::from(delta);
        self.scroll_offset = next.clamp(0, i32::from(u16::MAX)) as u16;
    }
}

/// Everything the layout needs from the session: the edition's rows and
/// how this reader's rail is ordered (favorites first, as the rail draws
/// them), which rooms they are in, and which rooms carry a shop bump.
/// The wall column: yesterday's most applauded gallery piece, printed in
/// black and white the way a paper prints anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaperWall {
    pub title: String,
    pub username: String,
    pub applause: i64,
    pub lines: Vec<String>,
}

pub(crate) struct PaperLayout<'a> {
    pub edition: &'a PaperEdition,
    /// The piece on the wall, when yesterday hung one.
    pub wall: Option<&'a PaperWall>,
    /// Member rooms in rail order; rooms the edition has no page for are
    /// skipped, rooms missing from the rail follow by activity.
    pub rail_order: &'a [Uuid],
    pub member_room_ids: &'a HashSet<Uuid>,
    /// Rail labels (slugs) of rooms under an active `room_bump`.
    pub bumped_labels: &'a [String],
}

/// What a span of the paper is, never what colour it is. The palette is a
/// render-pass thing (`theme`'s thread local is set by `App::render`), and
/// the modal is laid out a tick earlier, on whatever worker thread the
/// session happened to wake on: reading the palette here printed the paper
/// in whichever session last rendered on that thread. `ui.rs` maps these
/// to styles inside the draw, where the reader's theme is the live one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaperInk {
    /// A section heading: AMBER ON THE PAGE.
    Heading,
    /// A room's name, or the wall piece's title.
    Title,
    /// The counts beside a title.
    Meta,
    /// A shop bump on an elsewhere room.
    Bumped,
    /// The `/join` hint on an elsewhere topic room.
    JoinHint,
    /// A column's own text.
    Body,
    /// The byline and the footer.
    Faint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaperSpan {
    pub text: String,
    pub ink: PaperInk,
}

impl PaperSpan {
    pub(crate) fn new(text: impl Into<String>, ink: PaperInk) -> Self {
        Self {
            text: text.into(),
            ink,
        }
    }
}

/// One printed line: the spans left to right, empty for a blank line.
pub(crate) type PaperLine = Vec<PaperSpan>;

fn heading(text: &str) -> PaperLine {
    vec![PaperSpan::new(text, PaperInk::Heading)]
}

/// A room's headline. `elsewhere` rooms (the reader is not in them) count
/// "members" and, for topic rooms, carry the `/join` hint; the reader's own
/// rooms count "people".
fn room_head(page: &PaperRoomPage, elsewhere: bool, bumped: bool) -> PaperLine {
    let people = if elsewhere { "members" } else { "people" };
    let joinable = elsewhere && page.kind == "topic";
    let mut spans = vec![
        PaperSpan::new(format!("#{}", page.label), PaperInk::Title),
        PaperSpan::new(
            format!(
                " · {} message{} · {} {people}",
                page.message_count,
                if page.message_count == 1 { "" } else { "s" },
                page.member_count
            ),
            PaperInk::Meta,
        ),
    ];
    if bumped {
        spans.push(PaperSpan::new(" · bumped", PaperInk::Bumped));
    }
    if joinable {
        spans.push(PaperSpan::new(
            format!(" · /join #{}", page.label),
            PaperInk::JoinHint,
        ));
    }
    spans
}

fn column_lines(text: &str) -> Vec<PaperLine> {
    text.lines()
        .map(|line| vec![PaperSpan::new(line, PaperInk::Body)])
        .collect()
}

fn labels(pages: &[&PaperRoomPage]) -> String {
    pages
        .iter()
        .map(|page| format!("#{}", page.label))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The whole paper, top to bottom: byline, your rooms in rail order,
/// elsewhere, what we were reading, outside, and a footer naming the
/// rooms that were quiet or still at the press.
pub(crate) fn lay_out(layout: PaperLayout<'_>) -> Vec<PaperLine> {
    let PaperLayout {
        edition,
        wall,
        rail_order,
        member_room_ids,
        bumped_labels,
    } = layout;
    let covered = edition.edition.pred_opt().unwrap_or(edition.edition);
    let mut lines = vec![vec![PaperSpan::new(
        format!(
            "by @graybeard · covers {} (UTC) · he read it all so you would not have to",
            covered.format("%a %b %-d")
        ),
        PaperInk::Faint,
    )]];

    // Member rooms in rail order, then any the rail does not list.
    let mut member_pages: Vec<&PaperRoomPage> = Vec::new();
    for room_id in rail_order {
        if let Some(page) = edition.rooms.iter().find(|page| page.room_id == *room_id)
            && member_room_ids.contains(room_id)
            && !member_pages.iter().any(|seen| seen.room_id == page.room_id)
        {
            member_pages.push(page);
        }
    }
    for page in &edition.rooms {
        if member_room_ids.contains(&page.room_id)
            && !member_pages.iter().any(|seen| seen.room_id == page.room_id)
        {
            member_pages.push(page);
        }
    }
    let mut quiet: Vec<&PaperRoomPage> = Vec::new();
    let mut at_the_press: Vec<&PaperRoomPage> = Vec::new();
    let mut missed: Vec<&PaperRoomPage> = Vec::new();
    let mut printed_any = false;
    for page in &member_pages {
        match page.status {
            PaperStatus::Ready => {
                if !printed_any {
                    lines.push(PaperLine::new());
                    lines.push(heading("YOUR ROOMS"));
                    printed_any = true;
                }
                lines.push(PaperLine::new());
                lines.push(room_head(page, false, false));
                if let Some(text) = &page.text {
                    lines.extend(column_lines(text));
                }
            }
            PaperStatus::Quiet => quiet.push(page),
            PaperStatus::Printing => at_the_press.push(page),
            PaperStatus::Failed => missed.push(page),
        }
    }

    // Elsewhere: public rooms the reader is not in, bumped first, then by
    // activity (the rows already come sorted by message count). The
    // `/join` hint is for topic rooms only: `/join #<label>` opens (or
    // creates) a topic room by slug, and a language room's label is its
    // language code, which would open a brand-new topic room instead.
    let mut elsewhere: Vec<&PaperRoomPage> = edition
        .rooms
        .iter()
        .filter(|page| {
            page.status == PaperStatus::Ready && !member_room_ids.contains(&page.room_id)
        })
        .collect();
    let is_bumped = |page: &PaperRoomPage| {
        page.kind == "topic" && !page.permanent && bumped_labels.contains(&page.label)
    };
    elsewhere.sort_by_key(|page| !is_bumped(page));
    elsewhere.truncate(PAPER_ELSEWHERE_LIMIT);
    if !elsewhere.is_empty() {
        lines.push(PaperLine::new());
        lines.push(heading("ELSEWHERE ON LATE.SH"));
        for page in elsewhere {
            lines.push(PaperLine::new());
            lines.push(room_head(page, true, is_bumped(page)));
            if let Some(text) = &page.text {
                lines.extend(column_lines(text));
            }
        }
    }

    for (kind, title) in [
        (PaperSectionKind::Reading, "WHAT WE WERE READING"),
        (PaperSectionKind::Outside, "OUTSIDE"),
    ] {
        let Some(section) = edition
            .sections
            .iter()
            .find(|section| section.section == kind && section.status == PaperStatus::Ready)
        else {
            continue;
        };
        let Some(text) = &section.text else {
            continue;
        };
        lines.push(PaperLine::new());
        lines.push(heading(title));
        lines.extend(column_lines(text));
    }

    if let Some(wall) = wall {
        lines.push(PaperLine::new());
        lines.push(heading("ON THE WALL"));
        lines.push(vec![
            PaperSpan::new(format!("\"{}\"", wall.title), PaperInk::Title),
            PaperSpan::new(
                format!(
                    " by @{}, hung yesterday, {} applause so far. Page 4 has it in colour.",
                    wall.username, wall.applause
                ),
                PaperInk::Meta,
            ),
        ]);
        lines.push(PaperLine::new());
        for line in &wall.lines {
            lines.push(vec![PaperSpan::new(format!("    {line}"), PaperInk::Body)]);
        }
    }

    if !quiet.is_empty() || !at_the_press.is_empty() || !missed.is_empty() {
        lines.push(PaperLine::new());
        let mut footer = Vec::new();
        if !quiet.is_empty() {
            footer.push(format!("quiet: {}", labels(&quiet)));
        }
        if !at_the_press.is_empty() {
            footer.push(format!("still at the press: {}", labels(&at_the_press)));
        }
        if !missed.is_empty() {
            footer.push(format!("missed the press: {}", labels(&missed)));
        }
        lines.push(vec![PaperSpan::new(footer.join(" · "), PaperInk::Faint)]);
    }

    lines
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
