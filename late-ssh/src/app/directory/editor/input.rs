use late_core::models::work_profile::WorkProfile;
use uuid::Uuid;

use super::state::{
    EscapeOutcome, Field, FieldKind, Page, ProjectRow, ProjectsView, Save, Scope,
    about_onto_profile, projects_of,
};
use crate::app::common::textarea_input::{
    EditOutcome, handle_multiline_edit, handle_single_line_edit,
};
use crate::app::input::{MouseButton, MouseEventKind, ParsedInput};
use crate::app::state::App;
use crate::app::tag_picker::{self, state::TagPickerTarget};

const CTRL_S: u8 = 0x13;

/// The viewer's own card, if they have one, and their settings profile:
/// what every own-profile open seeds from.
fn own_card(app: &App) -> Option<WorkProfile> {
    app.chat
        .work
        .card_of(app.user_id)
        .map(|item| item.profile.clone())
}

/// `w` on page 5, or `e` on your own card: the editor on the card page.
pub(crate) fn open_own(app: &mut App, page: Page) {
    let card = own_card(app);
    let profile = app.profile_state.profile().clone();
    app.directory_editor
        .open_own(app.user_id, card.as_ref(), &profile, page);
}

/// `i` on page 5: the editor on a blank project form.
pub(crate) fn open_own_new_project(app: &mut App) {
    let card = own_card(app);
    let profile = app.profile_state.profile().clone();
    app.directory_editor
        .open_own_new_project(app.user_id, card.as_ref(), &profile);
}

/// `e` on a project: yours opens inside your profile, someone else's (for a
/// moderator) opens on its own. Returns false when it is not yours to edit.
pub(crate) fn open_project(app: &mut App, project_id: Uuid) -> bool {
    let Some(item) = app.chat.showcase.project(project_id) else {
        return false;
    };
    let owner = item.showcase.user_id;
    let username = item.author_username.clone();
    let row = ProjectRow::from_item(item);
    if owner == app.user_id {
        let card = own_card(app);
        let profile = app.profile_state.profile().clone();
        app.directory_editor
            .open_own_project(app.user_id, card.as_ref(), &profile, &row);
        return true;
    }
    if !app.chat.showcase.is_admin() {
        return false;
    }
    app.directory_editor
        .open_project_of(app.user_id, owner, username, &row);
    true
}

/// `e` on a card: yours opens your profile, someone else's (for a
/// moderator) opens that card alone. Returns false when it is not yours.
pub(crate) fn open_card(app: &mut App, card_id: Uuid) -> bool {
    let Some(item) = app.chat.work.card(card_id) else {
        return false;
    };
    if item.profile.user_id == app.user_id {
        open_own(app, Page::Card);
        return true;
    }
    if !app.chat.work.is_admin() {
        return false;
    }
    let owner = item.profile.user_id;
    let username = item.author_username.clone();
    let card = item.profile.clone();
    app.directory_editor
        .open_card_of(app.user_id, owner, username, &card);
    true
}

/// Route a key to the open editor. Order: the discard question, then a row
/// being typed, then the page's own keys.
pub(crate) fn handle_input(app: &mut App, event: &ParsedInput) {
    if let ParsedInput::Mouse(mouse) = event {
        handle_mouse(app, *mouse);
        return;
    }
    app.directory_editor.mouse.reveal_selection();

    // The parser hands printable keys over as `Char`; the row and list keys
    // below read bytes, so fold ASCII back before matching. A row being
    // typed into gets the original event, which the textarea helpers read.
    let key = match event {
        ParsedInput::Char(ch) if ch.is_ascii() => ParsedInput::Byte(*ch as u8),
        other => other.clone(),
    };

    if app.directory_editor.confirm_discard() {
        match key {
            ParsedInput::Byte(b'y' | b'Y') => {
                let _ = app.directory_editor.confirm_discard_yes();
            }
            ParsedInput::Byte(b'n' | b'N' | 0x1B) => {
                app.directory_editor.confirm_discard_no();
            }
            _ => {}
        }
        return;
    }

    if matches!(event, ParsedInput::Byte(CTRL_S) | ParsedInput::AltS) {
        save(app);
        return;
    }

    if app.directory_editor.editing() {
        handle_typing(app, event);
        return;
    }

    match app.directory_editor.page() {
        Page::Projects
            if matches!(
                app.directory_editor.projects_view(),
                ProjectsView::List { .. }
            ) =>
        {
            handle_project_list_key(app, &key);
        }
        Page::Card | Page::About | Page::Projects => handle_row_key(app, &key),
    }
}

