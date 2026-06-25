use anyhow::Result;
use russh::ChannelId;
use russh::server::Handle;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Configuration for a single NetHack child process.
pub struct HostConfig {
    /// Path to the nethack binary (e.g. `/usr/games/nethack`).
    pub bin: String,
    /// `HOME` for the child, where its `.nethackrc` lives. Saves/bones live in
    /// the nethack install's own playground, keyed by the `-u` name.
    pub data_dir: String,
    /// In-game player name, passed as `-u`. Already sanitized to be PTY-safe.
    pub playname: String,
    pub cols: u16,
    pub rows: u16,
    pub term: String,
}

enum Command {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

/// Per-SSH-session host for a local NetHack process. Owns a background task that
/// runs the child on a PTY and bridges it to the SSH channel: client bytes flow
/// in via [`PtyHost::send_input`], child terminal output flows back out over the
/// russh [`Handle`].
///
/// This is the server-side twin of late-ssh's old in-process `NethackProcess`:
/// the same `openpty` child, but the transport is an SSH channel rather than a
/// shared `vt100::Parser`.
pub struct PtyHost {
    cmd_tx: mpsc::Sender<Command>,
    task: JoinHandle<()>,
}

impl PtyHost {
    pub fn spawn(cfg: HostConfig, handle: Handle, channel: ChannelId) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(256);
        let task = tokio::spawn(async move {
            if let Err(e) = run_bridge(cfg, cmd_rx, handle, channel).await {
                tracing::warn!(error = ?e, "nethack host bridge ended with error");
            }
        });
        Self { cmd_tx, task }
    }

    pub fn send_input(&self, bytes: Vec<u8>) {
        let _ = self.cmd_tx.try_send(Command::Input(bytes));
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.cmd_tx.try_send(Command::Resize { cols, rows });
    }
}

impl Drop for PtyHost {
    fn drop(&mut self) {
        // Aborts the bridge task; the child's `kill_on_drop` then kills nethack.
        self.task.abort();
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
    let pty = openpty(Some(&winsize), None).context("failed to allocate nethack pty")?;
    let master = std::sync::Arc::new(fs::File::from(pty.master));
    let slave = fs::File::from(pty.slave);
    let slave_fd = slave.as_raw_fd();

    // Disable software flow control (XON/XOFF) on the pty. Otherwise a stray
    // Ctrl-S from the client is read as XOFF and the line discipline freezes the
    // game's output until an XON (Ctrl-Q) arrives. nethack has no use for
    // XON/XOFF, so Ctrl-S should pass through as an ordinary (ignored) key.
    {
        use nix::sys::termios::{self, InputFlags, SetArg};
        if let Ok(mut tio) = termios::tcgetattr(&slave) {
            tio.input_flags
                .remove(InputFlags::IXON | InputFlags::IXOFF | InputFlags::IXANY);
            let _ = termios::tcsetattr(&slave, SetArg::TCSANOW, &tio);
        }
    }

    let mut cmd = TokioCommand::new(&cfg.bin);
    // Spawn with a cleared environment and an explicit allowlist. Even though
    // this process is a dedicated host (not late-ssh), keep the env minimal so
    // the child only ever sees what it needs. NetHack's shell ('!') and suspend
    // ('^Z') escapes are compiled out in the nethack-build stage; clearing the
    // env is additional defense in depth.
    cmd.env_clear()
        .arg("-u")
        .arg(&cfg.playname)
        .env("TERM", &cfg.term)
        // HOME holds the per-player `.nethackrc`. We deliberately do NOT set
        // NETHACKDIR: the binary self-locates via its compiled-in HACKDIR, and
        // overriding NETHACKDIR to an empty dir makes nethack fail to chdir.
        .env("HOME", &cfg.data_dir)
        .env("LINES", cfg.rows.max(1).to_string())
        .env("COLUMNS", cfg.cols.max(1).to_string())
        .stdin(Stdio::from(
            slave
                .try_clone()
                .context("clone nethack pty slave for stdin")?,
        ))
        .stdout(Stdio::from(
            slave
                .try_clone()
                .context("clone nethack pty slave for stdout")?,
        ))
        .stderr(Stdio::from(
            slave
                .try_clone()
                .context("clone nethack pty slave for stderr")?,
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
        .with_context(|| format!("failed to start nethack ({})", cfg.bin))?;
    drop(slave);

    // Blocking reader: pump child output to the SSH channel. Runs on its own
    // thread (blocking reads) and forwards chunks through an unbounded channel
    // to the async select loop below, which writes them to the russh handle.
    let reader_master = master
        .try_clone()
        .context("clone nethack pty master for reader")?;
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

    tracing::debug!(playname = %cfg.playname, "nethack child exited; closing channel");

    // Close the SSH channel the instant nethack exits, BEFORE cleanup, so the
    // late-ssh client sees the close and returns to its launcher immediately.
    let _ = handle.eof(channel).await;
    let _ = handle.close(channel).await;

    // Dropping the child kills nethack (kill_on_drop); the reader then sees EOF.
    let _ = child.kill().await;
    drop(master);

    // Deliberately do NOT join the reader. On `S` save, nethack can hand the
    // save file to an external compressor that inherits the pty slave and
    // outlives the game by several seconds; a blocking `reader.join()` would pin
    // a runtime worker on that lingering process. The channel is already closed,
    // so the session ends now; the detached reader exits on its own at EOF.
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
                        break;
                    }
                }
                Some(Command::Resize { cols, rows }) => set_winsize(master, cols, rows),
                None => break, // host dropped (client closed the channel)
            },
            out = out_rx.recv() => match out {
                Some(bytes) => {
                    if handle.data(channel, bytes).await.is_err() {
                        break; // channel gone
                    }
                }
                None => break, // reader thread ended (pty EOF)
            },
            _ = child.wait() => break, // nethack exited (quit, death, crash)
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
