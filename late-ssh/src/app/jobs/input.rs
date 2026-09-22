//! Keys on the Jobs shelf, and on the post form over it. The page-level
//! keys (`Space`, `w`, `i`) are the directory's; these are the shelf's own.

use late_core::models::job_posting::JobSource;

use crate::app::common::primitives::Banner;
use crate::app::common::textarea_input::{
    EditOutcome, handle_multiline_edit, handle_single_line_edit,
};
use crate::app::input::ParsedInput;
use crate::app::state::App;
use crate::app::tag_picker::{self, state::TagPickerTarget};

use super::post::{PostField, PostKind};
use super::state::viewer_tags;

const CTRL_S: u8 = 0x13;

/// The viewer's match tags from the session's own data: their card in the
/// work feed, their languages on the profile.
pub(crate) fn own_tags(app: &App) -> Vec<String> {
    let card = app
        .chat
        .work
        .all_items()
        .iter()
        .find(|item| item.profile.user_id == app.user_id)
        .map(|item| &item.profile);
    viewer_tags(card, &app.profile_state.profile().langs)
}

fn visible_len(app: &App) -> usize {
    app.jobs.visible(&own_tags(app)).len()
}

pub(crate) fn handle_byte(app: &mut App, byte: u8) -> bool {
    let narrow = app.jobs.narrow();
    match byte {
        b'j' | b'J' => {
            let len = visible_len(app);
            app.jobs.move_selection(1, len);
            true
        }
        b'k' | b'K' => {
            let len = visible_len(app);
            app.jobs.move_selection(-1, len);
            true
        }
        b'h' | b'H' => {
            if narrow && app.jobs.detail_open() {
                app.jobs.close_detail();
            }
            true
        }
        b'l' | b'L' => {
            if narrow && !app.jobs.detail_open() {
                app.jobs.open_detail();
            }
            true
        }
        b'\r' | b'\n' | b'c' | b'C' => {
            if narrow && !app.jobs.detail_open() {
                app.jobs.open_detail();
                return true;
            }
            copy_link(app);
            true
        }
        b'n' | b'N' => {
            open_post_form(app);
            true
        }
        b'd' | b'D' => {
            retract_selected(app);
            true
        }
        b'/' => {
            app.jobs.toggle_for_me();
            let banner = if app.jobs.for_me {
                if own_tags(app).is_empty() {
                    Banner::info(
                        "Nothing to match on yet: put skills on your card (w) or langs on your late.fetch.",
                    )
                } else {
                    Banner::success("Showing postings that carry your tags.")
                }
            } else {
                Banner::success("Showing every posting.")
            };
            app.banner = Some(banner);
            true
        }
        _ => false,
    }
}

fn copy_link(app: &mut App) {
    let tags = own_tags(app);
    let url = app
        .jobs
        .visible(&tags)
        .get(app.jobs.selected())
        .map(|posting| posting.url.clone());
    if let Some(url) = url {
        app.pending_clipboard = Some(url);
        app.banner = Some(Banner::success("Posting link copied!"));
    }
}

/// `n` on the shelf, or `/jobs post`: the post form, unless the press is
/// stopped, in which case the shelf is empty and says so already.
pub(crate) fn open_post_form(app: &mut App) {
    if !app.jobs.enabled() {
        app.banner = Some(Banner::error(
            "The job press is stopped; nothing can be posted until it is back on.",
        ));
        return;
    }
    app.jobs.post.open();
}

/// `d` on a posting: down it comes when it was written here by you, or
/// by anyone when you moderate. The service answers with a banner.
fn retract_selected(app: &mut App) {
    let tags = own_tags(app);
    let selected = app
        .jobs
        .visible(&tags)
        .get(app.jobs.selected())
        .map(|posting| (posting.id, posting.source, posting.posted_by));
    let Some((id, source, posted_by)) = selected else {
        return;
    };
    let moderator = app.is_admin || app.is_moderator;
    let mine = posted_by == Some(app.user_id);
    if source != JobSource::Late || !(mine || moderator) {
        app.banner = Some(Banner::error(
            "Only postings made here come down, by whoever posted them or a moderator.",
        ));
        return;
    }
    app.banner = Some(Banner::info("Taking the posting down…"));
    app.jobs.service.request_retract(app.user_id, id, moderator);
}

