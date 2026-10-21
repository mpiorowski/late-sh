use std::net::IpAddr;

use super::is_blocked_ip;

/// A resolved answer carrying an IPv4 address inside IPv6 reaches that IPv4
/// address on a dual-stack socket, so it is judged by it.
#[test]
fn a_mapped_private_address_in_a_dns_answer_is_blocked() {
    for answer in [
        "::ffff:10.0.0.1",
        "::ffff:127.0.0.1",
        "::ffff:169.254.169.254",
    ] {
        let ip: IpAddr = answer.parse().expect("ip");
        assert!(is_blocked_ip(ip), "{answer} must be blocked");
    }
}
