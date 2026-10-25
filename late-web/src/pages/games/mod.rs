//! late.sh's games on the public web: the week's scoreboard as a card,
//! the picture the herald posts on Sundays (`late-ssh/src/app/herald`).
//! One line per game anyone played, under its shelf (the arcade, the
//! Lobby, the doors), its best of the week and who, from
//! `leaderboard::week_bests`. No page; the terminal is where the boards
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
use late_core::models::leaderboard::{WeekBest, WeekGame, week_bests, week_first_day};

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

/// The game's name, then the player, then the value right-aligned: the
/// three columns of a scoreboard line inside `card::CARD_COLUMNS`.
const GAME_COLUMNS: usize = 18;
const NAME_COLUMNS: usize = 30;
const VALUE_COLUMNS: usize = card::CARD_COLUMNS - GAME_COLUMNS - NAME_COLUMNS;

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
    let bests = week_bests(&client, last)
        .await
        .context("failed to read the games week")?;
    if bests.is_empty() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let png = board_png::render_blocking(card::canvas(&week_card(last, &bests)))
        .await
        .context("failed to render the games week")?;
    Ok(board_png::response(png))
}

/// The card's sections, in the order `week_bests` lists the rosters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shelf {
    Arcade,
    Lobby,
    Doors,
}

impl Shelf {
    fn of(game: WeekGame) -> Self {
        match game {
            WeekGame::Daily(_) | WeekGame::Score(_) => Self::Arcade,
            WeekGame::Lobby(_) => Self::Lobby,
            WeekGame::Door(_) => Self::Doors,
        }
    }

    fn heading(self) -> &'static str {
        match self {
            Self::Arcade => "ARCADE",
            Self::Lobby => "LOBBY",
            Self::Doors => "DOORS",
        }
    }
}

/// The card's lines: the masthead, the week, a section per shelf with a
/// line per game, the way in.
pub(crate) fn week_card(last: NaiveDate, bests: &[WeekBest]) -> Vec<CardLine> {
    let mut lines = vec![
        CardLine::new(Ink::Title, "THE LATE.SH GAMES"),
        CardLine::new(
            Ink::Dim,
            format!(
                "the week's best, {} to {}",
                week_first_day(last).format("%-d %B"),
                last.format("%-d %B %Y"),
            ),
        ),
    ];
    let mut shelf = None;
    for best in bests {
        let this = Shelf::of(best.game);
        if shelf != Some(this) {
            lines.push(CardLine::blank());
            lines.push(CardLine::new(Ink::Heading, this.heading()));
            shelf = Some(this);
        }
        let name: String = format!("~{}", best.username)
            .chars()
            .take(NAME_COLUMNS - 1)
            .collect();
        lines.push(CardLine::new(
            Ink::Body,
            format!(
                "{:<GAME_COLUMNS$}{:<NAME_COLUMNS$}{:>VALUE_COLUMNS$}",
                best.game.title(),
                name,
                best.shown_value(),
            ),
        ));
    }
    lines.push(CardLine::blank());
    lines.push(CardLine::new(Ink::Dim, "ssh late.sh"));
    lines
}

#[cfg(test)]
mod games_test;
