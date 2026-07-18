use std::time::Duration;

use anyhow::Result;
use russh::ChannelId;
use russh::server::Handle;
use tokio::sync::{mpsc, watch};

/// How long to wait for crawl's hangup-save after SIGHUP before falling back to
/// SIGKILL. crawl saves-and-exits on SIGHUP (the behavior every dgamelaunch
/// public server relies on) normally in well under a second; the bound just
/// stops a wedged child from pinning teardown forever. Must stay under the host
/// pod's `terminationGracePeriodSeconds` so a pod-wide SIGTERM can drain every
/// child (see `main.rs` SHUTDOWN_GRACE and infra/service-dcss.tf).
const HANGUP_SAVE_GRACE: Duration = Duration::from_secs(5);

/// Why the bridge loop stopped; decides whether teardown must SIGHUP-save.
enum StopReason {
    /// The child exited on its own (in-game save, death, quit, or crash). It
    /// already ran crawl's own exit path.
    ChildExited,
    /// The session is being torn down with the child still live: the client
    /// closed the channel (e.g. a service-ssh rollout) or the host got SIGTERM.
    /// The child must be SIGHUP-saved so the game survives instead of being
    /// SIGKILLed mid-run (crawl would offer a crash recovery prompt at best).
    Teardown,
}

/// Configuration for a single crawl child process.
pub struct HostConfig {
    /// Path to the crawl console binary (e.g. `/usr/games/crawl`).
    pub bin: String,
    /// `HOME` for the child. crawl keeps everything under `$HOME/.crawl`: saves
    /// keyed by the `-name` playname, shared scores/logfile/milestones, morgues.
    pub data_dir: String,
    /// In-game player name, passed as `-name`. Already sanitized to be PTY-safe.
    pub playname: String,
    pub cols: u16,
    pub rows: u16,
    pub term: String,
}

enum Command {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

/// Per-SSH-session host for a local crawl process. Owns a background task that
/// runs the child on a PTY and bridges it to the SSH channel: client bytes flow
/// in via [`PtyHost::send_input`], child terminal output flows back out over the
/// russh [`Handle`].
///
/// Same shape as the nethack host's `PtyHost`, including the graceful teardown:
/// the bridge task is detached, and on drop `cmd_tx` closes, the bridge sees the
/// channel end, and it runs the SIGHUP-save teardown (see [`run_bridge`]) before
/// exiting on its own; it is deliberately NOT aborted, since aborting would
/// SIGKILL crawl mid-game and lose the run.
pub struct PtyHost {
    cmd_tx: mpsc::Sender<Command>,
}

impl PtyHost {
    pub fn spawn(
        cfg: HostConfig,
        handle: Handle,
        channel: ChannelId,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(256);
        // Detached: the JoinHandle drops here, but the task runs to completion.
        // Keep a clone of the handle so we can guarantee the channel is closed
        // even when run_bridge returns Err *before* its own eof/close teardown
        // (openpty / spawn / pty-clone failure -- e.g. a broken image or a
        // misconfigured LATE_DCSS_BIN). Without this, the late-ssh client --
        // which marks the door Running the instant request_shell succeeds --
        // strands the user on the DCSS screen until the connection times out
        // instead of dropping back to the Games hub. All of run_bridge's `?`
        // early-returns are before eof/close, and nothing after eof/close can
        // fail, so an Err here always means the channel was never closed.
        let cleanup = handle.clone();
        tokio::spawn(async move {
            if let Err(e) = run_bridge(cfg, cmd_rx, handle, channel, shutdown_rx).await {
                tracing::warn!(error = ?e, "dcss host bridge ended with error");
                let _ = cleanup.eof(channel).await;
                let _ = cleanup.close(channel).await;
            }
        });
        Self { cmd_tx }
    }

