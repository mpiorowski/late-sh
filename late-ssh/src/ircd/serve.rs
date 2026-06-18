//! ircd TCP listener. Config-gated; spawned from main alongside SSH/API.
//!
//! Shutdown policy is fast-disconnect, no drain (FRD §10 L2): on shutdown we
//! send every connection `ERROR :Server restarting` and rely on client
//! auto-reconnect against the replacement pod.

use std::{fs::File, io::BufReader, net::IpAddr, sync::Arc};

use anyhow::{Context, Result};
use late_core::{rate_limit::IpRateLimiter, shutdown::CancellationToken};
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Semaphore,
};
use tokio_rustls::TlsAcceptor;

use super::conn;
use crate::{config::IrcConfig, state::State};

pub async fn run(state: State, shutdown: Option<CancellationToken>) -> Result<()> {
    let config = state.config.irc.clone();
    let tls_acceptor = load_tls_acceptor(&config)?;
    let listener = TcpListener::bind(("0.0.0.0", config.port)).await?;
    run_with_listener(state, shutdown, listener, tls_acceptor).await
}

pub async fn run_with_listener(
    state: State,
    shutdown: Option<CancellationToken>,
    listener: TcpListener,
    tls_acceptor: Option<TlsAcceptor>,
) -> Result<()> {
    let config = state.config.irc.clone();
    tracing::info!(
        port = listener
            .local_addr()
            .map(|addr| addr.port())
            .unwrap_or(config.port),
        tls = tls_acceptor.is_some(),
        "ircd listening"
    );
    let auth_limiter = IpRateLimiter::new(
        config.max_auth_failures_per_ip,
        config.auth_failure_window_secs,
    );
    let conn_limit = Arc::new(Semaphore::new(config.max_conns_global));

    loop {
        tokio::select! {
            _ = cancelled(&shutdown) => break,
            accepted = listener.accept() => {
                let (stream, addr) = match accepted {
                    Ok(accepted) => accepted,
                    Err(err) => {
                        tracing::warn!(error = %err, "ircd: accept failed");
                        continue;
                    }
                };
                let peer_ip: IpAddr = addr.ip();
                if state.is_draining.load(std::sync::atomic::Ordering::Relaxed) {
                    reject(stream, tls_acceptor.clone(), "Server restarting").await;
                    continue;
                }
                let Ok(conn_permit) = conn_limit.clone().try_acquire_owned() else {
                    reject(stream, tls_acceptor.clone(), "Too many connections").await;
                    continue;
                };
                let conn_state = state.clone();
                let conn_limiter = auth_limiter.clone();
                let conn_tls_acceptor = tls_acceptor.clone();
                tokio::spawn(async move {
                    let _conn_permit = conn_permit;
                    let result = if let Some(acceptor) = conn_tls_acceptor {
                        match acceptor.accept(stream).await {
                            Ok(tls_stream) => conn::handle(conn_state, tls_stream, peer_ip, conn_limiter).await,
                            Err(err) => {
                                tracing::debug!(error = %err, %peer_ip, "ircd: TLS handshake failed");
                                return;
                            }
                        }
                    } else {
                        conn::handle(conn_state, stream, peer_ip, conn_limiter).await
                    };
                    if let Err(err) = result {
                        tracing::debug!(error = %err, %peer_ip, "ircd: connection ended with error");
                    }
                });
            }
        }
    }

    let disconnected = state.irc_registry.disconnect_all("Server restarting");
    tracing::info!(disconnected, "ircd: shutdown, disconnected clients");
    Ok(())
}

fn load_tls_acceptor(config: &IrcConfig) -> Result<Option<TlsAcceptor>> {
    let (Some(cert_path), Some(key_path)) = (&config.tls_cert_path, &config.tls_key_path) else {
        return Ok(None);
    };

    if rustls::crypto::CryptoProvider::get_default().is_none() {
        rustls::crypto::ring::default_provider()
            .install_default()
            .map_err(|_| anyhow::anyhow!("failed to install rustls ring crypto provider"))?;
    }

    let cert_file = File::open(cert_path)
        .with_context(|| format!("failed to open LATE_IRC_TLS_CERT {}", cert_path.display()))?;
    let certs = rustls_pemfile::certs(&mut BufReader::new(cert_file))
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to read LATE_IRC_TLS_CERT {}", cert_path.display()))?;
    if certs.is_empty() {
        anyhow::bail!(
            "LATE_IRC_TLS_CERT {} contains no certificates",
            cert_path.display()
        );
    }

    let key_file = File::open(key_path)
        .with_context(|| format!("failed to open LATE_IRC_TLS_KEY {}", key_path.display()))?;
    let key = rustls_pemfile::private_key(&mut BufReader::new(key_file))
        .with_context(|| format!("failed to read LATE_IRC_TLS_KEY {}", key_path.display()))?
        .with_context(|| {
            format!(
                "LATE_IRC_TLS_KEY {} contains no private key",
                key_path.display()
            )
        })?;

    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("failed to build IRC TLS config from certificate and key")?;
    Ok(Some(TlsAcceptor::from(Arc::new(server_config))))
}

async fn cancelled(shutdown: &Option<CancellationToken>) {
    match shutdown {
        Some(token) => token.cancelled().await,
        None => std::future::pending().await,
    }
}

/// Best-effort ERROR line for connections refused before registration.
async fn reject(stream: TcpStream, tls_acceptor: Option<TlsAcceptor>, reason: &str) {
    if let Some(acceptor) = tls_acceptor {
        match acceptor.accept(stream).await {
            Ok(mut stream) => write_error_and_close(&mut stream, reason).await,
            Err(err) => tracing::debug!(error = %err, "ircd: TLS reject handshake failed"),
        }
        return;
    }

    let mut stream = stream;
    write_error_and_close(&mut stream, reason).await;
}

async fn write_error_and_close<S>(stream: &mut S, reason: &str)
where
    S: AsyncWrite + Unpin,
{
    let line = format!("ERROR :{reason}\r\n");
    let _ = stream.write_all(line.as_bytes()).await;
    let _ = stream.shutdown().await;
}
