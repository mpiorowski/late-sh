//! Parsing of FotMob's World Cup league page into a [`WorldCupSnapshot`].
//!
//! FotMob's JSON API now requires a signed request header, so — like golazo —
//! we scrape the Next.js page HTML and read the JSON embedded in the
//! `__NEXT_DATA__` script tag (`props.pageProps`). The page URL is
//! `https://www.fotmob.com/leagues/77/overview/world-cup` (league 77 = FIFA
//! World Cup).
//!
//! Everything here is best-effort and permissive: missing or renamed fields
//! degrade to empty/partial data rather than failing, so a FotMob shape change
//! shows a thinner HUD instead of a panic or a hard error.

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::model::{
    BracketRound, Group, Match, MatchStatus, Matchup, Qual, TeamRow, Winner, WorldCupSnapshot,
};

/// The FotMob World Cup overview page.
pub const WORLD_CUP_URL: &str = "https://www.fotmob.com/leagues/77/overview/world-cup";

// ---- raw __NEXT_DATA__ shapes (only the fields we consume) -----------------

#[derive(Debug, Deserialize)]
struct RawPage {
    #[serde(default)]
    props: RawProps,
}

#[derive(Debug, Default, Deserialize)]
struct RawProps {
    #[serde(default, rename = "pageProps")]
    page_props: RawPageProps,
}

#[derive(Debug, Default, Deserialize)]
struct RawPageProps {
    #[serde(default)]
    table: Vec<RawTableWrap>,
    #[serde(default)]
    playoff: RawPlayoff,
    #[serde(default)]
    overview: RawOverview,
}

#[derive(Debug, Default, Deserialize)]
struct RawTableWrap {
    #[serde(default)]
    data: RawTableData,
}

#[derive(Debug, Default, Deserialize)]
struct RawTableData {
    #[serde(default)]
    tables: Vec<RawGroupTable>,
}

#[derive(Debug, Default, Deserialize)]
struct RawGroupTable {
    #[serde(default, rename = "leagueName")]
    league_name: String,
    #[serde(default)]
    table: RawTableAll,
}

#[derive(Debug, Default, Deserialize)]
struct RawTableAll {
    #[serde(default)]
    all: Vec<RawTeamRow>,
}

