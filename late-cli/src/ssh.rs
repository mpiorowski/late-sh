use anyhow::{Context, Result};
use nix::{libc, pty::openpty, unistd::setsid};
use std::{
    fs, io,
    io::{IsTerminal, Read, Write},
    os::fd::AsRawFd,
    path::Path,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    process::{Child, Command},
    sync::oneshot,
};
use tracing::debug;

use super::config::Config;
use super::pty::{PtyResizeHandle, pty_winsize, terminal_size_or_default};

pub(super) const CLI_MODE_ENV: &str = "LATE_CLI_MODE";
const CLI_TOKEN_PREFIX: &str = "LATE_SESSION_TOKEN=";

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
const TIOCSCTTY_IOCTL_REQUEST: libc::c_ulong = libc::TIOCSCTTY as libc::c_ulong;
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
)))]
const TIOCSCTTY_IOCTL_REQUEST: libc::c_ulong = libc::TIOCSCTTY;

pub(super) enum SshExit {
    Process(std::process::ExitStatus),
    StdoutClosed,
}

pub(super) struct SshProcess {
    pub(super) child: Child,
    pub(super) output_task: tokio::task::JoinHandle<Result<()>>,
    pub(super) input_task: tokio::task::JoinHandle<Result<()>>,
    pub(super) resize_handle: PtyResizeHandle,
    pub(super) input_gate: Arc<AtomicBool>,
}

pub(super) async fn spawn_ssh(
    config: &Config,
    identity_file: &Path,
    token_tx: oneshot::Sender<String>,
) -> Result<SshProcess> {
    let (cols, rows) = terminal_size_or_default();
    let winsize = pty_winsize(cols, rows);
    let pty = openpty(Some(&winsize), None).context("failed to allocate local ssh pty")?;
    let master = Arc::new(fs::File::from(pty.master));
    let slave = fs::File::from(pty.slave);
    let slave_fd = slave.as_raw_fd();

    let (ssh_program, ssh_args) = config
        .ssh_bin
        .split_first()
        .context("ssh client command is empty")?;
    let mut cmd = Command::new(ssh_program);
    cmd.env(CLI_MODE_ENV, "1")
        .args(ssh_args)
        .arg("-i")
        .arg(identity_file)
        .arg("-tt")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg(format!("SendEnv={CLI_MODE_ENV}"))
        .arg(&config.ssh_target)
        .stdin(Stdio::from(
            slave
                .try_clone()
                .context("failed to clone ssh pty slave for stdin")?,
        ))
        .stdout(Stdio::from(
            slave
                .try_clone()
                .context("failed to clone ssh pty slave for stdout")?,
        ))
        .stderr(Stdio::from(
            slave
                .try_clone()
                .context("failed to clone ssh pty slave for stderr")?,
        ))
        .kill_on_drop(true);

    unsafe {
        cmd.pre_exec(move || {
            setsid().map_err(nix_to_io_error)?;
            if libc::ioctl(slave_fd, TIOCSCTTY_IOCTL_REQUEST, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = cmd.spawn().context("failed to start ssh session")?;
    drop(slave);

    let output_pty = master
        .try_clone()
        .context("failed to clone ssh pty master for output forwarding")?;
    let input_pty = master
        .try_clone()
        .context("failed to clone ssh pty master for input forwarding")?;
    let input_gate = Arc::new(AtomicBool::new(false));
    let input_gate_for_task = Arc::clone(&input_gate);

    let output_task = tokio::task::spawn_blocking(move || forward_ssh_output(output_pty, token_tx));
    let input_task =
        tokio::task::spawn_blocking(move || forward_stdin(input_pty, input_gate_for_task));

    Ok(SshProcess {
        child,
        output_task,
        input_task,
        resize_handle: PtyResizeHandle { master },
        input_gate,
    })
}

fn nix_to_io_error(err: nix::Error) -> io::Error {
    io::Error::from_raw_os_error(err as i32)
}

fn forward_ssh_output(mut pty: fs::File, token_tx: oneshot::Sender<String>) -> Result<()> {
    let mut pending = Vec::new();
    let mut buf = [0u8; 4096];
    let mut out = std::io::stdout();
    let mut token_sent = false;
    let mut token_tx = Some(token_tx);

    loop {
        let n = match pty.read(&mut buf) {
            Ok(n) => n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err.into()),
        };
        if n == 0 {
            break;
        }

        if token_sent {
            out.write_all(&buf[..n])?;
            out.flush()?;
            continue;
        }

        pending.extend_from_slice(&buf[..n]);

        while !pending.is_empty() && !token_sent {
            match parse_cli_banner(&pending) {
                BannerState::NeedMore => break,
                BannerState::Token { token, consumed } => {
                    if let Some(token_tx) = token_tx.take() {
                        let _ = token_tx.send(token);
                    }
                    debug!("captured cli session token banner");
                    if consumed < pending.len() {
                        out.write_all(&pending[consumed..])?;
                        out.flush()?;
                    }
                    pending.clear();
                    token_sent = true;
                }
                BannerState::Passthrough { consumed } => {
                    out.write_all(&pending[..consumed])?;
                    out.flush()?;
                    pending.drain(..consumed);
                }
            }
        }
    }

    if !pending.is_empty() {
        out.write_all(&pending)?;
        out.flush()?;
    }

    Ok(())
}

pub(super) fn flush_stdin_input_queue() {
    if !std::io::stdin().is_terminal() {
        return;
    }

    let rc = unsafe { libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH) };
    if rc == -1 {
        debug!(
            error = ?io::Error::last_os_error(),
            "failed to flush pending stdin before enabling ssh input"
        );
    }
}

fn forward_stdin(mut pty: fs::File, input_gate: Arc<AtomicBool>) -> Result<()> {
    let mut stdin = std::io::stdin().lock();
    let mut buf = [0u8; 4096];
    loop {
        let n = match stdin.read(&mut buf) {
            Ok(n) => n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err.into()),
        };
        if n == 0 {
            break;
        }
        if !input_gate.load(Ordering::Relaxed) {
            continue;
        }
        pty.write_all(&buf[..n])?;
    }
    Ok(())
}

enum BannerState {
    NeedMore,
    Token { token: String, consumed: usize },
    Passthrough { consumed: usize },
}

fn parse_cli_banner(buf: &[u8]) -> BannerState {
    let Some(newline_idx) = buf.iter().position(|b| *b == b'\n') else {
        return BannerState::NeedMore;
    };

    let line = &buf[..=newline_idx];
    let text = String::from_utf8_lossy(line);
    if let Some(rest) = text.strip_prefix(CLI_TOKEN_PREFIX) {
        return BannerState::Token {
            token: rest.trim().to_string(),
            consumed: newline_idx + 1,
        };
    }

    BannerState::Passthrough {
        consumed: newline_idx + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_banner_extracts_token_and_consumed_bytes() {
        let buf = b"LATE_SESSION_TOKEN=abc-123\r\n\x1b[?1049h";
        match parse_cli_banner(buf) {
            BannerState::Token { token, consumed } => {
                assert_eq!(token, "abc-123");
                assert_eq!(consumed, 28);
            }
            _ => panic!("expected token banner"),
        }
    }

    #[test]
    fn parse_cli_banner_passthroughs_regular_output() {
        let buf = b"hello\r\nworld";
        match parse_cli_banner(buf) {
            BannerState::Passthrough { consumed } => assert_eq!(consumed, 7),
            _ => panic!("expected passthrough"),
        }
    }
}
