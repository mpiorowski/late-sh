use super::*;

#[test]
fn watch_url_accepts_an_absolute_http_url_and_trims_it() {
    assert_eq!(
        parse_watch_url(" https://late.sh/live/abc_-1?voices=off\n").unwrap(),
        "https://late.sh/live/abc_-1?voices=off"
    );
    assert_eq!(
        parse_watch_url("http://localhost:3000/live/abc").unwrap(),
        "http://localhost:3000/live/abc"
    );
}

#[test]
fn watch_url_refuses_other_schemes_and_whitespace() {
    assert!(parse_watch_url("file:///etc/passwd").is_err());
    assert!(parse_watch_url("javascript:alert(1)").is_err());
    assert!(parse_watch_url("late.sh/live/abc").is_err());
    assert!(parse_watch_url("").is_err());
    assert!(parse_watch_url("https://late.sh/live/a b").is_err());
}
