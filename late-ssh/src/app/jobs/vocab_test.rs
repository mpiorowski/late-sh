use std::collections::HashSet;

use super::vocab::{Normalized, VOCAB, all_tags, canonical, normalize};

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

#[test]
fn every_alias_is_unique_and_every_tag_is_its_own_alias() {
    let mut tags = HashSet::new();
    for tag in all_tags() {
        assert_eq!(canonical(tag), Some(tag), "{tag} must alias itself");
        assert!(tags.insert(tag), "{tag} listed twice");
    }
    let mut aliases = HashSet::new();
    for (tag, spellings) in VOCAB {
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
}
