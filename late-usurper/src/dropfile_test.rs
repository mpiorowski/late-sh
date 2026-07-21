use std::fs;

use crate::dropfile::*;

#[test]
fn writes_local_mode_dropfile_with_identity() {
    let root = std::env::temp_dir().join(format!("late-usurper-drop-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let rel = write_door32(root.to_str().unwrap(), 3, "Gnoll_Fan").unwrap();
    assert_eq!(rel, "DROP/3/");
    let written = fs::read_to_string(root.join("DROP/3/door32.sys")).unwrap();
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(
        lines[0], "0",
        "comm type must be 0 = local (PTY, no socket)"
    );
    assert_eq!(lines[5], "Gnoll_Fan", "real name carries the identity");
    assert_eq!(lines[6], "Gnoll_Fan");
    assert_eq!(lines[9], "1", "ANSI emulation");
    assert_eq!(lines[10], "3", "node number");

    // Relaunching the same node rewrites the file for the new player.
    write_door32(root.to_str().unwrap(), 3, "Other").unwrap();
    assert!(
        fs::read_to_string(root.join("DROP/3/door32.sys"))
            .unwrap()
            .contains("Other")
    );
    let _ = fs::remove_dir_all(&root);
}
