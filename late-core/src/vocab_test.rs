use std::collections::HashSet;

use crate::vocab::{
    Group, Normalized, TAG_LIMIT, VOCAB, aliases_of, all_tags, canonical, group_of, normalize,
    normalize_langs, tags_in,
};

#[test]
fn aliases_fold_onto_one_canonical_tag() {
    assert_eq!(canonical("TS"), Some("typescript"));
    assert_eq!(canonical("#golang"), Some("go"));
    assert_eq!(canonical("PostgreSQL"), Some("postgres"));
    assert_eq!(canonical("c++"), Some("cpp"));
    assert_eq!(canonical("rust"), Some("rust"));
    assert_eq!(canonical("cobol"), None);
    assert_eq!(canonical("  "), None);
}

#[test]
fn normalize_splits_known_from_free_and_dedupes() {
    let skills = [
        "Rust", "rustlang", "pg", "cobol", "Cobol", "#nix", "ansible",
    ]
    .map(str::to_string);
    assert_eq!(
        normalize(&skills, 12),
        Normalized {
            tags: ["rust", "postgres", "nix", "ansible"]
                .map(str::to_string)
                .to_vec(),
            free: vec!["cobol".to_string()],
        }
    );
}

#[test]
fn normalize_stops_at_the_column_cap() {
    let skills = ["rust", "go", "cobol", "python"].map(str::to_string);
    let out = normalize(&skills, 3);
    assert_eq!(out.tags.len() + out.free.len(), 3);
    assert_eq!(out.free, vec!["cobol".to_string()]);
}

/// Langs are languages from the list and nothing else: a data store, an
/// unknown word, or a repeat is dropped, and the row caps at `TAG_LIMIT`.
#[test]
fn langs_fold_to_canonical_languages_and_drop_the_rest() {
    assert_eq!(
        normalize_langs(["Golang, TS", "rust postgres", "brainfuck", "#rust"]),
        vec!["go", "typescript", "rust"]
    );
    assert!(normalize_langs([""]).is_empty());
    let many: Vec<String> = tags_in(Group::Language).map(str::to_string).collect();
    assert!(
        many.len() > TAG_LIMIT,
        "the test needs more languages than the cap"
    );
    let joined = many.join(", ");
    assert_eq!(normalize_langs([joined.as_str()]).len(), TAG_LIMIT);
}

#[test]
fn every_tag_has_a_group_its_aliases_and_a_unique_alias_set() {
    let mut tags = HashSet::new();
    for tag in all_tags() {
        assert_eq!(canonical(tag), Some(tag), "{tag} must alias itself");
        assert!(tags.insert(tag), "{tag} listed twice");
        assert!(group_of(tag).is_some(), "{tag} has no group");
        assert_eq!(aliases_of(tag)[0], tag, "{tag} must lead its aliases");
    }
    assert_eq!(group_of("cobol"), None);
    assert!(aliases_of("cobol").is_empty());
    let mut aliases = HashSet::new();
    for (tag, _, spellings) in VOCAB {
        for alias in *spellings {
            assert!(
                aliases.insert(*alias),
                "{alias} folds onto two tags, one of them {tag}"
            );
            assert_eq!(
                *alias,
                alias.to_ascii_lowercase(),
                "{alias} must be lowercase"
            );
        }
    }
    // Every group is in use, and the groups together are the whole list.
    let grouped: usize = Group::ALL.iter().map(|g| tags_in(*g).count()).sum();
    assert_eq!(grouped, all_tags().count());
    for group in Group::ALL {
        assert!(tags_in(group).next().is_some(), "{group:?} is empty");
    }
}
