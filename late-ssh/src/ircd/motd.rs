//! MOTD content. Regenerated per connection so dynamic content (lounge
//! banner) stays fresh.

/// Build MOTD body lines (without the 375/372/376 framing).
// TODO(FRD §7.4): include the live lounge banner content once the banner
// source is plumbed through (the same content the TUI lounge top boxes show).
pub fn motd_lines(web_url: &str) -> Vec<String> {
    vec![
        "late.sh — Command-Line Clubhouse for Computer People".to_string(),
        String::new(),
        "Your nick is derived from your late.sh username; dots appear as ^.".to_string(),
        "Manage your IRC token: ssh late.sh → Settings → Account.".to_string(),
        "Everyone is joined to #lounge; it cannot be left.".to_string(),
        String::new(),
        format!("Web: {web_url}"),
    ]
}
