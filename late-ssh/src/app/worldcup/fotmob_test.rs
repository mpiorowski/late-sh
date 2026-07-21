use crate::app::worldcup::fotmob::*;
use crate::app::worldcup::model::{Qual, Winner};

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
