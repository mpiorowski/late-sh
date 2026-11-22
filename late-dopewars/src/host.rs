use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use russh::ChannelId;
use russh::server::Handle;
use tokio::sync::mpsc;

/// Per-session counter used only to give each child a unique, private `HOME`
/// directory name. dopewars may read/write a per-user config dotfile; a shared
/// `HOME` across concurrent sessions could race, so each gets its own scratch
/// `HOME`, which also holds the session's own high-score table
/// (`scorelog::SessionTable`). Not persisted; only unique within a process
/// lifetime, which is all the temp-dir name needs.
static HOME_SEQ: AtomicU64 = AtomicU64::new(0);

/// How often a live session's table is folded into the run log, on top of the
/// sync at the session's end: a game finished mid-session survives a host
/// killed before that end.
const SCORE_SYNC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// Configuration for a single dopewars child process.
pub(crate) struct HostConfig {
    /// Path to the dopewars binary (e.g. `/usr/games/dopewars`).
    pub(crate) bin: String,
    /// The shared append-only run log on the PVC, where every session's
    /// finished games land (`scorelog`) and the stats session streams from.
    pub(crate) runs_log: PathBuf,
    /// The sanitized player handle, passed as `-P` so dopewars skips its startup
    /// name prompt and records the right player in the session's table.
    pub(crate) playname: String,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    pub(crate) term: String,
}

enum Command {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

/// Per-SSH-session host for a local dopewars process. Owns a background task that
/// runs the child on a PTY and bridges it to the SSH channel: client bytes flow
/// in via [`PtyHost::send_input`], child terminal output flows back out over the
/// russh [`Handle`].
///
/// This is the server-side twin of late-ssh's old in-process `DopewarsProcess`:
/// the same `openpty` child, but the transport is an SSH channel rather than a
/// shared `vt100::Parser`. dopewars has no savegame and no save-lock, so teardown
/// is a plain kill -- none of the nethack host's SIGHUP-save dance.
pub(crate) struct PtyHost {
    cmd_tx: mpsc::Sender<Command>,
}

impl PtyHost {
    pub(crate) fn spawn(cfg: HostConfig, handle: Handle, channel: ChannelId) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(256);
        // Detached: the JoinHandle drops here, but the task runs to completion.
        // Keep a clone of the handle so we can guarantee the channel is closed
        // even when run_bridge returns Err *before* its own eof/close teardown
        // (openpty / spawn / pty-clone failure -- e.g. a broken image or a
        // misconfigured LATE_DOPEWARS_BIN). Without this, the late-ssh client --
        // which marks the door Running the instant request_shell succeeds --
        // strands the user on the dopewars screen until the connection times out
        // instead of dropping back to the Games hub. All of run_bridge's `?`
        // early-returns are before eof/close, and nothing after eof/close can
        // fail, so an Err here always means the channel was never closed.
        let cleanup = handle.clone();
        tokio::spawn(async move {
            if let Err(e) = run_bridge(cfg, cmd_rx, handle, channel).await {
                tracing::warn!(error = ?e, "dopewars host bridge ended with error");
                let _ = cleanup.eof(channel).await;
                let _ = cleanup.close(channel).await;
            }
        });
        Self { cmd_tx }
    }

    pub(crate) fn send_input(&self, bytes: Vec<u8>) {
        let _ = self.cmd_tx.try_send(Command::Input(bytes));
    }

    pub(crate) fn resize(&self, cols: u16, rows: u16) {
        let _ = self.cmd_tx.try_send(Command::Resize { cols, rows });
    }
}

