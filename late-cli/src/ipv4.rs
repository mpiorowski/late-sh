//! IPv4-only connection strategy for the `late` CLI.
//!
//! late.sh users whose network advertises IPv6 but cannot reliably use it
//! hit two distinct failures with the default address-family behavior of
//! both russh and `ssh(1)`:
//!
//! 1. **Silent v6 blackhole.** No SYN-ACK ever comes back from the v6
//!    address. OpenSSH waits out its connect timeout (~75 s) before
//!    falling back to v4, leaving the user staring at a hung terminal.
//!
//! 2. **v6 TCP works but the SSH session is closed immediately.** The
//!    user sees `Connection closed by <v6-addr> port 22` within
//!    milliseconds — something in front of the v6 endpoint (firewall,
//!    fail2ban, ban list, misconfigured listener) accepted the TCP
//!    handshake and then reset or hung up before banner exchange.
//!    Crucially, the address-family fallback that OpenSSH and most
//!    naive happy-eyeballs implementations ship does NOT cover this
//!    case: a successful `connect()` followed by EOF is treated as
//!    fatal, so v4 is never tried.
//!
//! late.sh is a single, well-known endpoint with a stable A record, and
//! the latency win from preferring v6 over v4 is negligible for an
//! interactive TUI session. Rather than implementing happy eyeballs and
//! an SSH-handshake-level retry on top, we simply never resolve or dial
//! AAAA. Both transports are routed through this module:
//!
//! * **Native (russh)**: [`connect_ipv4_only`] resolves the target via
//!   `getaddrinfo`, drops v6 candidates, and hands a connected
//!   `TcpStream` to `russh::client::connect_stream`.
//!
//! * **Subprocess modes (`openssh`, `old`)**: [`apply_ipv4_default`]
//!   prepends `-4` to the ssh argv at config-load time. All five places
//!   that consume `config.ssh_bin` (the four openssh-mode helpers and
//!   `spawn_subprocess_ssh`) automatically pick it up — none of them
//!   need to know about address families.

use std::net::SocketAddr;

use anyhow::{Context, Result};
use tokio::net::{TcpStream, lookup_host};

use crate::config::SshMode;

/// Resolve `host:port` and return a connected [`TcpStream`] to the first
/// IPv4 address. Errors if the host has no A record (i.e. the target is
/// IPv6-only) — late.sh is a dual-stack service so this is treated as a
/// hard failure rather than a silent fallback.
pub(crate) async fn connect_ipv4_only(host: &str, port: u16) -> Result<TcpStream> {
    let addr: SocketAddr = lookup_host((host, port))
        .await
        .with_context(|| format!("failed to resolve {host}:{port}"))?
        .find(SocketAddr::is_ipv4)
        .with_context(|| {
            format!("no IPv4 address found for {host}; the host appears to be IPv6-only")
        })?;

    TcpStream::connect(addr)
        .await
        .with_context(|| format!("failed to connect to {addr}"))
}

/// Inject `-4` into the ssh argv used by the subprocess SSH modes so
/// that `ssh(1)` skips AAAA resolution entirely. Returns `ssh_bin`
/// unchanged for [`SshMode::Native`] (the russh transport doesn't go
/// through `ssh(1)` and is handled by [`connect_ipv4_only`] instead).
///
/// `-4` is placed immediately after the program name so it precedes any
/// destination args appended later by the subprocess builders. If the
/// user supplied `--ssh-bin "ssh -6"`, the `-6` lands after our `-4`
/// and OpenSSH's last-flag-wins ordering for address-family flags means
/// the user's choice still takes effect — no explicit escape hatch is
/// needed in this PR.
pub(crate) fn apply_ipv4_default(ssh_bin: Vec<String>, mode: SshMode) -> Vec<String> {
    if !matches!(mode, SshMode::OpenSsh | SshMode::Subprocess) || ssh_bin.is_empty() {
        return ssh_bin;
    }
    let mut out = Vec::with_capacity(ssh_bin.len() + 1);
    let mut iter = ssh_bin.into_iter();
    out.push(iter.next().expect("non-empty checked above"));
    out.push("-4".to_string());
    out.extend(iter);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepends_minus_four_in_openssh_mode() {
        assert_eq!(
            apply_ipv4_default(vec!["ssh".into()], SshMode::OpenSsh),
            vec!["ssh", "-4"],
        );
    }

    #[test]
    fn prepends_minus_four_in_subprocess_mode() {
        assert_eq!(
            apply_ipv4_default(vec!["ssh".into(), "-vvv".into()], SshMode::Subprocess),
            vec!["ssh", "-4", "-vvv"],
        );
    }

    #[test]
    fn leaves_native_mode_untouched() {
        // Native mode goes through connect_ipv4_only, not the ssh argv.
        assert_eq!(
            apply_ipv4_default(vec!["ssh".into()], SshMode::Native),
            vec!["ssh"],
        );
    }
}
