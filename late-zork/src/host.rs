use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use russh::ChannelId;
use russh::server::Handle;
use tokio::sync::{mpsc, watch};

use crate::protocol::{Action, Edition};

const HANGUP_SAVE_GRACE: Duration = Duration::from_secs(5);

pub(crate) struct SessionLease {
    account: String,
    active_accounts: Arc<Mutex<HashMap<String, Edition>>>,
}

impl SessionLease {
    pub(crate) fn acquire(
        account: String,
        edition: Edition,
        active_accounts: Arc<Mutex<HashMap<String, Edition>>>,
    ) -> Option<Self> {
        {
            let mut active = active_accounts.lock().expect("active accounts mutex");
            if active.contains_key(&account) {
                return None;
            }
            active.insert(account.clone(), edition);
        }
        Some(Self {
            account,
            active_accounts,
        })
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
    pub(crate) story_dir: String,
    pub(crate) data_dir: String,
    pub(crate) edition: Edition,
    pub(crate) action: Action,
    pub(crate) account: String,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
}

enum Command {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Return,
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
            let result = run_bridge(cfg, cmd_rx, handle, channel, shutdown_rx).await;
            // Release after child exit and before notifying an awaiting switch.
            drop(lease);
            let code = match result {
                Ok(code) => code,
                Err(e) => {
                    tracing::warn!(error = ?e, "Zork host bridge ended with error");
                    let _ = cleanup
                        .extended_data(
                            channel,
                            1,
                            b"Unable to start Zork. Please try again.\n".to_vec(),
                        )
                        .await;
                    1
                }
            };
            let _ = cleanup.exit_status_request(channel, code).await;
            let _ = cleanup.eof(channel).await;
            let _ = cleanup.close(channel).await;
        });
        Self { cmd_tx }
    }

    pub(crate) fn send_input(&self, bytes: Vec<u8>) {
        let _ = self.cmd_tx.try_send(Command::Input(bytes));
    }

    pub(crate) fn return_to_menu(&self) {
        let _ = self.cmd_tx.try_send(Command::Return);
    }
    pub(crate) fn is_finished(&self) -> bool {
        self.cmd_tx.is_closed()
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
) -> Result<u32> {
    use std::os::fd::AsRawFd;
    use std::process::Stdio;
    use std::{fs, io};

    use anyhow::Context;
    use nix::libc;
    use nix::pty::{Winsize, openpty};
    use nix::unistd::setsid;
    use tokio::process::Command as TokioCommand;

    let home = crate::slots::directory(&cfg.data_dir, &cfg.account, cfg.edition);
    fs::create_dir_all(&home).with_context(|| format!("create Zork HOME {}", home.display()))?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        home.parent().expect("account directory"),
        fs::Permissions::from_mode(0o700),
    )?;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;

    let winsize = Winsize {
        ws_row: cfg.rows.clamp(4, 255),
        ws_col: cfg.cols.clamp(20, 255),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = openpty(Some(&winsize), None).context("failed to allocate zork pty")?;
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
        // Frotz renders into late.sh's vt100 parser, not the player's terminal.
        // Use the terminfo shipped in the image even for Kitty/Ghostty clients.
        .env("TERM", "xterm-256color")
        .env("HOME", &home)
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("LATE_FROTZ_DOOR", cfg.action.mode())
        .arg("-d")
        .arg(std::path::Path::new(&cfg.story_dir).join(format!("{}.z3", cfg.edition.key())))
        .stdin(Stdio::from(
            slave
                .try_clone()
                .context("clone zork pty slave for stdin")?,
        ))
        .stdout(Stdio::from(
            slave
                .try_clone()
                .context("clone zork pty slave for stdout")?,
        ))
        .stderr(Stdio::from(
            slave
                .try_clone()
                .context("clone zork pty slave for stderr")?,
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
        .with_context(|| format!("failed to start zork ({})", cfg.bin))?;
    drop(slave);

    let reader_master = master
        .try_clone()
        .context("clone zork pty master for reader")?;
    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(64);
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut src: &fs::File = &reader_master;
        let mut buf = [0u8; 8192];
        loop {
            match src.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if out_tx.blocking_send(buf[..n].to_vec()).is_err() {
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

    if matches!(stop, StopReason::Teardown)
        && let Some(pid) = child.id()
    {
        send_sighup(pid, &cfg.account);
    }

    let status = match tokio::time::timeout(HANGUP_SAVE_GRACE, child.wait()).await {
        Ok(status) => status?,
        Err(_) => {
            tracing::warn!(account = %cfg.account, "Zork exit grace elapsed; killing child");
            child.kill().await?;
            child.wait().await?
        }
    };
    let code = status.code().unwrap_or(1) as u32;
    while let Ok(Some(bytes)) =
        tokio::time::timeout(Duration::from_millis(100), out_rx.recv()).await
    {
        if handle.data(channel, bytes).await.is_err() {
            break;
        }
    }
    drop(master);
    drop(reader);
    Ok(code)
}

async fn bridge_loop(
    cmd_rx: &mut mpsc::Receiver<Command>,
    out_rx: &mut mpsc::Receiver<Vec<u8>>,
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
                Some(Command::Return) => {
                    if let Some(pid) = child.id() {
                        let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), nix::sys::signal::Signal::SIGUSR1);
                    }
                }
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
        Ok(()) => tracing::info!(pid, account, "SIGHUP -> Zork for save"),
        Err(e) => tracing::debug!(pid, account, error = ?e, "SIGHUP failed; child already exited?"),
    }
}

fn set_winsize(master: &std::fs::File, cols: u16, rows: u16) {
    use std::os::fd::AsRawFd;

    let ws = nix::libc::winsize {
        ws_row: rows.clamp(4, 255),
        ws_col: cols.clamp(20, 255),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe {
        nix::libc::ioctl(master.as_raw_fd(), nix::libc::TIOCSWINSZ, &ws);
    }
}
