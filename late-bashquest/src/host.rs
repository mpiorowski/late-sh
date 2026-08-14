use anyhow::Result;
use russh::ChannelId;
use russh::server::Handle;
use tokio::sync::mpsc;

/// Configuration for a single bashquest.sh child process.
pub(crate) struct HostConfig {
    /// Path to bashquest.sh.
    pub(crate) bin: String,
    /// The shared, persistent `HOME` every session gets (on the PVC in prod).
    /// bashquest.sh keeps `users.db` and every player's `<name>.save` under
    /// `$HOME/.bashquest`, so every player sharing this directory is what
    /// makes the in-game leaderboard actually mean something across the
    /// whole late.sh player base, not just one session.
    pub(crate) data_dir: String,
    /// The account's arcade handle, sanitized. Passed as `BASHQUEST_AUTOLOGIN`
    /// so bashquest.sh skips its own login/register screen and resumes (or
    /// creates) the save matching this exact handle.
    pub(crate) playname: String,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    pub(crate) term: String,
}

enum Command {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

/// Per-SSH-session host for a local bashquest.sh process. Owns a background
/// task that runs the child on a PTY and bridges it to the SSH channel:
/// client bytes flow in via [`PtyHost::send_input`], child terminal output
/// flows back out over the russh [`Handle`].
///
/// Same shape as the dopewars host: bashquest.sh has no external savegame
/// format to hangup-save, it just writes `$SAVE_DIR/<name>.save` continuously
/// as the player answers challenges (see bashquest.sh's `save_progress`
/// call sites), so teardown is a plain kill -- no SIGHUP dance.
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
        // misconfigured LATE_BASHQUEST_BIN). Without this the late-ssh client --
        // which marks the door Running the instant request_shell succeeds --
        // strands the user on the bashquest screen until the connection times
        // out instead of dropping back to the Games hub. All of run_bridge's `?`
        // early-returns are before eof/close, and nothing after eof/close can
        // fail, so an Err here always means the channel was never closed.
        let cleanup = handle.clone();
        tokio::spawn(async move {
            if let Err(e) = run_bridge(cfg, cmd_rx, handle, channel).await {
                tracing::warn!(error = ?e, "bashquest host bridge ended with error");
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
    let pty = openpty(Some(&winsize), None).context("failed to allocate bashquest pty")?;
    let master = std::sync::Arc::new(fs::File::from(pty.master));
    let slave = fs::File::from(pty.slave);
    let slave_fd = slave.as_raw_fd();

    // Disable software flow control (XON/XOFF) on the pty. Otherwise a stray
    // Ctrl-S from the client is read as XOFF and the line discipline freezes
    // the game's output until an XON (Ctrl-Q) arrives.
    {
        use nix::sys::termios::{self, InputFlags, SetArg};
        if let Ok(mut tio) = termios::tcgetattr(&slave) {
            tio.input_flags
                .remove(InputFlags::IXON | InputFlags::IXOFF | InputFlags::IXANY);
            let _ = termios::tcsetattr(&slave, SetArg::TCSANOW, &tio);
        }
    }

    let mut cmd = TokioCommand::new(&cfg.bin);
    // Spawn with a cleared environment plus an explicit allowlist: a TERM, the
    // shared persistent HOME, the account's handle as BASHQUEST_AUTOLOGIN (so
    // bashquest.sh's own login/register screen is skipped entirely, see
    // bashquest.sh's `autologin()`), a UTF-8 locale for its box-drawing
    // characters and emoji, and the window size.
    cmd.env_clear()
        .env("TERM", &cfg.term)
        .env("HOME", &cfg.data_dir)
        .env("BASHQUEST_AUTOLOGIN", &cfg.playname)
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("LINES", cfg.rows.max(1).to_string())
        .env("COLUMNS", cfg.cols.max(1).to_string())
        .stdin(Stdio::from(
            slave
                .try_clone()
                .context("clone bashquest pty slave for stdin")?,
        ))
        .stdout(Stdio::from(
            slave
                .try_clone()
                .context("clone bashquest pty slave for stdout")?,
        ))
        .stderr(Stdio::from(
            slave
                .try_clone()
                .context("clone bashquest pty slave for stderr")?,
        ))
        .kill_on_drop(true);

    // Give the child its own session and make the PTY its controlling
    // terminal, so bashquest.sh's `tput`/window-size assumptions behave.
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
        .with_context(|| format!("failed to start bashquest ({})", cfg.bin))?;
    drop(slave);

    // Blocking reader: pump child output to the SSH channel. Runs on its own
    // thread (blocking reads) and forwards chunks through an unbounded channel
    // to the async select loop below, which writes them to the russh handle.
    let reader_master = master
        .try_clone()
        .context("clone bashquest pty master for reader")?;
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
    )
    .await;

    // Close the SSH channel first so the late-ssh client returns to its
    // launcher immediately.
    let _ = handle.eof(channel).await;
    let _ = handle.close(channel).await;

    // Kill the child (a no-op if it already exited); the reader then sees EOF.
    // bashquest.sh saves continuously, so a dropped run loses at most the
    // current unanswered challenge, never earned progress -- no SIGHUP dance.
    let _ = child.kill().await;
    drop(master);
    // The reader exits on its own at EOF; don't block teardown joining it.
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
) {
    use std::io::Write;

    loop {
        tokio::select! {
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
            _ = child.wait() => return, // bashquest exited (quit, logout, crash)
        }
    }
}

/// Push a new window size to the PTY; the kernel signals SIGWINCH to the
/// child's foreground group.
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
