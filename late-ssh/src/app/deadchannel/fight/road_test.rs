use chrono::{Days, NaiveDate};

use super::{BRIGHT_NODES, FIGHT_STEPS, LANES, Mark, Node, Road, RoadRun, STEPS, Trace, road_for};

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, d).expect("a september day")
}

/// The road as it would be drawn: a row a lane, a mark a node.
fn picture(road: &Road) -> Vec<String> {
    (0..LANES)
        .map(|lane| {
            road.steps
                .iter()
                .map(|lanes| match lanes[lane] {
                    Node::Glyph => 'g',
                    Node::Bright => 'B',
                    Node::Rest => '+',
                    Node::Cache => '$',
                })
                .collect()
        })
        .collect()
}

/// One day's road, whole. The road is a pure function of the date, so
/// this picture is the same for every runner, on every replica, forever.
/// A change here moves every day's road under everybody; read it.
#[test]
fn one_days_road_holds_still() {
    assert_eq!(
        picture(&road_for(day(24))),
        PINNED_ROAD,
        "the road of 2026-09-24"
    );
    assert_eq!(road_for(day(24)), road_for(day(24)));
    assert_ne!(
        road_for(day(24)),
        road_for(day(25)),
        "a new day, a new road"
    );
}

/// What every road is, whatever the date: the same fights on every path
/// (the pace of the climb is the rations', never the route's), a glyph to
/// open the day and one to close it, room to breathe between fights, a
/// real choice on every quiet step, and two bright glyphs to walk to or
/// around.
#[test]
fn every_road_has_the_same_shape() {
    let mut roads = std::collections::HashSet::new();
    for offset in 0..800 {
        let date = day(1) + Days::new(offset);
        let road = road_for(date);
        roads.insert(picture(&road));
        let fights: Vec<bool> = road
            .steps
            .iter()
            .map(|lanes| lanes.iter().all(|node| node.is_fight()))
            .collect();
        let quiet: Vec<bool> = road
            .steps
            .iter()
            .map(|lanes| lanes.iter().all(|node| !node.is_fight()))
            .collect();
        assert!(
            fights
                .iter()
                .zip(&quiet)
                .all(|(fight, quiet)| fight ^ quiet),
            "{date}: a step is all fights or none: {:?}",
            picture(&road)
        );
        assert_eq!(
            fights.iter().filter(|fight| **fight).count(),
            FIGHT_STEPS,
            "{date}"
        );
        assert!(fights[0] && fights[STEPS - 1], "{date}");
        assert!(
            road.steps[0].iter().all(|node| *node == Node::Glyph),
            "{date}: the day opens on a plain glyph"
        );
        for run in fights.windows(3) {
            assert!(
                run.iter().any(|fight| *fight) && run.iter().any(|fight| !*fight),
                "{date}: three of a kind in a row: {:?}",
                picture(&road)
            );
        }
        for (step, lanes) in road.steps.iter().enumerate() {
            if quiet[step] {
                assert!(
                    lanes.contains(&Node::Rest) && lanes.contains(&Node::Cache),
                    "{date}: step {} is no choice",
                    step + 1
                );
            }
            assert!(
                lanes.iter().filter(|node| **node == Node::Bright).count() <= 1,
                "{date}: a bright one can always be walked around"
            );
        }
        assert_eq!(
            road.steps
                .iter()
                .flatten()
                .filter(|node| **node == Node::Bright)
                .count(),
            BRIGHT_NODES,
            "{date}"
        );
    }
    assert!(
        roads.len() > 700,
        "{} distinct roads in 800 days",
        roads.len()
    );
}

#[test]
fn a_run_reaches_the_lane_it_stands_in_and_the_ones_beside_it() {
    let mut run = RoadRun::default();
    assert_eq!(run.lane(), None);
    assert_eq!(run.open_lanes(), vec![0, 1, 2], "every lane, before a step");

    run.path.push(Trace {
        lane: 0,
        mark: Mark::Fighting,
    });
    assert_eq!(run.open_lanes(), vec![0, 1]);
    assert!(!run.reaches(2));
    assert!(!run.reaches(3), "there is no fourth lane");

    // A fight's end marks the step it was fought on, once.
    run.settle_fight(Mark::Won);
    run.settle_fight(Mark::Fell);
    assert_eq!(run.path[0].mark, Mark::Won);

    run.path.push(Trace {
        lane: 1,
        mark: Mark::Cached,
    });
    assert_eq!(run.open_lanes(), vec![0, 1, 2]);
    run.settle_fight(Mark::Ran);
    assert_eq!(run.path[1].mark, Mark::Cached, "a cache is not a fight");

    assert_eq!(road_for(day(24)).node(0, 0), None, "steps count from one");
    assert_eq!(road_for(day(24)).node(11, 0), None);
    assert_eq!(road_for(day(24)).node(1, 1), Some(Node::Glyph));
}

/// The road of 2026-09-24, top lane first.
const PINNED_ROAD: [&str; 3] = ["g$B++B+$gg", "g+g$+g$+gg", "g$g+$g$$gg"];