async fn run_bridge(
    cfg: HostConfig,
    mut cmd_rx: mpsc::Receiver<Command>,
    handle: Handle,
    channel: ChannelId,
) -> Result<()> {
    use std::os::fd::AsRawFd;
    use std::process::Stdio;
    use std::{fs, io};

    use anyhow::Context;
    use nix::libc;
    use nix::pty::{Winsize, openpty};
    use nix::unistd::setsid;
    use tokio::process::Command as TokioCommand;

    let winsize = Winsize {
        ws_row: cfg.rows.max(1),
        ws_col: cfg.cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = openpty(Some(&winsize), None).context("failed to allocate dopewars pty")?;
    let master = std::sync::Arc::new(fs::File::from(pty.master));
    let slave = fs::File::from(pty.slave);
    let slave_fd = slave.as_raw_fd();

    // Disable software flow control (XON/XOFF) on the pty. Otherwise a stray
    // Ctrl-S from the client is read as XOFF and the line discipline freezes the
    // game's output until an XON (Ctrl-Q) arrives. dopewars has no use for
    // XON/XOFF, so Ctrl-S should pass through as an ordinary (ignored) key.
    {
        use nix::sys::termios::{self, InputFlags, SetArg};
        if let Ok(mut tio) = termios::tcgetattr(&slave) {
            tio.input_flags
                .remove(InputFlags::IXON | InputFlags::IXOFF | InputFlags::IXANY);
            let _ = termios::tcsetattr(&slave, SetArg::TCSANOW, &tio);
        }
    }

    // Give the child a private, writable HOME so any per-user dopewars config
    // dotfile doesn't collide with concurrent sessions. The session's own
    // high-score table lives there too, empty at the start, so every game
    // this session plays makes it and reaches the run log (`scorelog`).
    let home = std::env::temp_dir().join(format!(
        "late-dopewars-home-{}",
        HOME_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let mut runs = SessionRuns {
        table: crate::scorelog::SessionTable::create(&home)
            .context("starting the dopewars session table")?,
        runs_log: cfg.runs_log,
        playname: cfg.playname,
    };

    let mut cmd = TokioCommand::new(&cfg.bin);
    // Single-player (`-n`), curses text client (`-t`), black-and-white (`-b`),
    // pointed at the session's own high-score table (`-f`), with the player's
    // handle as the playname (`-P`) so dopewars skips its "Hey dude, what's
    // your name?" prompt.
    //
    // `-b` is deliberate: dopewars' own color scheme hard-codes a blue-on-blue
    // window palette that assumes a black terminal and renders nearly unreadable
    // when embedded. Monochrome lets its default colors map to `Color::Reset`, so
    // the game inherits the late.sh theme (same approach as the nethack/rebels
    // doors) and stays legible.
    //
    // Spawn with a cleared environment plus an explicit allowlist so the child
    // sees only what curses needs: a TERM, a writable HOME, a UTF-8 locale for
    // the ncursesw line-drawing, and the window size.
    cmd.env_clear()
        .arg("-t")
        .arg("-n")
        .arg("-b")
        .arg("-f")
        .arg(runs.table.score_file())
        .arg("-P")
        .arg(&runs.playname)
        .env("TERM", &cfg.term)
        .env("HOME", &home)
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("LINES", cfg.rows.max(1).to_string())
        .env("COLUMNS", cfg.cols.max(1).to_string())
        .stdin(Stdio::from(
            slave
                .try_clone()
                .context("clone dopewars pty slave for stdin")?,
        ))
        .stdout(Stdio::from(
            slave
                .try_clone()
                .context("clone dopewars pty slave for stdout")?,
        ))
        .stderr(Stdio::from(
            slave
                .try_clone()
                .context("clone dopewars pty slave for stderr")?,
        ))
        .kill_on_drop(true);

    // Give the child its own session and make the PTY its controlling terminal,
    // so curses sizing and job control behave.
    unsafe {
        cmd.pre_exec(move || {
            setsid().map_err(|e| io::Error::from_raw_os_error(e as i32))?;
            if libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to start dopewars ({})", cfg.bin))?;
    drop(slave);

    // Blocking reader: pump child output to the SSH channel. Runs on its own
    // thread (blocking reads) and forwards chunks through an unbounded channel
    // to the async select loop below, which writes them to the russh handle.
    let reader_master = master
        .try_clone()
        .context("clone dopewars pty master for reader")?;
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut src: &fs::File = &reader_master;
        let mut buf = [0u8; 8192];
        loop {
            match src.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if out_tx.send(buf[..n].to_vec()).is_err() {
                        break; // bridge gone
                    }
                }
            }
        }
    });

    bridge_loop(
        &mut cmd_rx,
        &mut out_rx,
        &master,
        &mut child,
        &handle,
        channel,
        &mut runs,
    )
    .await;

    // Close the SSH channel first so the late-ssh client returns to its launcher
    // immediately.
    let _ = handle.eof(channel).await;
    let _ = handle.close(channel).await;

    // Kill the child (a no-op if it already exited); the reader then sees EOF.
    // dopewars has no savegame, so a dropped run simply ends -- no SIGHUP dance.
    let _ = child.kill().await;
    drop(master);
    // The reader exits on its own at EOF; don't block teardown joining it.
    drop(reader);

    // The child may have just written its table; fold what the timer has not
    // logged yet into the run log before the table goes with the HOME.
    runs.sync();
    // Best-effort cleanup of the per-session scratch HOME.
    let _ = fs::remove_dir_all(&home);
    Ok(())
}

/// A session's table with what folding it into the run log takes.
struct SessionRuns {
    table: crate::scorelog::SessionTable,
    /// The shared run log the session's finished games are appended to.
    runs_log: PathBuf,
    /// Whose session it is, for the failure log.
    playname: String,
}

impl SessionRuns {
    /// Fold the session's table into the run log. A failure is logged here.
    /// The table keeps what was not logged, so the next timer sync retries
    /// it; a failure of the last sync, at teardown, loses those runs.
    fn sync(&mut self) {
        let now_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if let Err(e) = self.table.sync(&self.runs_log, now_epoch) {
            tracing::warn!(error = ?e, playname = %self.playname, "failed to sync dopewars session table into the run log");
        }
    }
}

async fn bridge_loop(
    cmd_rx: &mut mpsc::Receiver<Command>,
    out_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    master: &std::sync::Arc<std::fs::File>,
    child: &mut tokio::process::Child,
    handle: &Handle,
    channel: ChannelId,
    runs: &mut SessionRuns,
) {
    use std::io::Write;

    // "Play again?" keeps one child running across games, so the table is
    // synced while the session lives, not only at its end.
    let start = tokio::time::Instant::now() + SCORE_SYNC_INTERVAL;
    let mut sync_tick = tokio::time::interval_at(start, SCORE_SYNC_INTERVAL);
    loop {
        tokio::select! {
            _ = sync_tick.tick() => runs.sync(),
            cmd = cmd_rx.recv() => match cmd {
                Some(Command::Input(bytes)) => {
                    let mut sink: &std::fs::File = master;
                    if sink.write_all(&bytes).is_err() {
                        // pty master write failed: the child's tty is gone, so it
                        // has already exited.
                        return;
                    }
                }
                Some(Command::Resize { cols, rows }) => set_winsize(master, cols, rows),
                // PtyHost dropped (client closed the channel, e.g. a rollout).
                None => return,
            },
            out = out_rx.recv() => match out {
                Some(bytes) => {
                    if handle.data(channel, bytes).await.is_err() {
                        return; // SSH channel to late-ssh gone (client disconnect)
                    }
                }
                None => return, // reader thread ended (pty EOF)
            },
            _ = child.wait() => return, // dopewars exited (quit, end of game, crash)
        }
    }
}

/// Push a new window size to the PTY; the kernel signals SIGWINCH to the child's
/// foreground group, and dopewars does a full `endwin()`+`newterm()` rebuild.
fn set_winsize(master: &std::fs::File, cols: u16, rows: u16) {
    use std::os::fd::AsRawFd;

    use nix::libc;

    let ws = libc::winsize {
        ws_row: rows.max(1),
        ws_col: cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe {
        libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &ws);
    }
}
