use std::time::Duration;

use anyhow::Result;
use russh::ChannelId;
use russh::server::Handle;
use tokio::sync::{mpsc, watch};

use crate::cp437;
use crate::nodes::NodeLease;

/// How long to wait for the game to exit after SIGHUP before falling back to
/// SIGKILL. Usurper has no hangup-save to run (player state is written to
/// DATA/USERS.DAT as it changes and the online table self-heals), so this is
/// just a polite window for the runtime to unwind before the hard kill.
const HANGUP_GRACE: Duration = Duration::from_secs(3);

/// Why the bridge loop stopped; decides whether teardown must signal the child.
enum StopReason {
    /// The child exited on its own (in-game quit, time-out, or crash).
    ChildExited,
    /// The session is being torn down with the child still live: the client
    /// closed the channel (e.g. a service-ssh rollout) or the host got SIGTERM.
    Teardown,
}

/// Configuration for a single Usurper child process.
pub struct HostConfig {
    /// Path to USURPER.EXE.
    pub bin: String,
    /// The shared game tree; becomes the child's working directory (the game
    /// resolves DATA/, TEXT/, NODE/, the dropfile path, everything, relative
    /// to it).
    pub game_dir: String,
    /// Game-relative dropfile directory for this session (from
    /// `dropfile::write_door32`), passed via `/P`.
    pub drop_rel: String,
    /// The leased node number, passed via `/N`. Held by the bridge task so the
    /// lease outlives the child and frees itself when the bridge ends.
    pub node: NodeLease,
    pub cols: u16,
    pub rows: u16,
}

