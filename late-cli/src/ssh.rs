use anyhow::{Context, Result};
use russh::{
    ChannelMsg, ChannelReadHalf, ChannelWriteHalf, Disconnect, client,
    keys::{self, PrivateKeyWithHashAlg, known_hosts},
};
use serde::Deserialize;
use std::{
    env, fs, io,
    io::{IsTerminal, Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tracing::{debug, info};

use super::{
    config::{Config, SshMode},
    pty::terminal_size_or_default,
};

#[cfg(unix)]
use super::pty::{PtyResizeHandle, pty_winsize};

#[cfg(unix)]
use nix::{libc, pty::openpty, unistd::setsid};

#[cfg(unix)]
use std::{os::fd::AsRawFd, process::Stdio};

#[cfg(unix)]
use tokio::process::{Child, Command};

pub(super) const CLI_MODE_ENV: &str = "LATE_CLI_MODE";
const CLI_TOKEN_PREFIX: &str = "LATE_SESSION_TOKEN=";
const CLI_TOKEN_REQUEST: &str = "late-cli-token-v1";

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
#[cfg(unix)]
const TIOCSCTTY_IOCTL_REQUEST: libc::c_ulong = libc::TIOCSCTTY as libc::c_ulong;
#[cfg(any(target_os = "android", target_env = "musl"))]
#[cfg(unix)]
const TIOCSCTTY_IOCTL_REQUEST: libc::c_int = libc::TIOCSCTTY;
#[cfg(not(any(
    target_os = "android",
    target_env = "musl",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
)))]
#[cfg(unix)]
const TIOCSCTTY_IOCTL_REQUEST: libc::c_ulong = libc::TIOCSCTTY;

#[derive(Debug)]
pub(super) enum SshExit {
    Clean,
    ProcessStatus {
        status: std::process::ExitStatus,
        stdout_closed_cleanly: bool,
    },
    RemoteStatus {
        code: Option<u32>,
    },
    RemoteSignal {
        signal_name: String,
        message: String,
    },
}

impl SshExit {
    pub(super) fn ensure_success(self) -> Result<()> {
        match self {
            Self::Clean => Ok(()),
            Self::ProcessStatus {
                status,
                stdout_closed_cleanly,
            } if status.success() || status.code() == Some(255) && stdout_closed_cleanly => Ok(()),
            Self::ProcessStatus { status, .. } => anyhow::bail!("ssh exited with status {status}"),
            Self::RemoteStatus { code: None } | Self::RemoteStatus { code: Some(0) } => Ok(()),
            Self::RemoteStatus { code: Some(code) } => {
                anyhow::bail!("ssh session exited with status {code}")
            }
            Self::RemoteSignal {
                signal_name,
                message,
            } if message.is_empty() => {
                anyhow::bail!("ssh session terminated by signal {signal_name}")
            }
            Self::RemoteSignal {
                signal_name,
                message,
            } => anyhow::bail!("ssh session terminated by signal {signal_name}: {message}"),
        }
    }
}

pub(super) struct SshProcess {
    pub(super) completion_task: JoinHandle<Result<SshExit>>,
    pub(super) input_task: JoinHandle<Result<()>>,
    pub(super) resize_handle: ResizeHandle,
    pub(super) input_gate: Arc<AtomicBool>,
}

pub(super) enum ResizeHandle {
    #[cfg(unix)]
    Subprocess(PtyResizeHandle),
    Native(mpsc::UnboundedSender<WriterCommand>),
}

pub(super) enum WriterCommand {
    Data(Vec<u8>),
    Eof,
    WindowChange { cols: u16, rows: u16 },
    Close,
}

pub(super) async fn spawn_ssh(
    config: &Config,
    identity_file: &Path,
    token_tx: oneshot::Sender<String>,
) -> Result<SshProcess> {
    match config.ssh_mode {
        SshMode::Subprocess => spawn_subprocess_ssh(config, identity_file, token_tx).await,
        SshMode::Native => spawn_native_ssh(config, identity_file, token_tx).await,
    }
}

