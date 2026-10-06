use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use russh::ChannelId;
use russh::server::Handle;
use tokio::sync::{mpsc, watch};

const HANGUP_SAVE_GRACE: Duration = Duration::from_secs(5);

pub(crate) struct SessionLease {
    account: String,
    active_accounts: Arc<Mutex<HashSet<String>>>,
}

impl SessionLease {
    pub(crate) fn new(account: String, active_accounts: Arc<Mutex<HashSet<String>>>) -> Self {
        Self {
            account,
            active_accounts,
        }
    }
}

impl Drop for SessionLease {
    fn drop(&mut self) {
        self.active_accounts
            .lock()
            .expect("active accounts mutex")
            .remove(&self.account);
    }
}

pub(crate) struct HostConfig {
    pub(crate) bin: String,
    pub(crate) data_dir: String,
    pub(crate) account: String,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    pub(crate) term: String,
}

enum Command {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

enum StopReason {
    ChildExited,
    Teardown,
}

/// Per-session PTY bridge. The detached task owns the account lease until the
/// child has exited and its save has been flushed.
pub(crate) struct PtyHost {
    cmd_tx: mpsc::Sender<Command>,
}

impl PtyHost {
    pub(crate) fn spawn(
        cfg: HostConfig,
        handle: Handle,
        channel: ChannelId,
        shutdown_rx: watch::Receiver<bool>,
        lease: SessionLease,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(256);
        let cleanup = handle.clone();
        tokio::spawn(async move {
            let _lease = lease;
            if let Err(e) = run_bridge(cfg, cmd_rx, handle, channel, shutdown_rx).await {
                tracing::warn!(error = ?e, "codekeep host bridge ended with error");
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

    let home = std::path::Path::new(&cfg.data_dir).join(&cfg.account);
    fs::create_dir_all(&home)
        .with_context(|| format!("create CodeKeep HOME {}", home.display()))?;

    let winsize = Winsize {
        ws_row: cfg.rows.max(1),
        ws_col: cfg.cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = openpty(Some(&winsize), None).context("failed to allocate codekeep pty")?;
    let master = Arc::new(fs::File::from(pty.master));
    let slave = fs::File::from(pty.slave);
    let slave_fd = slave.as_raw_fd();

    {
        use nix::sys::termios::{self, InputFlags, SetArg};
        if let Ok(mut tio) = termios::tcgetattr(&slave) {
            tio.input_flags
                .remove(InputFlags::IXON | InputFlags::IXOFF | InputFlags::IXANY);
            let _ = termios::tcsetattr(&slave, SetArg::TCSANOW, &tio);
        }
    }

    let mut cmd = TokioCommand::new(&cfg.bin);
    cmd.env_clear()
        .current_dir(&home)
        .env("TERM", &cfg.term)
        .env("HOME", &home)
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("LINES", cfg.rows.max(1).to_string())
        .env("COLUMNS", cfg.cols.max(1).to_string())
        .stdin(Stdio::from(
            slave
                .try_clone()
                .context("clone codekeep pty slave for stdin")?,
        ))
        .stdout(Stdio::from(
            slave
                .try_clone()
                .context("clone codekeep pty slave for stdout")?,
        ))
        .stderr(Stdio::from(
            slave
                .try_clone()
                .context("clone codekeep pty slave for stderr")?,
        ))
        .kill_on_drop(true);

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
        .with_context(|| format!("failed to start codekeep ({})", cfg.bin))?;
    drop(slave);

    let reader_master = master
        .try_clone()
        .context("clone codekeep pty master for reader")?;
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
                        break;
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

    let _ = handle.eof(channel).await;
    let _ = handle.close(channel).await;

    if matches!(stop, StopReason::Teardown)
        && let Some(pid) = child.id()
    {
        send_sighup(pid, &cfg.account);
        if tokio::time::timeout(HANGUP_SAVE_GRACE, child.wait())
            .await
            .is_err()
        {
            tracing::warn!(account = %cfg.account, "CodeKeep save grace elapsed; killing child");
        }
    }

    let _ = child.kill().await;
    drop(master);
    drop(reader);
    Ok(())
}

async fn bridge_loop(
    cmd_rx: &mut mpsc::Receiver<Command>,
    out_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    master: &Arc<std::fs::File>,
    child: &mut tokio::process::Child,
    handle: &Handle,
    channel: ChannelId,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> StopReason {
    use std::io::Write;

    if *shutdown_rx.borrow() {
        return StopReason::Teardown;
    }
    let mut watch_live = true;

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => match cmd {
                Some(Command::Input(bytes)) => {
                    let mut sink: &std::fs::File = master;
                    if sink.write_all(&bytes).is_err() {
                        return StopReason::ChildExited;
                    }
                }
                Some(Command::Resize { cols, rows }) => set_winsize(master, cols, rows),
                None => return StopReason::Teardown,
            },
            out = out_rx.recv() => match out {
                Some(bytes) => {
                    if handle.data(channel, bytes).await.is_err() {
                        return StopReason::Teardown;
                    }
                }
                None => return StopReason::ChildExited,
            },
            _ = child.wait() => return StopReason::ChildExited,
            result = shutdown_rx.changed(), if watch_live => match result {
                Ok(()) if *shutdown_rx.borrow() => return StopReason::Teardown,
                Ok(()) => {}
                Err(_) => watch_live = false,
            },
        }
    }
}

fn send_sighup(pid: u32, account: &str) {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;

    match kill(Pid::from_raw(pid as i32), Signal::SIGHUP) {
        Ok(()) => tracing::info!(pid, account, "SIGHUP -> CodeKeep for save"),
        Err(e) => tracing::debug!(pid, account, error = ?e, "SIGHUP failed; child already exited?"),
    }
}

fn set_winsize(master: &std::fs::File, cols: u16, rows: u16) {
    use std::os::fd::AsRawFd;

    let ws = nix::libc::winsize {
        ws_row: rows.max(1),
        ws_col: cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe {
        nix::libc::ioctl(master.as_raw_fd(), nix::libc::TIOCSWINSZ, &ws);
    }
}
