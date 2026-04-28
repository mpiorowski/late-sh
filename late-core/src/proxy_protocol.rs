//! HAProxy PROXY protocol v1 parsing.
//!
//! Used by `late-ssh` (today) and `late-bastion` (Phase 3) to recover the
//! real client IP when fronted by NGINX TCP passthrough. v1 is the textual
//! variant terminated by `\r\n` and capped at 108 bytes per the spec.

use anyhow::{Context, Result};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Maximum length of a v1 PROXY header line (incl. CRLF), per the spec.
pub const PROXY_V1_MAX_LEN: usize = 108;

/// Read a PROXY v1 header from the front of a TCP stream and return the
/// asserted source `SocketAddr`.
///
/// Returns:
/// - `Ok(Some(addr))` — header parsed; source family was `TCP4` or `TCP6`.
/// - `Ok(None)`       — header parsed; source family was `UNKNOWN` (the
///   spec-blessed way for the proxy to disclaim knowledge of the client).
/// - `Err(_)`         — malformed header, IO error, or header timeout.
///
/// The caller is responsible for deciding whether to trust the asserted
/// address (e.g. a CIDR check on the transport peer).
pub async fn read_proxy_v1_client_addr(
    stream: &mut TcpStream,
    timeout_duration: Duration,
) -> Result<Option<SocketAddr>> {
    let mut line = Vec::with_capacity(PROXY_V1_MAX_LEN);
    let mut byte = [0u8; 1];

    let read_future = async {
        while line.len() < PROXY_V1_MAX_LEN {
            stream.read_exact(&mut byte).await?;
            line.push(byte[0]);
            if line.len() >= 2 && line[line.len() - 2..] == *b"\r\n" {
                return parse_proxy_v1_addr(&line);
            }
        }
        anyhow::bail!(
            "proxy protocol v1 header exceeded {} bytes",
            PROXY_V1_MAX_LEN
        );
    };

    match timeout(timeout_duration, read_future).await {
        Ok(Ok(addr)) => Ok(addr),
        Ok(Err(e)) => Err(e.context("failed to read proxy protocol header")),
        Err(_) => anyhow::bail!("timed out waiting for proxy protocol header"),
    }
}

/// Parse a complete PROXY v1 header line (including the trailing `\r\n`)
/// into its asserted source `SocketAddr`.
///
/// Pure logic; useful as a unit-test target and for parsing buffers we
/// already have in hand.
pub fn parse_proxy_v1_addr(line: &[u8]) -> Result<Option<SocketAddr>> {
    let text = std::str::from_utf8(line).context("proxy v1 header is not valid UTF-8")?;
    let text = text
        .strip_suffix("\r\n")
        .ok_or_else(|| anyhow::anyhow!("proxy v1 header missing CRLF terminator"))?;
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() < 2 || parts[0] != "PROXY" {
        anyhow::bail!("proxy v1 header malformed");
    }
    match parts[1] {
        "UNKNOWN" => Ok(None),
        "TCP4" | "TCP6" => {
            if parts.len() != 6 {
                anyhow::bail!("proxy v1 TCP header has unexpected field count");
            }
            let src_ip: IpAddr = parts[2]
                .parse()
                .with_context(|| format!("invalid proxy v1 source IP '{}'", parts[2]))?;
            let src_port: u16 = parts[4]
                .parse()
                .with_context(|| format!("invalid proxy v1 source port '{}'", parts[4]))?;
            Ok(Some(SocketAddr::new(src_ip, src_port)))
        }
        fam => anyhow::bail!("unsupported proxy v1 protocol family '{fam}'"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tcp4_header() {
        let line = b"PROXY TCP4 192.0.2.1 198.51.100.1 56324 443\r\n";
        let addr = parse_proxy_v1_addr(line).unwrap().unwrap();
        assert_eq!(addr.ip(), "192.0.2.1".parse::<IpAddr>().unwrap());
        assert_eq!(addr.port(), 56324);
    }

    #[test]
    fn parses_tcp6_header() {
        let line = b"PROXY TCP6 2001:db8::1 2001:db8::2 12345 443\r\n";
        let addr = parse_proxy_v1_addr(line).unwrap().unwrap();
        assert_eq!(addr.ip(), "2001:db8::1".parse::<IpAddr>().unwrap());
        assert_eq!(addr.port(), 12345);
    }

    #[test]
    fn unknown_family_returns_none() {
        let line = b"PROXY UNKNOWN\r\n";
        assert!(parse_proxy_v1_addr(line).unwrap().is_none());
    }

    #[test]
    fn missing_crlf_is_error() {
        let line = b"PROXY TCP4 192.0.2.1 198.51.100.1 56324 443";
        assert!(parse_proxy_v1_addr(line).is_err());
    }

    #[test]
    fn malformed_prefix_is_error() {
        let line = b"NOTPROXY TCP4 192.0.2.1 198.51.100.1 56324 443\r\n";
        assert!(parse_proxy_v1_addr(line).is_err());
    }

    #[test]
    fn unsupported_family_is_error() {
        let line = b"PROXY TCPX 192.0.2.1 198.51.100.1 56324 443\r\n";
        assert!(parse_proxy_v1_addr(line).is_err());
    }

    #[test]
    fn invalid_ip_is_error() {
        let line = b"PROXY TCP4 not-an-ip 198.51.100.1 56324 443\r\n";
        assert!(parse_proxy_v1_addr(line).is_err());
    }

    #[test]
    fn invalid_port_is_error() {
        let line = b"PROXY TCP4 192.0.2.1 198.51.100.1 abc 443\r\n";
        assert!(parse_proxy_v1_addr(line).is_err());
    }

    #[test]
    fn truncated_tcp4_field_count_is_error() {
        let line = b"PROXY TCP4 192.0.2.1 198.51.100.1 56324\r\n";
        assert!(parse_proxy_v1_addr(line).is_err());
    }
}
