use super::*;

#[test]
fn all_embedded_creatures_parse() {
    // Tripwire: any new `.kdl` added to DEFAULT_CREATURE_SOURCES must
    // parse cleanly. Catches typos in tag names, heredoc fences, or
    // missing required fields before they hit a live aquarium.
    let creatures = load_default_creatures().expect("embedded creature kdl files must all parse");
    assert!(
        !creatures.is_empty(),
        "expected at least one default creature"
    );

    let names: std::collections::HashSet<String> =
        creatures.iter().map(|c| c.name.clone()).collect();
    for required in ["anchovy", "clownfish", "pufferfish"] {
        assert!(
            names.contains(required),
            "new creature `{required}` missing from default sources"
        );
    }
}
