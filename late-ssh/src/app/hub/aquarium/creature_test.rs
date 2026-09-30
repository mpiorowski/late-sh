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
    for required in [
        "anchovy",
        "clownfish",
        "pufferfish",
        FRY_CREATURE,
        SPROUT_CREATURE,
    ] {
        assert!(
            names.contains(required),
            "new creature `{required}` missing from default sources"
        );
    }
}

fn mini_of(source: &str) -> Result<MiniGlyph> {
    let path = Path::new("test.kdl");
    let doc = kdl_parse::parse_document(path, source).expect("test kdl parses");
    parse_mini(&doc, path)
}

#[test]
fn a_creature_without_a_mini_glyph_is_rejected() {
    let missing = mini_of(r#"name "bare""#).expect_err("no mini node");
    assert_eq!(missing.to_string(), "test.kdl has no `mini` glyph");

    let one_sided = mini_of(r#"mini left="<'""#).expect_err("no right glyph");
    assert_eq!(
        one_sided.to_string(),
        "test.kdl `mini` needs a `right` string"
    );
}

#[test]
fn a_mini_glyph_is_one_to_three_cells() {
    assert_eq!(
        mini_of(r#"mini left="<'-" right="-'>""#).expect("three cells fit"),
        MiniGlyph {
            left: "<'-".to_string(),
            right: "-'>".to_string(),
        }
    );
    let wide = mini_of(r#"mini left="<'--" right="-'>""#).expect_err("four cells");
    assert_eq!(
        wide.to_string(),
        "test.kdl `mini left` must be one row of 1 to 3 cells"
    );
    let empty = mini_of(r#"mini left="<'-" right="""#).expect_err("no cells");
    assert_eq!(
        empty.to_string(),
        "test.kdl `mini right` must be one row of 1 to 3 cells"
    );
}