#[derive(Debug, Default, Deserialize)]
struct RawTeamRow {
    #[serde(default)]
    name: String,
    #[serde(default)]
    played: u32,
    #[serde(default, rename = "goalConDiff")]
    goal_con_diff: i32,
    #[serde(default)]
    pts: u32,
    #[serde(default, rename = "qualColor")]
    qual_color: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawPlayoff {
    #[serde(default)]
    rounds: Vec<RawRound>,
    #[serde(default, rename = "bronzeFinal")]
    bronze_final: Option<RawMatchup>,
}

#[derive(Debug, Default, Deserialize)]
struct RawRound {
    #[serde(default)]
    stage: String,
    #[serde(default)]
    matchups: Vec<RawMatchup>,
}

#[derive(Debug, Default, Deserialize)]
struct RawMatchup {
    #[serde(default, rename = "homeTeam")]
    home_team: String,
    #[serde(default, rename = "awayTeam")]
    away_team: String,
    #[serde(default, rename = "homeTeamShortName")]
    home_short: String,
    #[serde(default, rename = "awayTeamShortName")]
    away_short: String,
    #[serde(default, rename = "homeTeamId")]
    home_team_id: Option<i64>,
    #[serde(default, rename = "awayTeamId")]
    away_team_id: Option<i64>,
    #[serde(default, rename = "homeScore")]
    home_score: Option<i32>,
    #[serde(default, rename = "awayScore")]
    away_score: Option<i32>,
    #[serde(default)]
    winner: Option<i64>,
    #[serde(default, rename = "tbdTeam1")]
    tbd_team1: bool,
    #[serde(default, rename = "tbdTeam2")]
    tbd_team2: bool,
}

#[derive(Debug, Default, Deserialize)]
struct RawOverview {
    #[serde(default, rename = "selectedSeason")]
    selected_season: String,
    #[serde(default, rename = "leagueOverviewMatches")]
    matches: Vec<RawMatch>,
}

#[derive(Debug, Default, Deserialize)]
struct RawMatch {
    #[serde(default)]
    home: RawSide,
    #[serde(default)]
    away: RawSide,
    #[serde(default)]
    status: RawStatus,
}

#[derive(Debug, Default, Deserialize)]
struct RawSide {
    #[serde(default)]
    name: String,
    #[serde(default)]
    score: Option<i32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawStatus {
    #[serde(default, rename = "utcTime")]
    utc_time: Option<String>,
    #[serde(default)]
    finished: bool,
    #[serde(default)]
    started: bool,
    #[serde(default)]
    cancelled: bool,
    #[serde(default)]
    reason: Option<RawReason>,
}

#[derive(Debug, Default, Deserialize)]
struct RawReason {
    #[serde(default)]
    short: Option<String>,
}

// ---- public entry points ---------------------------------------------------

/// Slices the JSON out of the `__NEXT_DATA__` script tag, or `None` if the
/// page doesn't contain it.
pub fn extract_next_data(html: &str) -> Option<&str> {
    let marker = html.find("__NEXT_DATA__")?;
    let start = html[marker..].find('>')? + marker + 1;
    let end = html[start..].find("</script>")? + start;
    Some(html[start..end].trim())
}

/// Parses a fetched World Cup page into a snapshot. Returns `None` only when
/// the page is unrecognizable (no `__NEXT_DATA__` or invalid JSON); a valid
/// page with thin data yields a sparse-but-valid snapshot.
pub fn parse_page(html: &str) -> Option<WorldCupSnapshot> {
    let json = extract_next_data(html)?;
    let page: RawPage = serde_json::from_str(json).ok()?;
    Some(build_snapshot(page.props.page_props))
}

fn build_snapshot(pp: RawPageProps) -> WorldCupSnapshot {
    let groups = pp
        .table
        .into_iter()
        .flat_map(|w| w.data.tables)
        .filter_map(convert_group)
        .collect();

    let mut bracket: Vec<BracketRound> = pp
        .playoff
        .rounds
        .into_iter()
        .filter_map(convert_round)
        .collect();
    if let Some(bronze) = pp.playoff.bronze_final {
        bracket.push(BracketRound {
            label: "Third place".to_string(),
            matchups: vec![convert_matchup(bronze)],
        });
    }

    let matches = pp.overview.matches.into_iter().map(convert_match).collect();

    WorldCupSnapshot {
        season: pp.overview.selected_season,
        groups,
        matches,
        bracket,
        fetched_at: None,
        stale: false,
    }
}

// ---- conversions -----------------------------------------------------------

fn convert_group(t: RawGroupTable) -> Option<Group> {
    let letter = group_letter(&t.league_name)?;
    let rows = t
        .table
        .all
        .into_iter()
        .filter(|r| !r.name.trim().is_empty())
        .map(|r| TeamRow {
            name: r.name,
            played: r.played,
            goal_diff: r.goal_con_diff,
            points: r.pts,
            qual: classify_qual(r.qual_color.as_deref()),
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return None;
    }
    Some(Group { letter, rows })
}

/// "Grp. A" → "A". Only a single A–Z letter qualifies, which filters out the
/// pseudo-tables FotMob ships alongside the real groups ("Best 3rd placed
/// teams", "Qualified teams").
fn group_letter(league_name: &str) -> Option<String> {
    let token = league_name.split_whitespace().last()?;
    let mut chars = token.chars();
    let c = chars.next()?;
    if chars.next().is_none() && c.is_ascii_alphabetic() {
        Some(c.to_ascii_uppercase().to_string())
    } else {
        None
    }
}

/// Maps FotMob's `qualColor` hex to a qualification tier. Green hues mark a
/// direct advancing slot; any other non-empty color is a contended slot.
fn classify_qual(color: Option<&str>) -> Qual {
    let color = color.map(str::trim).unwrap_or("");
    if color.is_empty() {
        return Qual::None;
    }
    match parse_hex(color) {
        Some((r, g, b)) if g > r && g > b => Qual::Direct,
        _ => Qual::Playoff,
    }
}

fn parse_hex(color: &str) -> Option<(u8, u8, u8)> {
    let h = color.strip_prefix('#').unwrap_or(color);
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some((r, g, b))
}

fn convert_round(r: RawRound) -> Option<BracketRound> {
    if r.matchups.is_empty() {
        return None;
    }
    Some(BracketRound {
        label: round_label(&r.stage),
        matchups: r.matchups.into_iter().map(convert_matchup).collect(),
    })
}

fn round_label(stage: &str) -> String {
    match stage {
        "1/16" => "Round of 32",
        "1/8" => "Round of 16",
        "1/4" => "Quarter-finals",
        "1/2" => "Semi-finals",
        "final" => "Final",
        "bronze" => "Third place",
        other => other,
    }
    .to_string()
}

fn convert_matchup(m: RawMatchup) -> Matchup {
    let winner = match m.winner {
        Some(w) if Some(w) == m.home_team_id => Winner::Home,
        Some(w) if Some(w) == m.away_team_id => Winner::Away,
        _ => Winner::None,
    };
    Matchup {
        home_name: m.home_team,
        away_name: m.away_team,
        home_short: m.home_short,
        away_short: m.away_short,
        home_score: m.home_score,
        away_score: m.away_score,
        winner,
        tbd: m.tbd_team1 || m.tbd_team2,
    }
}

fn convert_match(m: RawMatch) -> Match {
    let status = if m.status.cancelled {
        MatchStatus::Cancelled
    } else if m.status.finished {
        MatchStatus::Finished
    } else if m.status.started {
        MatchStatus::Live
    } else {
        MatchStatus::Upcoming
    };
    // Scores are only meaningful once the ball is rolling; upcoming fixtures
    // carry a placeholder 0 we don't want to display.
    let scored = matches!(status, MatchStatus::Live | MatchStatus::Finished);
    Match {
        home: m.home.name,
        away: m.away.name,
        home_score: scored.then_some(m.home.score.unwrap_or(0)),
        away_score: scored.then_some(m.away.score.unwrap_or(0)),
        kickoff: m.status.utc_time.as_deref().and_then(parse_utc),
        status,
        reason_short: m
            .status
            .reason
            .and_then(|r| r.short)
            .filter(|s| !s.is_empty()),
    }
}

fn parse_utc(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}


