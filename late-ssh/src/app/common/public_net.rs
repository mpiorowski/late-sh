//! Which addresses a server-side fetch of a user-supplied URL may reach.
//! Every such fetch (chat images, RSS feeds, the jobs link check, the News
//! reader's page) holds this one rule: a URL somebody typed never opens a
//! connection into the network the server sits in. How a fetch enforces it
//! stays the caller's: the Files downloader checks every answer up front and
//! pins the connection to them; the News reader resolves through
//! [`PublicOnlyResolver`].

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use reqwest::Url;

/// Whether `ip` is a public internet address. An IPv4 address carried in
/// IPv6 (`::ffff:a.b.c.d`) is judged by the address it carries, since a
/// dual-stack socket connects to exactly that.
pub(crate) fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
            Some(mapped) => is_public_ipv4(mapped),
            None => is_public_ipv6(ip),
        },
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        // "This network", 0.0.0.0/8, and reserved, 240.0.0.0/4.
        || a == 0
        || a >= 240
        // Carrier-grade NAT, 100.64.0.0/10.
        || (a == 100 && (64..=127).contains(&b))
        // Benchmarking, 198.18.0.0/15.
        || (a == 198 && (18..=19).contains(&b)))
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_unique_local()
        || ip.is_unicast_link_local())
}

/// An http(s) URL whose host, when it is a literal IP, is public. A host
/// name is checked where it resolves.
pub(crate) fn is_public_http_url(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    // An IPv6 literal comes bracketed (`[::1]`).
    match host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
    {
        Ok(ip) => is_public_ip(ip),
        Err(_) => true,
    }
}

/// System DNS with every non-public answer dropped, so a name that points
/// inside the network never gets a connection, however it is reached (the
/// shared URL or any redirect after it).
pub(crate) struct PublicOnlyResolver;

impl reqwest::dns::Resolve for PublicOnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|addr| is_public_ip(addr.ip()))
                .collect();
            if addrs.is_empty() {
                return Err(format!("{host} has no public address").into());
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}