fn handle_typing(app: &mut App, event: &ParsedInput) {
    let Some(field) = app.directory_editor.active_field() else {
        app.directory_editor.stop_editing();
        return;
    };
    if matches!(event, ParsedInput::Byte(b'\t')) {
        app.directory_editor.commit_and_advance(true);
        return;
    }
    if matches!(event, ParsedInput::BackTab) {
        app.directory_editor.commit_and_advance(false);
        return;
    }
    let max = field.max_len();
    let outcome = match field.kind() {
        FieldKind::Text => {
            handle_single_line_edit(app.directory_editor.field_mut(field), event, max)
        }
        FieldKind::Multi => {
            handle_multiline_edit(app.directory_editor.field_mut(field), event, max)
        }
        FieldKind::Choice | FieldKind::Tags => EditOutcome::Ignored,
    };
    match outcome {
        EditOutcome::Handled => {}
        EditOutcome::Submit => app.directory_editor.commit_and_advance(true),
        EditOutcome::Cancel => app.directory_editor.stop_editing(),
        EditOutcome::Ignored => {
            // Up/down leave a one-line row for its neighbour; in a block they
            // moved the cursor already.
            if field.kind() == FieldKind::Text {
                match event {
                    ParsedInput::Arrow(b'A') => app.directory_editor.commit_and_advance(false),
                    ParsedInput::Arrow(b'B') => app.directory_editor.commit_and_advance(true),
                    _ => {}
                }
            }
        }
    }
}

fn handle_row_key(app: &mut App, event: &ParsedInput) {
    let editor = &mut app.directory_editor;
    match event {
        ParsedInput::Byte(b'\t') => editor.switch_page(true),
        ParsedInput::BackTab => editor.switch_page(false),
        ParsedInput::Byte(b'j' | b'J') | ParsedInput::Arrow(b'B') => editor.move_row(1),
        ParsedInput::Byte(b'k' | b'K') | ParsedInput::Arrow(b'A') => editor.move_row(-1),
        ParsedInput::Byte(b'h' | b'H') | ParsedInput::Arrow(b'D') => editor.cycle_choice(false),
        ParsedInput::Byte(b'l' | b'L') | ParsedInput::Arrow(b'C') => editor.cycle_choice(true),
        // A tag row opens the picker; every other row edits in place.
        ParsedInput::Byte(b'\r' | b'e' | b'E' | b'i' | b'I') => match editor.active_field() {
            Some(Field::Skills) => tag_picker::input::open(app, TagPickerTarget::EditorSkills),
            Some(Field::Langs) => tag_picker::input::open(app, TagPickerTarget::EditorLangs),
            Some(_) | None => editor.start_editing(),
        },
        ParsedInput::Byte(0x1B) => {
            let _ = editor.escape();
        }
        _ => {}
    }
}

fn handle_project_list_key(app: &mut App, event: &ParsedInput) {
    let projects = projects_of(app.chat.showcase.all_items(), app.user_id);
    let len = projects.len();
    match event {
        ParsedInput::Byte(b'\t') => app.directory_editor.switch_page(true),
        ParsedInput::BackTab => app.directory_editor.switch_page(false),
        ParsedInput::Byte(b'j' | b'J') | ParsedInput::Arrow(b'B') => {
            app.directory_editor.move_project_selection(1, len);
        }
        ParsedInput::Byte(b'k' | b'K') | ParsedInput::Arrow(b'A') => {
            app.directory_editor.move_project_selection(-1, len);
        }
        ParsedInput::Byte(b'a' | b'A' | b'i' | b'I' | b'n' | b'N') => {
            app.directory_editor.start_new_project();
        }
        ParsedInput::Byte(b'\r' | b'e' | b'E') => {
            match projects.get(app.directory_editor.project_selected()) {
                Some(project) => app.directory_editor.start_editing_project(project),
                // Nothing to edit yet: Enter on the empty list adds.
                None => app.directory_editor.start_new_project(),
            }
        }
        ParsedInput::Byte(b'd' | b'D') => {
            if let Some(project) = projects.get(app.directory_editor.project_selected()) {
                let id = project.id;
                if let Some(banner) = app.chat.showcase.delete_project(id) {
                    app.banner = Some(banner);
                }
                app.directory_editor
                    .move_project_selection(0, len.saturating_sub(1));
            }
        }
        ParsedInput::Byte(0x1B) => {
            let _ = app.directory_editor.escape();
        }
        _ => {}
    }
}