    pub fn send_input(&self, bytes: Vec<u8>) {
        let _ = self.cmd_tx.try_send(Command::Input(bytes));
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.cmd_tx.try_send(Command::Resize { cols, rows });
    }
}

async fn run_bridge(
    cfg: HostConfig,
    mut cmd_rx: mpsc::Receiver<Command>,
    handle: Handle,
    channel: ChannelId,
    mut shutdown_rx: watch::Receiver<bool>,
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
    let pty = openpty(Some(&winsize), None).context("failed to allocate dcss pty")?;
    let master = std::sync::Arc::new(fs::File::from(pty.master));
    let slave = fs::File::from(pty.slave);
    let slave_fd = slave.as_raw_fd();

    // Disable software flow control (XON/XOFF) on the pty. Otherwise a stray
    // Ctrl-S from the client is read as XOFF and the line discipline freezes the
    // game's output until an XON (Ctrl-Q) arrives. crawl binds Ctrl-S itself
    // (save), so it must arrive as an ordinary key.
    {
        use nix::sys::termios::{self, InputFlags, SetArg};
        if let Ok(mut tio) = termios::tcgetattr(&slave) {
            tio.input_flags
                .remove(InputFlags::IXON | InputFlags::IXOFF | InputFlags::IXANY);
            let _ = termios::tcsetattr(&slave, SetArg::TCSANOW, &tio);
        }
    }

    let mut cmd = TokioCommand::new(&cfg.bin);
    // Spawn with a cleared environment and an explicit allowlist. crawl needs a
    // TERM, a writable HOME (everything lives under `$HOME/.crawl`), a UTF-8
    // locale for its Unicode map glyphs and ncursesw line drawing, and the
    // window size. The `-name` argument keys the per-player save inside the
    // shared playground; crawl skips its name prompt when one is given.
    cmd.env_clear()
        .arg("-name")
        .arg(&cfg.playname)
        .env("TERM", &cfg.term)
        .env("HOME", &cfg.data_dir)
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("LINES", cfg.rows.max(1).to_string())
        .env("COLUMNS", cfg.cols.max(1).to_string())
        .stdin(Stdio::from(
            slave.try_clone().context("clone dcss pty slave for stdin")?,
        ))
        .stdout(Stdio::from(
            slave
                .try_clone()
                .context("clone dcss pty slave for stdout")?,
        ))
        .stderr(Stdio::from(
            slave
                .try_clone()
                .context("clone dcss pty slave for stderr")?,
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
        .with_context(|| format!("failed to start crawl ({})", cfg.bin))?;
    drop(slave);

    // Blocking reader: pump child output to the SSH channel. Runs on its own
    // thread (blocking reads) and forwards chunks through an unbounded channel
    // to the async select loop below, which writes them to the russh handle.
    let reader_master = master
        .try_clone()
        .context("clone dcss pty master for reader")?;
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

    let stop = bridge_loop(
        &mut cmd_rx,
        &mut out_rx,
        &master,
        &mut child,
        &handle,
        channel,
        &mut shutdown_rx,
    )
    .await;

    // Close the SSH channel first so the late-ssh client returns to its launcher
    // immediately; any (possibly slow) save below then runs out of band.
    let _ = handle.eof(channel).await;
    let _ = handle.close(channel).await;

    match stop {
        StopReason::ChildExited => {
            // The game already ran crawl's own exit path; nothing to save.
            tracing::debug!(playname = %cfg.playname, "crawl child exited; closing channel");
        }
        StopReason::Teardown => {
            // Client hung up (e.g. a service-ssh rollout) or the host is shutting
            // down, with the game still live. SIGHUP runs crawl's hangup-save so
            // the run resumes next launch instead of being SIGKILLed mid-game.
            if let Some(pid) = child.id() {
                send_sighup(pid, &cfg.playname);
                // Bound the wait so a wedged child can't pin teardown; the
                // SIGKILL below is the backstop.
                match tokio::time::timeout(HANGUP_SAVE_GRACE, child.wait()).await {
                    Ok(_) => {
                        tracing::info!(playname = %cfg.playname, "crawl hangup-save complete")
                    }
                    Err(_) => tracing::warn!(
                        playname = %cfg.playname,
                        "crawl did not exit within hangup-save grace; killing"
                    ),
                }
            }
        }
    }

    // Backstop: a no-op if the child already exited above, else SIGKILL via
    // kill_on_drop. The reader then sees EOF.
    let _ = child.kill().await;
    drop(master);

    // The reader exits on its own at EOF; don't block teardown joining it (the
    // nethack host learned this the hard way with a save-time grandchild holding
    // the PTY open).
    drop(reader);
    Ok(())
}

async fn bridge_loop(
    cmd_rx: &mut mpsc::Receiver<Command>,
    out_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    master: &std::sync::Arc<std::fs::File>,
    child: &mut tokio::process::Child,
    handle: &Handle,
    channel: ChannelId,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> StopReason {
    use std::io::Write;

    // Already shutting down when the game launched: tear down (and save) at once.
    if *shutdown_rx.borrow() {
        return StopReason::Teardown;
    }
    // Disabled once the watch sender drops, so its always-ready `changed()` can't
    // spin the select loop.
    let mut watch_live = true;

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => match cmd {
                Some(Command::Input(bytes)) => {
                    let mut sink: &std::fs::File = master;
                    if sink.write_all(&bytes).is_err() {
                        // pty master write failed: the child's tty is gone, so it
                        // has already exited, so no hangup-save to run.
                        return StopReason::ChildExited;
                    }
                }
                Some(Command::Resize { cols, rows }) => set_winsize(master, cols, rows),
                // PtyHost dropped (client closed the channel, e.g. a rollout): the
                // child is still live, so SIGHUP-save it.
                None => return StopReason::Teardown,
            },
            out = out_rx.recv() => match out {
                Some(bytes) => {
                    if handle.data(channel, bytes).await.is_err() {
                        // SSH channel to late-ssh gone (client disconnect) while the
                        // child is still live: SIGHUP-save it.
                        return StopReason::Teardown;
                    }
                }
                None => return StopReason::ChildExited, // reader thread ended (pty EOF)
            },
            _ = child.wait() => return StopReason::ChildExited, // crawl exited (save, death, quit, crash)
            res = shutdown_rx.changed(), if watch_live => match res {
                Ok(()) if *shutdown_rx.borrow() => return StopReason::Teardown, // host SIGTERM
                Ok(()) => {}                 // spurious wake; value still false
                Err(_) => watch_live = false, // sender dropped; stop polling this arm
            },
        }
    }
}

/// Send SIGHUP to a live crawl child so it runs its hangup-save (save-and-exit,
/// resumable next launch) instead of being SIGKILLed.
fn send_sighup(pid: u32, playname: &str) {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;

    match kill(Pid::from_raw(pid as i32), Signal::SIGHUP) {
        Ok(()) => tracing::info!(pid, playname, "SIGHUP -> crawl for hangup-save"),
        Err(e) => {
            tracing::debug!(pid, playname, error = ?e, "SIGHUP to crawl failed (already exited?)")
        }
    }
}

/// Push a new window size to the PTY; the kernel signals SIGWINCH to the child's
/// foreground group so curses redraws at the new size.
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