pub(super) async fn forward_resize_events(handle: ResizeHandle) {
    #[cfg(unix)]
    {
        let Ok(mut sigwinch) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())
        else {
            return;
        };

        while sigwinch.recv().await.is_some() {
            if let Err(err) = apply_resize(&handle).await {
                debug!(error = ?err, "failed to forward local terminal resize");
            }
        }
    }

    #[cfg(not(unix))]
    {
        let mut last_size = terminal_size_or_default();
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            interval.tick().await;
            let current = terminal_size_or_default();
            if current != last_size {
                last_size = current;
                if let Err(err) = apply_resize(&handle).await {
                    debug!(error = ?err, "failed to forward local terminal resize");
                    break;
                }
            }
        }
    }
}

#[cfg(unix)]
async fn apply_resize(handle: &ResizeHandle) -> Result<()> {
    match handle {
        ResizeHandle::Subprocess(handle) => handle.resize_to_current(),
        ResizeHandle::Native(tx) => {
            let (cols, rows) = terminal_size_or_default();
            tx.send(WriterCommand::WindowChange { cols, rows })
                .map_err(|_| anyhow::anyhow!("native ssh resize channel is closed"))?;
            Ok(())
        }
    }
}

#[cfg(not(unix))]
async fn apply_resize(handle: &ResizeHandle) -> Result<()> {
    match handle {
        ResizeHandle::Native(tx) => {
            let (cols, rows) = terminal_size_or_default();
            tx.send(WriterCommand::WindowChange { cols, rows })
                .map_err(|_| anyhow::anyhow!("native ssh resize channel is closed"))?;
            Ok(())
        }
    }
}

#[cfg(unix)]
async fn spawn_subprocess_ssh(
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
        .arg(format!("SendEnv={CLI_MODE_ENV}"));

    if let Some(port) = config.ssh_port {
        cmd.arg("-p").arg(port.to_string());
    }
    if let Some(user) = config.ssh_user.as_deref() {
        cmd.arg("-l").arg(user);
    }

    cmd.arg(&config.ssh_target)
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
        tokio::task::spawn_blocking(move || forward_stdin_to_pty(input_pty, input_gate_for_task));
    let completion_task =
        tokio::spawn(async move { wait_for_subprocess_exit(child, output_task).await });

    Ok(SshProcess {
        completion_task,
        input_task,
        resize_handle: ResizeHandle::Subprocess(PtyResizeHandle { master }),
        input_gate,
    })
}

#[cfg(not(unix))]
async fn spawn_subprocess_ssh(
    _config: &Config,
    _identity_file: &Path,
    _token_tx: oneshot::Sender<String>,
) -> Result<SshProcess> {
    anyhow::bail!("subprocess ssh mode is only available on Unix; use --ssh-mode native");
}

async fn spawn_native_ssh(
    config: &Config,
    identity_file: &Path,
    token_tx: oneshot::Sender<String>,
) -> Result<SshProcess> {
    let target = ResolvedTarget::from_config(config)?;
    let private_key = keys::load_secret_key(identity_file, None).with_context(|| {
        format!(
            "failed to load SSH identity from {}",
            identity_file.display()
        )
    })?;
    let handler = NativeClientHandler {
        host: target.host.clone(),
        port: target.port,
    };
    let client_config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    });

    let mut session = client::connect(client_config, (target.host.as_str(), target.port), handler)
        .await
        .with_context(|| format!("failed to connect to {}:{}", target.host, target.port))?;

    let auth = session
        .authenticate_publickey(
            target.user.clone(),
            PrivateKeyWithHashAlg::new(
                Arc::new(private_key),
                session.best_supported_rsa_hash().await?.flatten(),
            ),
        )
        .await
        .with_context(|| {
            format!(
                "failed to authenticate to {}:{} as {}",
                target.host, target.port, target.user
            )
        })?;
    if !auth.success() {
        anyhow::bail!(
            "public key authentication failed for {}@{}:{}",
            target.user,
            target.host,
            target.port
        );
    }

    let token = fetch_native_session_token(&session)
        .await
        .context("failed to fetch session token over native ssh handshake")?;
    let _ = token_tx.send(token);

    let channel = session
        .channel_open_session()
        .await
        .context("failed to open native ssh session channel")?;
    let (cols, rows) = terminal_size_or_default();
    let term = env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string());
    channel
        .request_pty(true, &term, cols as u32, rows as u32, 0, 0, &[])
        .await
        .context("failed to request ssh pty")?;
    channel
        .request_shell(true)
        .await
        .context("failed to request remote shell")?;

    let (mut read_half, write_half) = channel.split();
    let input_gate = Arc::new(AtomicBool::new(false));
    let input_gate_for_task = Arc::clone(&input_gate);
    let (writer_tx, writer_rx) = mpsc::unbounded_channel();
    let writer_tx_for_completion = writer_tx.clone();
    let writer_tx_for_resize = writer_tx.clone();

    let writer_task = tokio::spawn(async move { drive_native_writer(write_half, writer_rx).await });
    let input_task = tokio::task::spawn_blocking(move || {
        forward_stdin_to_native(writer_tx, input_gate_for_task)
    });
    let completion_task = tokio::spawn(async move {
        let exit = drive_native_output(&mut read_half).await;
        let _ = writer_tx_for_completion.send(WriterCommand::Close);
        let _ = writer_task.await;
        let _ = session
            .disconnect(Disconnect::ByApplication, "", "en")
            .await;
        exit
    });

    Ok(SshProcess {
        completion_task,
        input_task,
        resize_handle: ResizeHandle::Native(writer_tx_for_resize),
        input_gate,
    })
}

