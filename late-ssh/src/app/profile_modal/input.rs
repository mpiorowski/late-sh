use crate::app::{
    input::{MouseEventKind, ParsedInput},
    state::App,
};

pub fn handle_input(app: &mut App, event: ParsedInput) {
    if is_close_event(&event) {
        close(app);
        return;
    }

    match event {
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => {
            app.profile_modal_state.scroll_by(1);
        }
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => {
            app.profile_modal_state.scroll_by(-1);
        }
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => app.profile_modal_state.scroll_by(-3),
            MouseEventKind::ScrollDown => app.profile_modal_state.scroll_by(3),
            _ => {}
        },
        ParsedInput::PageDown => {
            let step = (app.size.1 / 2).max(1) as i16;
            app.profile_modal_state.scroll_by(step);
        }
        ParsedInput::PageUp => {
            let step = (app.size.1 / 2).max(1) as i16;
            app.profile_modal_state.scroll_by(-step);
        }
        ParsedInput::Byte(b'r' | b'R') | ParsedInput::Char('r' | 'R') => {
            send_friend_request(app);
        }
        ParsedInput::Byte(b'a' | b'A') | ParsedInput::Char('a' | 'A') => {
            accept_friend_request(app);
        }
        ParsedInput::Byte(b'x' | b'X') | ParsedInput::Char('x' | 'X') => {
            decline_or_cancel(app);
        }
        ParsedInput::Byte(b'u' | b'U') | ParsedInput::Char('u' | 'U') => {
            unfriend(app);
        }
        _ => {}
    }
}

fn viewed_target(app: &App) -> Option<(uuid::Uuid, String)> {
    let target = app.profile_modal_state.viewed_user_id()?;
    if target == app.user_id {
        return None;
    }
    Some((target, app.profile_modal_state.viewed_name()))
}

fn send_friend_request(app: &mut App) {
    use crate::app::common::primitives::Banner;
    use late_core::models::friendship::FriendshipStatus;
    let Some((target, name)) = viewed_target(app) else {
        return;
    };
    match app.friends_state.local_status(target) {
        FriendshipStatus::None => {
            app.friends_state
                .service()
                .send_request_task(app.user_id, target, name);
        }
        FriendshipStatus::OutgoingPending => {
            app.banner = Some(Banner::success(&format!(
                "Already waiting on {name} to accept."
            )));
        }
        FriendshipStatus::IncomingPending => {
            // Press `a` to accept, not `r` to spam another request.
            app.banner = Some(Banner::success(&format!(
                "{name} already sent you a request — press `a` to accept."
            )));
        }
        FriendshipStatus::Friends => {
            app.banner = Some(Banner::success(&format!(
                "You and {name} are already friends."
            )));
        }
    }
}

fn accept_friend_request(app: &mut App) {
    use late_core::models::friendship::FriendshipStatus;
    let Some((target, name)) = viewed_target(app) else {
        return;
    };
    if app.friends_state.local_status(target) == FriendshipStatus::IncomingPending {
        app.friends_state
            .service()
            .accept_task(app.user_id, target, name);
    }
}

fn decline_or_cancel(app: &mut App) {
    use late_core::models::friendship::FriendshipStatus;
    let Some((target, name)) = viewed_target(app) else {
        return;
    };
    let status = app.friends_state.local_status(target);
    if matches!(
        status,
        FriendshipStatus::IncomingPending | FriendshipStatus::OutgoingPending
    ) {
        app.friends_state
            .service()
            .decline_or_cancel_task(app.user_id, target, name);
    }
}

fn unfriend(app: &mut App) {
    use late_core::models::friendship::FriendshipStatus;
    let Some((target, name)) = viewed_target(app) else {
        return;
    };
    if app.friends_state.local_status(target) == FriendshipStatus::Friends {
        app.friends_state
            .service()
            .unfriend_task(app.user_id, target, name);
    }
}

pub fn handle_escape(app: &mut App) {
    close(app);
}

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(
        event,
        ParsedInput::Byte(b'q' | b'Q' | 0x1B) | ParsedInput::Char('q' | 'Q')
    )
}

fn close(app: &mut App) {
    app.show_profile_modal = false;
    app.profile_modal_state.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_keys_include_printable_q_variants() {
        assert!(is_close_event(&ParsedInput::Char('q')));
        assert!(is_close_event(&ParsedInput::Char('Q')));
        assert!(is_close_event(&ParsedInput::Byte(b'q')));
        assert!(is_close_event(&ParsedInput::Byte(b'Q')));
        assert!(is_close_event(&ParsedInput::Byte(0x1B)));
        assert!(!is_close_event(&ParsedInput::Char('j')));
    }
}
