use late_ssh::app::audio::liquidsoap::send_command;

#[tokio::test]
async fn send_command_returns_error_for_invalid_address() {
    let err = send_command("not-a-valid-address", "noop")
        .await
        .expect_err("expected failure");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("connection failed") || msg.contains("connection timeout"),
        "unexpected error: {msg}"
    );
}