#[derive(Deserialize)]
struct SessionTokenResponse {
    session_token: String,
}

async fn fetch_native_session_token(
    session: &client::Handle<NativeClientHandler>,
) -> Result<String> {
    let mut channel = session
        .channel_open_session()
        .await
        .context("failed to open native ssh token channel")?;
    channel
        .exec(true, CLI_TOKEN_REQUEST)
        .await
        .context("failed to request native ssh token handshake")?;

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut exit_code = None;

    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => stdout.extend_from_slice(data.as_ref()),
            ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(data.as_ref()),
            ChannelMsg::ExitStatus { exit_status } => exit_code = Some(exit_status),
            ChannelMsg::Failure => anyhow::bail!("server rejected the native ssh token handshake"),
            ChannelMsg::Close => break,
            _ => {}
        }
    }

    if let Some(code) = exit_code
        && code != 0
    {
        let stderr = String::from_utf8_lossy(&stderr);
        anyhow::bail!("native ssh token handshake exited with status {code}: {stderr}");
    }

    if !stderr.is_empty() {
        debug!(
            stderr = %String::from_utf8_lossy(&stderr),
            "native ssh token handshake wrote stderr"
        );
    }

    let response: SessionTokenResponse = serde_json::from_slice(&stdout)
        .context("native ssh token handshake returned invalid JSON")?;
    if response.session_token.trim().is_empty() {
        anyhow::bail!("native ssh token handshake returned an empty session token");
    }
    Ok(response.session_token)
}

#[cfg(unix)]
fn nix_to_io_error(err: nix::Error) -> io::Error {
    io::Error::from_raw_os_error(err as i32)
}

