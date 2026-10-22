use super::*;

#[test]
fn oembed_url_asks_for_the_canonical_watch_link_as_json() {
    assert_eq!(
        oembed_url("dQw4w9WgXcQ"),
        "https://www.youtube.com/oembed?url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3DdQw4w9WgXcQ&format=json"
    );
}

#[test]
fn oembed_title_reads_the_trimmed_title() {
    let body =
        br#"{"title":"  Never Gonna Give You Up  ","author_name":"Rick Astley","type":"video"}"#;
    assert_eq!(
        oembed_title(body).expect("title"),
        "Never Gonna Give You Up"
    );
}

#[test]
fn oembed_title_rejects_a_blank_title_and_a_non_json_body() {
    assert!(oembed_title(br#"{"title":"   "}"#).is_err());
    // A private video's oEmbed answer is a bare "Unauthorized".
    assert!(oembed_title(b"Unauthorized").is_err());
}
