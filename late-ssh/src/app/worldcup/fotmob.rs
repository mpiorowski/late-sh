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

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_HTML: &str = r##"<html><body>
<script id="__NEXT_DATA__" type="application/json">
{"props":{"pageProps":{
  "table":[{"data":{"tables":[
    {"leagueName":"Grp. A","table":{"all":[
      {"name":"Mexico","played":3,"wins":3,"draws":0,"losses":0,"goalConDiff":6,"pts":9,"qualColor":"#2AD572","scoresStr":"6-0"},
      {"name":"South Korea","played":3,"goalConDiff":-1,"pts":3,"qualColor":"#FFD908"},
      {"name":"Czechia","played":3,"goalConDiff":-4,"pts":1,"qualColor":null}
    ]}},
    {"leagueName":"Best 3rd placed teams","table":{"all":[]}}
  ]}}],
  "playoff":{"rounds":[
    {"stage":"1/16","matchups":[
      {"homeTeam":"Germany","awayTeam":"Paraguay","homeTeamShortName":"GER","awayTeamShortName":"PAR","homeTeamId":8570,"awayTeamId":6724,"homeScore":1,"awayScore":1,"winner":6724,"tbdTeam1":false,"tbdTeam2":false},
      {"homeTeam":"Winner SF 1","awayTeam":"Winner SF 2","homeTeamShortName":"WS1","awayTeamShortName":"WS2","tbdTeam1":true,"tbdTeam2":true}
    ]}
  ],"bronzeFinal":null},
  "overview":{"selectedSeason":"2026","leagueOverviewMatches":[
    {"home":{"name":"Mexico","score":2},"away":{"name":"South Africa","score":0},"status":{"utcTime":"2026-06-11T19:00:00Z","finished":true,"started":true,"cancelled":false,"reason":{"short":"FT"}}},
    {"home":{"name":"Brazil","score":1},"away":{"name":"Norway","score":1},"status":{"utcTime":"2026-06-30T17:00:00.000Z","finished":false,"started":true,"cancelled":false}},
    {"home":{"name":"Ivory Coast","score":0},"away":{"name":"Norway","score":0},"status":{"utcTime":"2026-07-01T17:00:00.000Z","finished":false,"started":false,"cancelled":false}}
  ]}
}}}
</script>
</body></html>"##;

    #[test]
    fn extract_next_data_pulls_script_json() {
        let json = extract_next_data(FIXTURE_HTML).expect("script json");
        assert!(json.starts_with('{'));
        assert!(json.contains("pageProps"));
        assert!(!json.contains("</script>"));
    }

    #[test]
    fn extract_next_data_missing_returns_none() {
        assert!(extract_next_data("<html>no next data here</html>").is_none());
    }

    #[test]
    fn parses_groups_and_filters_pseudo_tables() {
        let snap = parse_page(FIXTURE_HTML).expect("snapshot");
        assert_eq!(snap.season, "2026");
        // Only "Grp. A" survives; "Best 3rd placed teams" is filtered.
        assert_eq!(snap.groups.len(), 1);
        let g = &snap.groups[0];
        assert_eq!(g.letter, "A");
        assert_eq!(g.rows.len(), 3);
        assert_eq!(g.rows[0].name, "Mexico");
        assert_eq!(g.rows[0].points, 9);
        assert_eq!(g.rows[0].goal_diff, 6);
        assert_eq!(g.rows[0].qual, Qual::Direct); // green
        assert_eq!(g.rows[1].qual, Qual::Playoff); // amber
        assert_eq!(g.rows[2].qual, Qual::None); // null
    }

    #[test]
    fn classifies_match_status_and_scores() {
        let snap = parse_page(FIXTURE_HTML).expect("snapshot");
        let finished: Vec<_> = snap.recent_finished().collect();
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].home, "Mexico");
        assert_eq!(finished[0].home_score, Some(2));
        assert_eq!(finished[0].reason_short.as_deref(), Some("FT"));

        let live: Vec<_> = snap.live().collect();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].home, "Brazil");
        assert_eq!(live[0].home_score, Some(1));

        let upcoming: Vec<_> = snap.upcoming().collect();
        assert_eq!(upcoming.len(), 1);
        assert_eq!(upcoming[0].home, "Ivory Coast");
        // Upcoming fixtures must not show a (placeholder) score.
        assert_eq!(upcoming[0].home_score, None);
        assert!(upcoming[0].kickoff.is_some());
    }

    #[test]
    fn parses_bracket_with_winner_and_tbd() {
        let snap = parse_page(FIXTURE_HTML).expect("snapshot");
        assert_eq!(snap.bracket.len(), 1);
        let round = &snap.bracket[0];
        assert_eq!(round.label, "Round of 32");
        assert_eq!(round.matchups.len(), 2);

        let decided = &round.matchups[0];
        assert_eq!(decided.home_short, "GER");
        assert_eq!(decided.winner, Winner::Away); // Paraguay won
        assert!(!decided.tbd);

        let pending = &round.matchups[1];
        assert!(pending.tbd);
        assert_eq!(pending.winner, Winner::None);
    }

    #[test]
    fn unparseable_page_is_none() {
        assert!(parse_page("<html>nothing</html>").is_none());
    }
}