#[cfg(unix)]
async fn wait_for_subprocess_exit(
    mut child: Child,
    mut output_task: JoinHandle<Result<()>>,
) -> Result<SshExit> {
    let mut stdout_result = None;
    let mut stdout_task_consumed = false;
    let exit = tokio::select! {
        status = child.wait() => {
            let status = status.context("ssh process failed to exit cleanly")?;
            SshExit::ProcessStatus {
                status,
                stdout_closed_cleanly: false,
            }
        }
        stdout = &mut output_task => {
            stdout_task_consumed = true;
            match stdout {
                Ok(Ok(())) => {
                    info!("ssh stdout closed; treating session as ended");
                    stdout_result = Some(Ok(Ok(())));
                }
                Ok(Err(err)) => return Err(err.context("ssh stdout forwarding failed")),
                Err(err) => return Err(anyhow::anyhow!("ssh stdout task join failed: {err}")),
            }
            SshExit::Clean
        }
    };

    let exit = match exit {
        SshExit::ProcessStatus { status, .. } => {
            info!(%status, "ssh session exited");
            SshExit::ProcessStatus {
                status,
                stdout_closed_cleanly: matches!(stdout_result, Some(Ok(Ok(())))),
            }
        }
        SshExit::Clean => {
            if let Err(err) = child.start_kill() {
                debug!(error = ?err, "failed to kill lingering ssh wrapper after stdout closed");
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
            SshExit::Clean
        }
        other => other,
    };

    if !stdout_task_consumed && output_task.is_finished() {
        stdout_result = Some(output_task.await);
    } else if !stdout_task_consumed {
        output_task.abort();
        let _ = output_task.await;
    }

    if let SshExit::ProcessStatus { status, .. } = exit {
        Ok(SshExit::ProcessStatus {
            status,
            stdout_closed_cleanly: matches!(stdout_result, Some(Ok(Ok(())))),
        })
    } else {
        Ok(exit)
    }
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

async fn drive_native_output(read_half: &mut ChannelReadHalf) -> Result<SshExit> {
    let mut stdout = tokio::io::stdout();
    let mut stderr = tokio::io::stderr();
    let mut exit_code = None;
    let mut exit_signal = None;

    while let Some(msg) = read_half.wait().await {
        match msg {
            ChannelMsg::Data { data } => {
                stdout.write_all(data.as_ref()).await?;
                stdout.flush().await?;
            }
            ChannelMsg::ExtendedData { data, .. } => {
                stderr.write_all(data.as_ref()).await?;
                stderr.flush().await?;
            }
            ChannelMsg::ExitStatus { exit_status } => {
                exit_code = Some(exit_status);
            }
            ChannelMsg::ExitSignal {
                signal_name,
                error_message,
                ..
            } => {
                exit_signal = Some((render_signal_name(&signal_name), error_message));
            }
            ChannelMsg::Close => break,
            ChannelMsg::Eof => {}
            ChannelMsg::Failure => debug!("native ssh channel request failed"),
            ChannelMsg::Success => {}
            _ => {}
        }
    }

    if let Some((signal_name, message)) = exit_signal {
        return Ok(SshExit::RemoteSignal {
            signal_name,
            message,
        });
    }

    Ok(SshExit::RemoteStatus { code: exit_code })
}

fn render_signal_name(signal_name: &russh::Sig) -> String {
    match signal_name {
        russh::Sig::ABRT => "ABRT".to_string(),
        russh::Sig::ALRM => "ALRM".to_string(),
        russh::Sig::FPE => "FPE".to_string(),
        russh::Sig::HUP => "HUP".to_string(),
        russh::Sig::ILL => "ILL".to_string(),
        russh::Sig::INT => "INT".to_string(),
        russh::Sig::KILL => "KILL".to_string(),
        russh::Sig::PIPE => "PIPE".to_string(),
        russh::Sig::QUIT => "QUIT".to_string(),
        russh::Sig::SEGV => "SEGV".to_string(),
        russh::Sig::TERM => "TERM".to_string(),
        russh::Sig::USR1 => "USR1".to_string(),
        russh::Sig::Custom(name) => name.clone(),
    }
}

async fn drive_native_writer(
    write_half: ChannelWriteHalf<russh::client::Msg>,
    mut rx: mpsc::UnboundedReceiver<WriterCommand>,
) -> Result<()> {
    while let Some(command) = rx.recv().await {
        match command {
            WriterCommand::Data(data) => {
                write_half
                    .data(data.as_slice())
                    .await
                    .context("failed to forward stdin to native ssh channel")?;
            }
            WriterCommand::Eof => {
                let _ = write_half.eof().await;
            }
            WriterCommand::WindowChange { cols, rows } => {
                write_half
                    .window_change(cols as u32, rows as u32, 0, 0)
                    .await
                    .context("failed to forward terminal resize to native ssh channel")?;
            }
            WriterCommand::Close => {
                let _ = write_half.close().await;
                break;
            }
        }
    }

    Ok(())
}

pub(super) fn flush_stdin_input_queue() {
    if !std::io::stdin().is_terminal() {
        return;
    }

    #[cfg(unix)]
    {
        let rc = unsafe { libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH) };
        if rc == -1 {
            debug!(
                error = ?io::Error::last_os_error(),
                "failed to flush pending stdin before enabling ssh input"
            );
        }
    }
}

#[cfg(unix)]
fn forward_stdin_to_pty(mut pty: fs::File, input_gate: Arc<AtomicBool>) -> Result<()> {
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

fn forward_stdin_to_native(
    tx: mpsc::UnboundedSender<WriterCommand>,
    input_gate: Arc<AtomicBool>,
) -> Result<()> {
    let mut stdin = std::io::stdin().lock();
    let mut buf = [0u8; 4096];
    loop {
        let n = match stdin.read(&mut buf) {
            Ok(n) => n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err.into()),
        };
        if n == 0 {
            if input_gate.load(Ordering::Relaxed) {
                let _ = tx.send(WriterCommand::Eof);
            }
            break;
        }
        if !input_gate.load(Ordering::Relaxed) {
            continue;
        }
        if tx.send(WriterCommand::Data(buf[..n].to_vec())).is_err() {
            break;
        }
    }
    Ok(())
}

#[derive(Debug)]
struct NativeClientHandler {
    host: String,
    port: u16,
}

impl client::Handler for NativeClientHandler {
    type Error = anyhow::Error;

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        eprint!("{banner}");
        Ok(())
    }

    async fn check_server_key(
        &mut self,
        server_public_key: &keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        match known_hosts::check_known_hosts(&self.host, self.port, server_public_key) {
            Ok(true) => Ok(true),
            Ok(false) => {
                known_hosts::learn_known_hosts(&self.host, self.port, server_public_key)
                    .with_context(|| {
                        format!(
                            "failed to record server host key for {}:{}",
                            self.host, self.port
                        )
                    })?;
                info!(
                    host = %self.host,
                    port = self.port,
                    "accepted and recorded new server host key"
                );
                Ok(true)
            }
            Err(err) => Err(anyhow::anyhow!(
                "server host key verification failed for {}:{}: {err}",
                self.host,
                self.port
            )),
        }
    }
}

