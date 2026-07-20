use std::fs;

use crate::seed::*;

fn tmpdir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("late-usurper-seed-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("tmpdir");
    dir
}

#[test]
fn seeds_missing_files_but_never_overwrites() {
    let root = tmpdir("copy");
    let seed = root.join("seed");
    let game = root.join("game");
    fs::create_dir_all(seed.join("DATA")).unwrap();
    fs::write(seed.join("USURPER.CFG"), "seed-cfg").unwrap();
    fs::write(seed.join("DATA/MONSTER.DAT"), "monsters").unwrap();
    fs::create_dir_all(game.join("DATA")).unwrap();
    fs::write(game.join("DATA/MONSTER.DAT"), "live-world").unwrap();

    prepare_game_dir(seed.to_str().unwrap(), game.to_str().unwrap()).unwrap();

    // Missing file arrived; existing world data untouched.
    assert_eq!(fs::read_to_string(game.join("USURPER.CFG")).unwrap(), "seed-cfg");
    assert_eq!(
        fs::read_to_string(game.join("DATA/MONSTER.DAT")).unwrap(),
        "live-world"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn sweeps_stale_locks_at_boot() {
    let root = tmpdir("sweep");
    let seed = root.join("seed");
    let game = root.join("game");
    fs::create_dir_all(&seed).unwrap();
    fs::create_dir_all(game.join("DATA")).unwrap();
    fs::create_dir_all(game.join("NODE")).unwrap();
    fs::write(game.join("DATA/MAINT.FLG"), "").unwrap();
    fs::write(game.join("NODE/ONLINERS.DAT"), "ghost").unwrap();

    prepare_game_dir(seed.to_str().unwrap(), game.to_str().unwrap()).unwrap();

    assert!(!game.join("DATA/MAINT.FLG").exists());
    assert!(!game.join("NODE/ONLINERS.DAT").exists());
    let _ = fs::remove_dir_all(&root);
}
