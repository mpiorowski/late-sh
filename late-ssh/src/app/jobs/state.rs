//! The Jobs shelf's session state and the pure pieces every surface
//! shares: the viewer's tag set, the match score, the row copy. No I/O;
//! `svc.rs` fills `items` from the replica's snapshot.

use std::cell::Cell;

use late_core::models::app_flag::AppFlag;
use late_core::models::job_posting::{JobPosting, RemoteKind};
use late_core::models::work_profile::{WorkProfile, WorkStatus};
use tokio::sync::{broadcast, oneshot, watch};

use super::svc::{JobsEvent, JobsService, JobsSnapshot};
use super::vocab;

/// Matches shown under a person's own card, and in the paper.
pub(crate) const FOR_YOU_LIMIT: usize = 5;
pub(crate) const PAPER_MATCHES: usize = 3;

/// The shelf's per-session state, owned by `App`.
pub(crate) struct JobsState {
    pub(super) service: JobsService,
    pub(super) rx: watch::Receiver<JobsSnapshot>,
    pub(super) events_rx: broadcast::Receiver<JobsEvent>,
    /// The replica's active rows, copied in `tick` when the snapshot moves.
    pub(crate) items: Vec<JobPosting>,
    /// Whether the snapshot has been read from the database at all, so an
    /// empty shelf can say "nothing yet" rather than "loading".
    pub(crate) loaded: bool,
    /// `/` on the shelf: only postings that match the viewer's tags.
    pub(crate) for_me: bool,
    selected: usize,
    /// Under the stacked layout the detail pane opens over the list.
    detail_open: bool,
    narrow: Cell<bool>,
    pub(super) pending_flag_writes: Vec<PendingFlagWrite>,
}

/// An admin's flag write in flight, answered with a banner in tick.
pub(super) struct PendingFlagWrite {
    pub flag: AppFlag,
    pub enabled: bool,
    pub done: &'static str,
    pub rx: oneshot::Receiver<anyhow::Result<()>>,
}

impl JobsState {
    pub(crate) fn new(service: JobsService) -> Self {
        let rx = service.subscribe_snapshot();
        let events_rx = service.subscribe_events();
        Self {
            service,
            rx,
            events_rx,
            items: Vec::new(),
            loaded: false,
            for_me: false,
            selected: 0,
            detail_open: false,
            narrow: Cell::new(false),
            pending_flag_writes: Vec::new(),
        }
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn move_selection(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        let clamped = self.selected.min(len - 1) as isize;
        self.selected = (clamped + delta).clamp(0, len as isize - 1) as usize;
    }

    pub(crate) fn clamp_selection(&mut self, len: usize) {
        if len == 0 {
            self.selected = 0;
        } else if self.selected > len - 1 {
            self.selected = len - 1;
        }
    }

    pub(crate) fn toggle_for_me(&mut self) {
        self.for_me = !self.for_me;
        self.selected = 0;
    }

    pub(crate) fn set_narrow(&self, narrow: bool) {
        self.narrow.set(narrow);
    }

    pub(crate) fn narrow(&self) -> bool {
        self.narrow.get()
    }

    pub(crate) fn detail_open(&self) -> bool {
        self.detail_open
    }

    pub(crate) fn open_detail(&mut self) {
        self.detail_open = true;
    }

    pub(crate) fn close_detail(&mut self) {
        self.detail_open = false;
    }

    /// The rows the shelf shows right now: everything, or the viewer's
    /// matches best first.
    pub(crate) fn visible<'a>(&'a self, viewer_tags: &[String]) -> Vec<&'a JobPosting> {
        if self.for_me {
            matches(&self.items, viewer_tags, self.items.len())
        } else {
            self.items.iter().collect()
        }
    }
}

/// What `/jobs` asked for. The open is for everyone; the rest is the
/// press, admin-only and refused with a banner for anyone else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobsCommand {
    /// `/jobs`: the Profiles page on the Jobs shelf.
    Open,
    /// `/jobs pull`: run tonight's press now (fetch, read, release,
    /// expire), or just fetch and read when today's slice is out already.
    Pull,
    /// `/jobs release`: release a slice of the HN queue now, on top of
    /// whatever the day already released.
    Release,
    /// `/jobs on`: the press runs (the kill switch row).
    On,
    /// `/jobs off`: no press, an empty shelf that says so, no NEW WORK.
    Off,
}