/// Keys while the post form is open. Order: a save in flight waits, then
/// Ctrl+S, then a row being typed, then the form's own keys.
pub(crate) fn handle_post_input(app: &mut App, event: &ParsedInput) {
    if app.jobs.post.pending() {
        return;
    }
    if matches!(event, ParsedInput::Byte(CTRL_S) | ParsedInput::AltS) {
        submit_post(app);
        return;
    }
    // The parser hands printable keys over as `Char`; the row keys read
    // bytes, so fold ASCII back. A row being typed gets the original.
    let key = match event {
        ParsedInput::Char(ch) if ch.is_ascii() => ParsedInput::Byte(*ch as u8),
        other => other.clone(),
    };
    if app.jobs.post.editing() {
        handle_post_typing(app, event);
        return;
    }
    let form = &mut app.jobs.post;
    match key {
        ParsedInput::Byte(b'j' | b'J' | b'\t') | ParsedInput::Arrow(b'B') => form.move_row(1),
        ParsedInput::Byte(b'k' | b'K') | ParsedInput::BackTab | ParsedInput::Arrow(b'A') => {
            form.move_row(-1)
        }
        ParsedInput::Byte(b'h' | b'H') | ParsedInput::Arrow(b'D') => form.cycle_scope(false),
        ParsedInput::Byte(b'l' | b'L') | ParsedInput::Arrow(b'C') => form.cycle_scope(true),
        ParsedInput::Byte(b'\r' | b'e' | b'E' | b'i' | b'I') => match form.active_field() {
            PostField::Tags => tag_picker::input::open(app, TagPickerTarget::JobPost),
            PostField::Company
            | PostField::Title
            | PostField::Link
            | PostField::Scope
            | PostField::Regions
            | PostField::Pay
            | PostField::Excerpt => form.start_editing(),
        },
        ParsedInput::Byte(0x1B) => {
            form.escape();
        }
        _ => {}
    }
}

fn handle_post_typing(app: &mut App, event: &ParsedInput) {
    let form = &mut app.jobs.post;
    if matches!(event, ParsedInput::Byte(b'\t')) {
        form.commit_and_advance(true);
        return;
    }
    if matches!(event, ParsedInput::BackTab) {
        form.commit_and_advance(false);
        return;
    }
    let field = form.active_field();
    let max = field.max_len();
    let outcome = match field.kind() {
        PostKind::Text => handle_single_line_edit(form.field_mut(field), event, max),
        PostKind::Multi => handle_multiline_edit(form.field_mut(field), event, max),
        PostKind::Choice | PostKind::Tags => EditOutcome::Ignored,
    };
    match outcome {
        EditOutcome::Handled => {}
        EditOutcome::Submit => form.commit_and_advance(true),
        EditOutcome::Cancel => form.stop_editing(),
        EditOutcome::Ignored => {
            // Up/down leave a one-line row for its neighbour; in the
            // excerpt they moved the cursor already.
            if field.kind() == PostKind::Text {
                match event {
                    ParsedInput::Arrow(b'A') => form.commit_and_advance(false),
                    ParsedInput::Arrow(b'B') => form.commit_and_advance(true),
                    _ => {}
                }
            }
        }
    }
}

/// Ctrl+S: validate, then hand the posting to the service; the answer
/// comes back through the jobs events in `tick`.
fn submit_post(app: &mut App) {
    let Some(posting) = app.jobs.post.submit(app.user_id) else {
        return;
    };
    app.jobs.service.request_post(posting);
}

/// Esc from the app-level escape dispatch.
pub(crate) fn handle_post_escape(app: &mut App) {
    if app.jobs.post.pending() {
        return;
    }
    app.jobs.post.escape();
}

pub(crate) fn move_selection(app: &mut App, delta: isize) {
    let len = visible_len(app);
    app.jobs.move_selection(delta, len);
}

pub(crate) fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => handle_byte(app, b'k'),
        b'B' => handle_byte(app, b'j'),
        b'D' => handle_byte(app, b'h'),
        b'C' => handle_byte(app, b'l'),
        _ => false,
    }
}

/// Esc on the shelf: close the stacked detail pane.
pub(crate) fn handle_escape(app: &mut App) -> bool {
    if app.jobs.narrow() && app.jobs.detail_open() {
        app.jobs.close_detail();
        return true;
    }
    false
}
