//! Session-side orchestration, once per world tick: read the gate once
//! the opening sequence is over, open the moment the login owes, stamp
//! the account so no login shows it again, count it. `/comeback N` opens
//! one the same way without the stamp or the count.

use chrono::Utc;
use late_core::models::user::ComebackMark;

use crate::app::state::App;

use super::state::{Arrival, Beat, Moment, Standing, Start, due_moment};

/// The box is the last thing in a session's opening sequence, where the
/// paper's login pop would be: after the splash and after a tour, which
/// captures keys and must not end up under it. The gate is read here and
/// not at `App::new` because what the session has (a paired client) is
/// only certain by now. Returns whether something
/// render-visible changed.
pub(crate) fn tick(app: &mut App) -> bool {
    let opening_done = !app.show_splash && app.clubhouse.tutorial_settled();
    if !opening_done {
        return false;
    }
    let Some(arrival) = app.comeback.take_arrival() else {
        return false;
    };
    let mark = match arrival {
        Arrival::Login(mark) => mark,
        Arrival::NotALogin => return false,
    };
    let standing = Standing {
        hears: app.paired_client_state().is_some(),
    };
    let Some(moment) = due_moment(
        mark,
        app.account_created.date_naive(),
        Utc::now().date_naive(),
        standing,
    ) else {
        return false;
    };
    show(app, moment, Start::Login);
    true
}

/// Open a moment. A login start stamps the account with it as of today, so
/// a dropped connection never replays it, counts it shown, and takes the
/// paper's place: one box after the splash, never two.
pub(crate) fn show(app: &mut App, moment: Moment, start: Start) {
    app.comeback.show(moment, start);
    match start {
        Start::Login => {
            crate::metrics::record_comeback(moment, Beat::Shown);
            app.paper.disarm_login_pop();
            app.profile_state.service().set_comeback_mark(
                app.user_id,
                ComebackMark {
                    step: moment.step(),
                    day: Utc::now().date_naive(),
                },
            );
        }
        Start::Command => {}
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
