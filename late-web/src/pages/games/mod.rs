//! late.sh's games on the public web: the week's scoreboard as a card,
//! the picture the herald posts on Sundays (`late-ssh/src/app/herald`).
//! One line per game anyone played, under its shelf (the arcade, the
//! Lobby, the doors), with everyone's plays of the week added up, from
//! `leaderboard::week_tallies`. No page and no row: the card is drawn on
//! request from the games' own tables; the terminal is where the boards
//! live.

use anyhow::Context;
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::NaiveDate;
use late_core::models::leaderboard::{DayTally, week_first_day, week_tallies};

use crate::{
    AppState,
    error::AppError,
    metrics,
    pages::shared::{
        board_png,
        card::{self, CardLine, Ink},
    },
};

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/games/week/{last}/image.png", get(week_image_handler))
}

/// The game's name, then its plays right-aligned: the two columns of a
/// scoreboard line inside `card::CARD_COLUMNS`. Totals, no player: the
/// card is public and the names stay on the terminal's boards.
const GAME_COLUMNS: usize = 18;
const VALUE_COLUMNS: usize = card::CARD_COLUMNS - GAME_COLUMNS;

/// The week ending on `last` (`YYYY-MM-DD`) as a card. A path that is not
/// a date, or a week nobody played, is 404.
#[tracing::instrument(skip(state))]
async fn week_image_handler(
    State(state): State<AppState>,
    Path(last): Path<String>,
) -> Result<Response, AppError> {
    metrics::record_page_view("games_week_image", false);

    let Ok(last) = NaiveDate::parse_from_str(&last, "%Y-%m-%d") else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let client = state
        .db
        .get()
        .await
        .context("failed to get db client for the games week")?;
    let tallies = week_tallies(&client, last)
        .await
        .context("failed to read the games week")?;
    if tallies.is_empty() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let png = board_png::render_blocking(card::canvas(&week_card(last, &tallies)))
        .await
        .context("failed to render the games week")?;
    Ok(board_png::response(png))
}

/// The card's lines: the masthead, the week, a section per shelf
/// (`leaderboard::Shelf`, in the order `week_tallies` lists the rosters)
/// with a line per game and its plays, the way in.
pub(crate) fn week_card(last: NaiveDate, tallies: &[DayTally]) -> Vec<CardLine> {
    let mut lines = vec![
        CardLine::new(Ink::Title, "THE LATE.SH GAMES"),
        CardLine::new(
            Ink::Dim,
            format!(
                "everything played, {} to {}",
                week_first_day(last).format("%-d %B"),
                last.format("%-d %B %Y"),
            ),
        ),
    ];
    let mut shelf = None;
    for tally in tallies {
        let this = tally.game.shelf();
        if shelf != Some(this) {
            lines.push(CardLine::blank());
            lines.push(CardLine::new(Ink::Heading, this.heading()));
            shelf = Some(this);
        }
        lines.push(CardLine::new(
            Ink::Body,
            format!(
                "{:<GAME_COLUMNS$}{:>VALUE_COLUMNS$}",
                tally.game.title(),
                tally.shown_plays(),
            ),
        ));
    }
    lines.push(CardLine::blank());
    lines.push(CardLine::new(Ink::Dim, "ssh late.sh"));
    lines
}

#[cfg(test)]
mod games_test;