/// Esc from the app-level escape dispatch.
pub(crate) fn handle_escape(app: &mut App) -> EscapeOutcome {
    if app.directory_editor.confirm_discard() {
        app.directory_editor.confirm_discard_no();
        return EscapeOutcome::Stayed;
    }
    app.directory_editor.escape()
}

/// Ctrl+S: validate, then hand each save to its service. The card and the
/// project go through the chat-side feed states (their events raise the
/// banners); the about page goes through the profile service the settings
/// modal uses, so both doors write the same row.
fn save(app: &mut App) {
    let Ok(saves) = app.directory_editor.save() else {
        return;
    };
    for save in saves {
        match save {
            Save::Card { params, editing } => app.chat.work.save(params, editing),
            Save::Project { params, editing } => app.chat.showcase.save(params, editing),
            Save::About(values) => {
                let params = about_onto_profile(&values, app.profile_state.profile());
                app.profile_state
                    .service()
                    .edit_profile(app.user_id, params);
            }
        }
    }
}

/// What the modal's title names: you, or the person a moderator is editing.
pub(crate) fn subject_label(scope: &Scope, viewer_name: &str) -> String {
    match scope {
        Scope::Own => format!("@{viewer_name}"),
        Scope::CardOf { username, .. } | Scope::ProjectOf { username, .. } => {
            format!("@{username}")
        }
    }
}

fn handle_mouse(app: &mut App, mouse: crate::app::input::MouseEvent) {
    use super::state::MouseTarget;
    if !app.interaction_mode.mouse_enabled() {
        return;
    }
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return;
    };
    // The discard question is answered with the keys.
    if app.directory_editor.confirm_discard() {
        return;
    }
    let target = app.directory_editor.mouse.target(x, y, app.size);
    match mouse.kind {
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let delta = if mouse.kind == MouseEventKind::ScrollUp {
                -3
            } else {
                3
            };
            match target {
                // A multiline row scrolls its own text under the wheel.
                Some(MouseTarget::Field(field)) if field.kind() == FieldKind::Multi => {
                    app.directory_editor.field_mut(field).scroll(
                        ratatui_textarea::Scrolling::Delta {
                            rows: delta as i16,
                            cols: 0,
                        },
                    );
                    app.directory_editor.mouse.invalidate();
                }
                _ => {
                    app.directory_editor.mouse.scroll(x, y, delta, app.size);
                }
            }
        }
        MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => {
            if !app.directory_editor.mouse.click_track(x, y, app.size) {
                match target {
                    Some(MouseTarget::Page(page)) => app.directory_editor.select_page(page),
                    Some(MouseTarget::Field(field)) => {
                        app.directory_editor.stop_editing();
                        if let Some(index) = app
                            .directory_editor
                            .fields()
                            .iter()
                            .position(|candidate| *candidate == field)
                        {
                            app.directory_editor.set_row(index);
                            match field {
                                Field::Skills => {
                                    tag_picker::input::open(app, TagPickerTarget::EditorSkills)
                                }
                                Field::Langs => {
                                    tag_picker::input::open(app, TagPickerTarget::EditorLangs)
                                }
                                _ => app.directory_editor.start_editing(),
                            }
                        }
                    }
                    Some(MouseTarget::Choice(field, forward)) => {
                        app.directory_editor.stop_editing();
                        if let Some(index) = app
                            .directory_editor
                            .fields()
                            .iter()
                            .position(|candidate| *candidate == field)
                        {
                            app.directory_editor.set_row(index);
                            app.directory_editor.cycle_choice(forward);
                        }
                    }
                    // A click selects the row, as the keys do; Edit and
                    // Delete then act on it, so a stray click cannot open
                    // or drop a project on its own.
                    Some(MouseTarget::Project(id)) => {
                        let projects = projects_of(app.chat.showcase.all_items(), app.user_id);
                        if let Some(index) = projects.iter().position(|project| project.id == id) {
                            app.directory_editor
                                .set_project_selection(index, projects.len());
                        }
                    }
                    Some(MouseTarget::Add) => {
                        app.directory_editor.start_new_project();
                        app.directory_editor.mouse.reset_pane(Page::Projects);
                    }
                    Some(MouseTarget::EditProject) => {
                        handle_project_list_key(app, &ParsedInput::Byte(b'e'))
                    }
                    Some(MouseTarget::DeleteProject) => {
                        handle_project_list_key(app, &ParsedInput::Byte(b'd'))
                    }
                    None => {}
                }
            }
            app.directory_editor.mouse.invalidate();
        }
        _ => {}
    }
}
