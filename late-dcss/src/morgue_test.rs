use std::fs;
use std::path::Path;

use super::{migrate_flat, owner_of, player_dir};

/// A throwaway `HOME` with a flat morgue tree in it, in the shapes the live
/// playground actually holds: a finished game (dump + stash list) and an
/// in-game `#` character dump (same pair).
fn flat_tree(files: &[&str]) -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("tempdir");
    let morgue = home.path().join(".crawl/morgue");
    fs::create_dir_all(&morgue).expect("morgue dir");
    for file in files {
        fs::write(morgue.join(file), format!("dump of {file}")).expect("write dump");
    }
    home
}

fn data_dir(home: &tempfile::TempDir) -> String {
    home.path().to_str().expect("utf-8 temp dir").to_string()
}

#[test]
fn player_dirs_are_distinct_and_sit_under_the_published_root() {
    assert_eq!(
        player_dir("/data", "Luimory"),
        "/data/.crawl/morgue/Luimory"
    );
    // A trailing slash on the configured data dir must not double up.
    assert_eq!(player_dir("/data/", "ssx"), "/data/.crawl/morgue/ssx");
    assert_ne!(player_dir("/data", "alice"), player_dir("/data", "bob"));
}

#[test]
fn flat_dumps_move_into_their_players_directory() {
    let home = flat_tree(&[
        "morgue-Luimory-20260906-033636.txt",
        "morgue-Luimory-20260906-033636.lst",
        "morgue-ssx-20260910-175254.txt",
        "AZATHOTH.txt",
        "AZATHOTH.lst",
    ]);
    let root = home.path().join(".crawl/morgue");

    assert_eq!(migrate_flat(&data_dir(&home)), 5);

    assert_eq!(
        fs::read_to_string(root.join("Luimory/morgue-Luimory-20260906-033636.txt"))
            .expect("game dump moved"),
        "dump of morgue-Luimory-20260906-033636.txt"
    );
    assert!(
        root.join("Luimory/morgue-Luimory-20260906-033636.lst")
            .exists()
    );
    assert!(root.join("ssx/morgue-ssx-20260910-175254.txt").exists());
    // The `#` character dump and its stash list belong to the same player.
    assert!(root.join("AZATHOTH/AZATHOTH.txt").exists());
    assert!(root.join("AZATHOTH/AZATHOTH.lst").exists());
    // Nothing is left flat for a fetcher to miss.
    assert!(!root.join("morgue-Luimory-20260906-033636.txt").exists());
    assert!(!root.join("AZATHOTH.txt").exists());
}

#[test]
fn a_migrated_tree_is_left_alone_on_the_next_boot() {
    let home = flat_tree(&["morgue-mat_2-20260906-033636.txt"]);
    let data_dir = data_dir(&home);
    let moved = home
        .path()
        .join(".crawl/morgue/mat_2/morgue-mat_2-20260906-033636.txt");

    // An underscore is legal in a playname, so it must survive the split.
    assert_eq!(migrate_flat(&data_dir), 1);
    assert!(moved.exists());

    // Every later boot walks a directory of directories and moves nothing.
    assert_eq!(migrate_flat(&data_dir), 0);
    assert_eq!(
        fs::read_to_string(&moved).expect("dump kept"),
        "dump of morgue-mat_2-20260906-033636.txt"
    );
}

#[test]
fn a_dump_crawl_already_wrote_in_place_is_never_clobbered() {
    let home = flat_tree(&["morgue-ssx-20260910-175254.txt"]);
    let root = home.path().join(".crawl/morgue");
    let live = root.join("ssx/morgue-ssx-20260910-175254.txt");
    fs::create_dir_all(root.join("ssx")).expect("player dir");
    fs::write(&live, "written under -morgue").expect("live dump");

    assert_eq!(migrate_flat(&data_dir(&home)), 0);

    assert_eq!(
        fs::read_to_string(&live).expect("live dump kept"),
        "written under -morgue"
    );
}

#[test]
fn a_fresh_volume_has_nothing_to_carry_over() {
    let home = tempfile::tempdir().expect("tempdir");
    assert_eq!(migrate_flat(&data_dir(&home)), 0);
}

#[test]
fn owner_parsing_reads_both_shapes_and_refuses_the_rest() {
    // The timestamp carries dashes; the playname cannot, so the first one ends
    // the name.
    assert_eq!(
        owner_of("morgue-Luimory-20260906-033636.txt"),
        Some("Luimory")
    );
    assert_eq!(owner_of("morgue-mat_2-20260906-033636.lst"), Some("mat_2"));
    assert_eq!(owner_of("AZATHOTH.txt"), Some("AZATHOTH"));

    // Not something crawl writes here, so not something to file under a name.
    assert_eq!(owner_of("morgue-.txt"), None);
    assert_eq!(owner_of(".txt"), None);
    assert_eq!(owner_of("logfile"), None);
    assert_eq!(owner_of("notes.md"), None);
    // A hand-made file must never name a directory outside this one.
    assert_eq!(owner_of("...txt"), None);
    assert_eq!(owner_of("morgue-..-20260906-033636.txt"), None);
    assert_eq!(owner_of("a/b.txt"), None);
}

#[test]
fn every_moved_file_stays_inside_the_morgue_root() {
    // The escape a bad owner would buy: a dump landing beside the saves.
    let home = flat_tree(&["...txt", "morgue-..-20260906-033636.txt"]);
    let root = home.path().join(".crawl/morgue");

    assert_eq!(migrate_flat(&data_dir(&home)), 0);

    // Left where they were, and nothing appeared one level up.
    assert!(root.join("...txt").exists());
    assert_eq!(
        fs::read_dir(Path::new(&data_dir(&home)).join(".crawl"))
            .expect("crawl dir")
            .count(),
        1
    );
}
