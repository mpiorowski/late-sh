use std::{net::SocketAddr, time::Duration};

use anyhow::{Context, Result};
use tokio::{io::AsyncReadExt, net::TcpStream, time::timeout};

const PROXY_V1_MAX_LEN: usize = 108;
const PROXY_V1_PREFIX: &[u8] = b"PROXY ";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum V1Header {
    Absent,
    Present(Option<SocketAddr>),
}

pub(crate) fn is_trusted_peer(ip: std::net::IpAddr, trusted_cidrs: &[ipnet::IpNet]) -> bool {
    trusted_cidrs.iter().any(|cidr| cidr.contains(&ip))
}

pub(crate) async fn read_v1_client_addr(
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
                return parse_v1_addr(&line);
            }
        }
        anyhow::bail!(
            "proxy protocol v1 header exceeded {} bytes",
            PROXY_V1_MAX_LEN
        );
    };

    match timeout(timeout_duration, read_future).await {
        Ok(Ok(addr)) => Ok(addr),
        Ok(Err(error)) => Err(error.context("failed to read proxy protocol header")),
        Err(_) => anyhow::bail!("timed out waiting for proxy protocol header"),
    }
}

pub(crate) async fn read_optional_v1_header(
    stream: &mut TcpStream,
    timeout_duration: Duration,
) -> Result<V1Header> {
    let deadline = tokio::time::Instant::now() + timeout_duration;
    let mut prefix = [0u8; PROXY_V1_PREFIX.len()];

    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .context("timed out waiting for initial connection bytes")?;
        let bytes_read = timeout(remaining, stream.peek(&mut prefix))
            .await
            .context("timed out waiting for initial connection bytes")??;
        if bytes_read == 0 {
            anyhow::bail!("connection closed before proxy protocol header");
        }
        if !PROXY_V1_PREFIX.starts_with(&prefix[..bytes_read]) {
            return Ok(V1Header::Absent);
        }
        if bytes_read >= PROXY_V1_PREFIX.len() {
            if prefix == PROXY_V1_PREFIX {
                return read_v1_client_addr(stream, timeout_duration)
                    .await
                    .map(V1Header::Present);
            }
            return Ok(V1Header::Absent);
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

fn parse_v1_addr(line: &[u8]) -> Result<Option<SocketAddr>> {
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
            let src_ip = parts[2]
                .parse()
                .with_context(|| format!("invalid proxy v1 source IP '{}'", parts[2]))?;
            let src_port = parts[4]
                .parse()
                .with_context(|| format!("invalid proxy v1 source port '{}'", parts[4]))?;
            Ok(Some(SocketAddr::new(src_ip, src_port)))
        }
        family => anyhow::bail!("unsupported proxy v1 protocol family '{family}'"),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn parses_tcp4_source_addr() {
        let line = b"PROXY TCP4 203.0.113.10 10.42.0.76 54231 2222\r\n";
        let addr = parse_v1_addr(line).expect("parse").expect("source address");
        assert_eq!(
            addr,
            SocketAddr::from_str("203.0.113.10:54231").expect("socket address")
        );
    }

    #[test]
    fn parses_unknown_as_no_client_addr() {
        assert!(
            parse_v1_addr(b"PROXY UNKNOWN\r\n")
                .expect("parse")
                .is_none()
        );
    }

    #[test]
    fn rejects_malformed_header() {
        let line = b"PROXY TCP4 203.0.113.10 10.42.0.76 only-one-port\r\n";
        assert!(parse_v1_addr(line).is_err());
    }
}
