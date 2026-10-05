use super::*;

/// The screen at the room a default terminal leaves under the tour header.
fn text(fight: &Fight) -> Vec<String> {
    lines(fight, "mat", 78, 16)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// The whole scene, start to finish: the newcomer walks in, takes the
/// breath, and kills the dragon on the last strike. Further strikes change
/// nothing.
#[test]
fn six_strikes_walk_in_and_kill_the_dragon() {
    let mut fight = Fight::new();
    let opening = text(&fight);
    assert_eq!(
        opening,
        [
            "#                      #.#              mat the Slayer",
            "              ##########'##########     Minotaur of Okawaru ****..",
            "              #...................#     Health: 118/118 ======================",
            "              #....#.........#..?.##### Magic:  11/11   ======================",
            "              #...................'.... AC: 21           Str: 27",
            "###############...................##### EV: 12           Int:  6",
            "..............'....@......D.......#     SH:  9           Dex: 13",
            "#####.#########...................#     XL: 14 Next: 62% Place: Dungeon:13",
            "    #.#       #...............$$..#     Noise: ---------  Time: 31204.7 (1.0)",
            "    #.#       #....#.........#....#     a) +3 battleaxe (flame)",
            "    #.#       #...................#     Throw: 7 javelins",
            "    #.#       ##########+##########     D █ fire dragon",
            "A fire dragon comes into view.",
        ]
    );

    // The view follows the hero in, and the breath fills the lane.
    fight.strike();
    fight.strike();
    let burning = text(&fight);
    assert!(
        burning[5].starts_with("#############.....§§§...........#"),
        "{burning:#?}"
    );
    assert!(
        burning[6].starts_with("............'......@§§§§D.......#"),
        "{burning:#?}"
    );
    assert!(
        burning[2].ends_with("Health: 41/118  ========--------------"),
        "{burning:#?}"
    );
    assert!(
        burning[8].ends_with("Noise: =======--  Time: 31206.7 (1.0)"),
        "{burning:#?}"
    );
    assert_eq!(
        burning[12..],
        [
            "A fire dragon comes into view.",
            "The fire dragon roars deafeningly!",
            "The fire dragon breathes flames at you.",
            "The blast of flame engulfs you!! You are burned terribly!",
        ]
    );

    for _ in 0..4 {
        assert!(!fight.won());
        fight.strike();
    }
    assert!(fight.won());
    let over = text(&fight);
    assert!(
        over[6].starts_with("........'..........@†.......#"),
        "{over:#?}"
    );
    // The dragon leaves the monster list with its last hit point.
    assert_eq!(over[11], "#       ##########+##########");
    assert_eq!(
        over[12..],
        [
            "You slash the fire dragon!! The fire dragon claws you!",
            "The fire dragon is severely wounded.",
            "You slice the fire dragon!!! You kill the fire dragon!",
            "Okawaru is honoured by your kill.",
        ]
    );

    fight.strike();
    assert_eq!(text(&fight), over);
}