impl JobsCommand {
    pub(crate) fn admin_only(self) -> bool {
        match self {
            Self::Open => false,
            Self::Pull | Self::Release | Self::On | Self::Off => true,
        }
    }
}

/// `None`: not a `/jobs` line. `Some(None)`: `/jobs` with junk after it.
pub(crate) fn parse_jobs_command(body: &str) -> Option<Option<JobsCommand>> {
    let rest = body.trim().strip_prefix("/jobs")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let words: Vec<&str> = rest.split_whitespace().collect();
    Some(match words.as_slice() {
        [] => Some(JobsCommand::Open),
        ["pull"] => Some(JobsCommand::Pull),
        ["release"] => Some(JobsCommand::Release),
        ["on"] => Some(JobsCommand::On),
        ["off"] => Some(JobsCommand::Off),
        _ => None,
    })
}

/// The tags a person is matched on: their card's normalized skills plus
/// the languages on their late.fetch, folded through the vocabulary and
/// deduplicated. Empty for someone with neither.
pub(crate) fn viewer_tags(card: Option<&WorkProfile>, langs: &[String]) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    let skills = card.map(|card| card.skills_tags.as_slice()).unwrap_or(&[]);
    for tag in skills.iter().chain(langs.iter()) {
        let Some(canonical) = vocab::canonical(tag) else {
            continue;
        };
        if !tags.iter().any(|known| known == canonical) {
            tags.push(canonical.to_string());
        }
    }
    tags
}

/// Whether the paper and the "for you" section speak to this card at all:
/// an open or casual card wants matches, a not-looking one said so.
pub(crate) fn wants_matches(status: WorkStatus) -> bool {
    match status {
        WorkStatus::Open | WorkStatus::Casual => true,
        WorkStatus::NotLooking => false,
    }
}

/// How many of the viewer's tags the posting carries.
pub(crate) fn score(posting: &JobPosting, viewer_tags: &[String]) -> usize {
    posting
        .tags
        .iter()
        .filter(|tag| viewer_tags.contains(tag))
        .count()
}

/// The postings that share a tag with the viewer, best overlap first,
/// newest release inside a tie (the input order), at most `limit`.
pub(crate) fn matches<'a>(
    items: &'a [JobPosting],
    viewer_tags: &[String],
    limit: usize,
) -> Vec<&'a JobPosting> {
    let mut scored: Vec<(usize, usize, &JobPosting)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, posting)| {
            let score = score(posting, viewer_tags);
            (score > 0).then_some((score, index, posting))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, _, posting)| posting)
        .collect()
}

/// The scope line: `remote worldwide`, `remote · EU, US`, `hybrid · Berlin`.
pub(crate) fn scope_label(posting: &JobPosting) -> String {
    let kind = match posting.remote_kind {
        Some(kind) => kind,
        None => RemoteKind::Regions,
    };
    if posting.regions.is_empty() {
        kind.label().to_string()
    } else {
        format!("{} · {}", kind.label(), posting.regions.join(", "))
    }
}

/// One line for a match, the same under a card and in the paper:
/// `Company · Role · scope · tags · pay`.
pub(crate) fn match_line(posting: &JobPosting, tag_limit: usize) -> String {
    format!("{} · {}", posting.company, match_tail(posting, tag_limit))
}

/// The match line after the company: `Role · scope · tags · pay`, so a
/// surface can ink the company on its own.
pub(crate) fn match_tail(posting: &JobPosting, tag_limit: usize) -> String {
    let mut parts = vec![posting.title.clone(), scope_label(posting)];
    if !posting.tags.is_empty() {
        parts.push(
            posting
                .tags
                .iter()
                .take(tag_limit)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    if !posting.pay.trim().is_empty() {
        parts.push(posting.pay.trim().to_string());
    }
    parts.join(" · ")
}

pub(crate) fn posting_count_label(count: usize) -> String {
    match count {
        1 => "1 posting".to_string(),
        n => format!("{n} postings"),
    }
}