struct ResolvedTarget {
    host: String,
    port: u16,
    user: String,
}

impl ResolvedTarget {
    fn from_config(config: &Config) -> Result<Self> {
        let parsed = ParsedTarget::parse(&config.ssh_target)?;
        let user = config
            .ssh_user
            .clone()
            .or(parsed.user)
            .unwrap_or_else(local_username);
        let port = config.ssh_port.or(parsed.port).unwrap_or(22);

        Ok(Self {
            host: parsed.host,
            port,
            user,
        })
    }
}

struct ParsedTarget {
    host: String,
    user: Option<String>,
    port: Option<u16>,
}

impl ParsedTarget {
    fn parse(raw: &str) -> Result<Self> {
        let (user, host_port) = match raw.rsplit_once('@') {
            Some((user, host_port)) if !user.is_empty() && !host_port.is_empty() => {
                (Some(user.to_string()), host_port)
            }
            _ => (None, raw),
        };
        let (host, port) = parse_host_and_port(host_port)?;
        Ok(Self { host, user, port })
    }
}

fn parse_host_and_port(raw: &str) -> Result<(String, Option<u16>)> {
    if raw.is_empty() {
        anyhow::bail!("ssh target cannot be empty");
    }

    if let Some(rest) = raw.strip_prefix('[') {
        let end = rest
            .find(']')
            .context("invalid ssh target: missing closing ']' for IPv6 host")?;
        let host = &rest[..end];
        let tail = &rest[end + 1..];
        let port = if tail.is_empty() {
            None
        } else {
            Some(
                tail.strip_prefix(':')
                    .context("invalid ssh target after bracketed host")?
                    .parse()
                    .context("invalid ssh target port")?,
            )
        };
        return Ok((host.to_string(), port));
    }

    if raw.matches(':').count() == 1
        && let Some((host, port)) = raw.rsplit_once(':')
        && port.chars().all(|ch| ch.is_ascii_digit())
    {
        return Ok((
            host.to_string(),
            Some(port.parse().context("invalid ssh target port")?),
        ));
    }

    Ok((raw.to_string(), None))
}

fn local_username() -> String {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "late".to_string())
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

    #[test]
    fn parse_target_supports_user_and_port() {
        let parsed = ParsedTarget::parse("alice@late.sh:2222").unwrap();
        assert_eq!(parsed.user.as_deref(), Some("alice"));
        assert_eq!(parsed.host, "late.sh");
        assert_eq!(parsed.port, Some(2222));
    }

    #[test]
    fn parse_target_supports_bracketed_ipv6() {
        let parsed = ParsedTarget::parse("alice@[::1]:2222").unwrap();
        assert_eq!(parsed.user.as_deref(), Some("alice"));
        assert_eq!(parsed.host, "::1");
        assert_eq!(parsed.port, Some(2222));
    }
}
