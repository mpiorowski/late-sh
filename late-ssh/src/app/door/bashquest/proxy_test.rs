use super::*;

/// Mirrors late-bashquest's `host::build_certificate_marker` byte layout
/// exactly; the two MUST stay in sync (see the cross-crate contract note
/// on `CERT_MARKER_TAG`).
fn build_marker(handle: &str, cert: &[u8]) -> Vec<u8> {
    let digest = blake3::hash(cert).to_hex();
    let mut out = Vec::new();
    out.extend_from_slice(CERT_MARKER_TAG);
    out.extend_from_slice(digest.as_bytes());
    out.push(0x01);
    out.extend_from_slice(handle.as_bytes());
    out.push(0x01);
    out.extend_from_slice(cert);
    out.push(0x00);
    out
}

#[test]
fn extracts_a_well_formed_marker() {
    let marker = build_marker("grad_one", b"CERTIFICATE TEXT");
    let (record, span) = extract_marker(&marker).expect("marker parses");
    assert_eq!(record.handle, "grad_one");
    assert_eq!(record.certificate, b"CERTIFICATE TEXT");
    assert_eq!(span, 0..marker.len());
}

#[test]
fn extracts_a_marker_surrounded_by_other_bytes_and_reports_correct_span() {
    let marker = build_marker("grad_two", b"cert");
    let mut data = b"leading ".to_vec();
    data.extend_from_slice(&marker);
    data.extend_from_slice(b" trailing");
    let (record, span) = extract_marker(&data).expect("marker parses");
    assert_eq!(record.handle, "grad_two");
    assert_eq!(&data[span.clone()], marker.as_slice());
    let mut rest = data[..span.start].to_vec();
    rest.extend_from_slice(&data[span.end..]);
    assert_eq!(rest, b"leading  trailing");
}

#[test]
fn rejects_a_marker_whose_certificate_does_not_match_its_digest() {
    let mut marker = build_marker("grad_three", b"real cert");
    // Flip the certificate payload's first byte without recomputing the
    // digest -- simulates a corrupted or forged marker.
    let cert_start = marker.len() - "real cert".len() - 1;
    marker[cert_start] = b'R';
    assert!(extract_marker(&marker).is_none());
}

#[test]
fn ignores_plain_output_with_no_marker() {
    assert!(extract_marker(b"just some normal bashquest.sh output\n").is_none());
}

#[test]
fn ignores_a_truncated_marker() {
    let marker = build_marker("grad_four", b"cert");
    assert!(extract_marker(&marker[..marker.len() - 5]).is_none());
}
