use super::*;

#[test]
fn rc_paths_are_distinct_per_playname() {
    assert_eq!(rc_path("/data", "alice"), "/data/rc/alice.nethackrc");
    assert_eq!(rc_path("/data/", "bob"), "/data/rc/bob.nethackrc");
}

fn scratch_data_dir(test: &str) -> String {
    let dir = std::env::temp_dir().join(format!("late-nethack-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.to_str().expect("utf-8 temp dir").to_string()
}

#[test]
fn materialize_writes_then_clears_the_rc_file() {
    let data_dir = scratch_data_dir("materialize");

    let path = materialize_rc(&data_dir, "alice", Some("OPTIONS=autopickup\n"))
        .expect("push should land a file");
    assert_eq!(
        std::fs::read_to_string(&path).expect("rc readable"),
        "OPTIONS=autopickup\n"
    );

    // An empty push deletes the file and reports no rc to use.
    assert_eq!(materialize_rc(&data_dir, "alice", Some("")), None);
    assert!(std::fs::metadata(&path).is_err());
    // Clearing again (nothing on disk) stays quiet.
    assert_eq!(materialize_rc(&data_dir, "alice", Some("")), None);
}

#[test]
fn materialize_none_keeps_whatever_is_on_disk() {
    let data_dir = scratch_data_dir("none");

    // No push, no file: launch with defaults.
    assert_eq!(materialize_rc(&data_dir, "bob", None), None);

    // No push, existing file (written by a newer client earlier): keep using it.
    let path = materialize_rc(&data_dir, "bob", Some("OPTIONS=color\n")).expect("file lands");
    assert_eq!(materialize_rc(&data_dir, "bob", None), Some(path));
}

/// A PTY resize after spawn must reach the child's curses. ncurses reads
/// `LINES`/`COLUMNS` as an override of the pty size and then ignores SIGWINCH,
/// so exporting them freezes the game at its spawn-time geometry while
/// late.sh's vt100 grid follows the resize: ghost glyphs, rows shifted into
/// window borders, and a Ctrl-R that redraws at the stale size.
#[test]
fn child_curses_sees_the_pty_resized_after_spawn() {
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;

    use nix::pty::{Winsize, openpty};

    let data_dir = scratch_data_dir("resize");
    std::fs::create_dir_all(&data_dir).expect("scratch dir");
    // Stands in for nethack: waits for a key, then asks curses for its size.
    let bin = format!("{data_dir}/fake-nethack");
    std::fs::write(&bin, "#!/bin/sh\nread _\ntput cols\ntput lines\n").expect("write fake bin");
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
        .expect("chmod fake bin");
    let cfg = HostConfig {
        bin,
        data_dir,
        playname: "alice".to_string(),
        cols: 100,
        rows: 30,
        term: "xterm-256color".to_string(),
        rc: None,
    };

    let winsize = Winsize {
        ws_row: cfg.rows,
        ws_col: cfg.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = openpty(Some(&winsize), None).expect("openpty");
    let mut master = std::fs::File::from(pty.master);
    let slave = std::fs::File::from(pty.slave);
    let mut child = child_command(&cfg, None)
        .stdin(slave.try_clone().expect("clone slave"))
        .stdout(slave.try_clone().expect("clone slave"))
        .stderr(slave)
        .spawn()
        .expect("spawn fake nethack");

    // The watchers' chat opening beside the game narrows it after spawn.
    set_winsize(&master, 60, 20);
    master.write_all(b"\n").expect("press a key");
    child.wait().expect("fake nethack exits");

    // Drain until the closed slave reports EIO.
    let mut out = Vec::new();
    let mut buf = [0u8; 1024];
    while let Ok(n) = master.read(&mut buf)
        && n > 0
    {
        out.extend_from_slice(&buf[..n]);
    }
    let out = String::from_utf8(out).expect("utf-8 output");
    let sizes: Vec<&str> = out.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    assert_eq!(sizes, ["60", "20"]);
}
