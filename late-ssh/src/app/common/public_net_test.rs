use std::net::IpAddr;

use reqwest::Url;

use super::public_net::{is_public_http_url, is_public_ip};

/// No address inside a network, reserved, or carried in IPv6 counts as
/// public; ordinary internet addresses do.
#[test]
fn only_internet_addresses_are_public() {
    for private in [
        "127.0.0.1",
        "10.1.2.3",
        "172.16.0.9",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "198.18.0.1",
        "203.0.113.5",
        "0.0.0.0",
        "0.1.2.3",
        "240.0.0.1",
        "224.0.0.1",
        "::1",
        "::",
        "fd00::1",
        "fe80::1",
        "ff02::1",
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
    ] {
        let ip: IpAddr = private.parse().expect("ip");
        assert!(!is_public_ip(ip), "{private} must not be public");
    }
    for public in [
        "93.184.216.34",
        "1.1.1.1",
        "2606:4700::6810:85e5",
        "::ffff:93.184.216.34",
    ] {
        let ip: IpAddr = public.parse().expect("ip");
        assert!(is_public_ip(ip), "{public} is public");
    }
}

/// A URL is refused for its scheme or for a literal private host, bracketed
/// IPv6 included; a host name passes here and is checked where it resolves.
#[test]
fn a_url_needs_http_and_a_public_literal_host() {
    for refused in [
        "ftp://example.com/file",
        "file:///etc/passwd",
        "http://10.0.0.1/admin",
        "http://[::1]:8080/",
        "http://[::ffff:127.0.0.1]/",
    ] {
        let url = Url::parse(refused).expect("url");
        assert!(!is_public_http_url(&url), "{refused} must be refused");
    }
    for allowed in [
        "https://example.com/post",
        "http://93.184.216.34/",
        "http://localhost/",
    ] {
        let url = Url::parse(allowed).expect("url");
        assert!(is_public_http_url(&url), "{allowed} passes the url check");
    }
}