enum Command {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

/// Per-SSH-session host for a local Usurper process. Owns a background task
/// that runs the child on a PTY and bridges it to the SSH channel: client
/// bytes flow in via [`PtyHost::send_input`], child terminal output is
/// transcoded CP437 -> UTF-8 and flows back out over the russh [`Handle`].
///
/// Same shape as the dcss host's `PtyHost`: the bridge task is detached, and on
/// drop `cmd_tx` closes, the bridge sees the channel end, and it runs the
/// teardown before exiting on its own.
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
        // The cloned handle guarantees the channel is closed even when
        // run_bridge returns Err before its own eof/close teardown (openpty /
        // spawn failure, e.g. a broken image). Without this the late-ssh
        // client, which marks the door Running the instant request_shell
        // succeeds, strands the user on the Usurper screen until the
        // connection times out instead of dropping back to the Games hub.
        let cleanup = handle.clone();
        tokio::spawn(async move {
            if let Err(e) = run_bridge(cfg, cmd_rx, handle, channel, shutdown_rx).await {
                tracing::warn!(error = ?e, "usurper host bridge ended with error");
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
    let pty = openpty(Some(&winsize), None).context("failed to allocate usurper pty")?;
    let master = std::sync::Arc::new(fs::File::from(pty.master));
    let slave = fs::File::from(pty.slave);
    let slave_fd = slave.as_raw_fd();

    // Disable software flow control (XON/XOFF) on the pty. Otherwise a stray
    // Ctrl-S from the client is read as XOFF and the line discipline freezes
    // the game's output until an XON (Ctrl-Q) arrives, and Usurper binds
    // Ctrl-S itself (send stuff), so it must arrive as an ordinary key.
    {
        use nix::sys::termios::{self, InputFlags, SetArg};
        if let Ok(mut tio) = termios::tcgetattr(&slave) {
            tio.input_flags
                .remove(InputFlags::IXON | InputFlags::IXOFF | InputFlags::IXANY);
            let _ = termios::tcsetattr(&slave, SetArg::TCSANOW, &tio);
        }
    }

    let mut cmd = TokioCommand::new(&cfg.bin);
    // Spawn with a cleared environment and an explicit allowlist. The game
    // resolves every path relative to its working directory (the shared game
    // tree). TERM is pinned to plain xterm ON PURPOSE, a deliberate
    // divergence from the nethack/dcss hosts' pass-through-with-fallback: the
    // child's output is interpreted by late-ssh's vt100 parser, never by the
    // player's real terminal, and the FPC Crt runtime keys some behavior off
    // TERM, so pinning it makes every session emit the same dialect. `/P`
    // points at this session's dropfile dir, `/N` is the leased node.
    cmd.env_clear()
        .current_dir(&cfg.game_dir)
        .arg(format!("/P{}", cfg.drop_rel))
        .arg(format!("/N{}", cfg.node.number()))
        .env("TERM", "xterm")
        .env("HOME", &cfg.game_dir)
        .stdin(Stdio::from(
            slave
                .try_clone()
                .context("clone usurper pty slave for stdin")?,
        ))
        .stdout(Stdio::from(
            slave
                .try_clone()
                .context("clone usurper pty slave for stdout")?,
        ))
        .stderr(Stdio::from(
            slave
                .try_clone()
                .context("clone usurper pty slave for stderr")?,
        ))
        .kill_on_drop(true);

    // Give the child its own session and make the PTY its controlling terminal.
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
        .with_context(|| format!("failed to start usurper ({})", cfg.bin))?;
    drop(slave);

    // Blocking reader: pump child output to the SSH channel. Runs on its own
    // thread (blocking reads) and forwards chunks through an unbounded channel
    // to the async select loop below, which transcodes and writes them to the
    // russh handle.
    let reader_master = master
        .try_clone()
        .context("clone usurper pty master for reader")?;
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

    // Close the SSH channel first so the late-ssh client returns to its
    // launcher immediately; the child teardown below runs out of band.
    let _ = handle.eof(channel).await;
    let _ = handle.close(channel).await;

    match stop {
        StopReason::ChildExited => {
            tracing::debug!(node = cfg.node.number(), "usurper child exited; closing channel");
        }
        StopReason::Teardown => {
            // Client hung up or the host is shutting down with the game still
            // live. SIGHUP first (the BBS-era "carrier drop" signal; lets the
            // runtime unwind), SIGKILL as the backstop. Player state is
            // already on disk (the game writes USERS.DAT as it goes) and a
            // stale online entry ages out via the game's own kick-out plus
            // the boot sweep.
            if let Some(pid) = child.id() {
                send_sighup(pid, cfg.node.number());
                match tokio::time::timeout(HANGUP_GRACE, child.wait()).await {
                    Ok(_) => {
                        tracing::info!(node = cfg.node.number(), "usurper child exited on SIGHUP")
                    }
                    Err(_) => tracing::warn!(
                        node = cfg.node.number(),
                        "usurper child ignored SIGHUP; killing"
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
    // nethack host learned this the hard way with a grandchild holding the PTY
    // open).
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

    // Already shutting down when the game launched: tear down at once.
    if *shutdown_rx.borrow() {
        return StopReason::Teardown;
    }
    // Disabled once the watch sender drops, so its always-ready `changed()`
    // can't spin the select loop.
    let mut watch_live = true;

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => match cmd {
                Some(Command::Input(bytes)) => {
                    // Keep the input stream byte-clean for the game: it reads
                    // CP437/ASCII, so multi-byte UTF-8 from the client (which
                    // would arrive as high bytes the game misreads as CP437
                    // glyph codes) is dropped; ASCII keys and the ESC-prefixed
                    // arrow sequences pass through untouched.
                    let filtered: Vec<u8> = bytes.into_iter().filter(|b| *b < 0x80).collect();
                    if filtered.is_empty() {
                        continue;
                    }
                    let mut sink: &std::fs::File = master;
                    if sink.write_all(&filtered).is_err() {
                        // pty master write failed: the child's tty is gone, so
                        // it has already exited.
                        return StopReason::ChildExited;
                    }
                }
                Some(Command::Resize { cols, rows }) => set_winsize(master, cols, rows),
                // PtyHost dropped (client closed the channel, e.g. a rollout).
                None => return StopReason::Teardown,
            },
            out = out_rx.recv() => match out {
                Some(bytes) => {
                    // CP437 -> UTF-8 before the bytes enter the SSH channel;
                    // the client's vt100 parser only speaks UTF-8. Safe on a
                    // raw chunk boundary because the mapping is byte-wise
                    // (see cp437.rs).
                    if handle.data(channel, cp437::to_utf8(&bytes)).await.is_err() {
                        // SSH channel to late-ssh gone (client disconnect)
                        // while the child is still live.
                        return StopReason::Teardown;
                    }
                }
                None => return StopReason::ChildExited, // reader thread ended (pty EOF)
            },
            _ = child.wait() => return StopReason::ChildExited, // game exited (quit, time-out, crash)
            res = shutdown_rx.changed(), if watch_live => match res {
                Ok(()) if *shutdown_rx.borrow() => return StopReason::Teardown, // host SIGTERM
                Ok(()) => {}                 // spurious wake; value still false
                Err(_) => watch_live = false, // sender dropped; stop polling this arm
            },
        }
    }
}

/// Send SIGHUP to a live child: the BBS-era carrier-drop signal, giving the
/// runtime a chance to unwind before the SIGKILL backstop.
fn send_sighup(pid: u32, node: u16) {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;

    match kill(Pid::from_raw(pid as i32), Signal::SIGHUP) {
        Ok(()) => tracing::info!(pid, node, "SIGHUP -> usurper child"),
        Err(e) => {
            tracing::debug!(pid, node, error = ?e, "SIGHUP to usurper failed (already exited?)")
        }
    }
}

/// Push a new window size to the PTY. The game draws a fixed 80x25 screen, but
/// keeping the PTY honest costs nothing and keeps the transport identical to
/// the other hosts.
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
