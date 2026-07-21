use super::*;

fn test_config() -> Config {
    Config {
        ssh_target: "late.example".to_string(),
        ssh_port: Some(2222),
        ssh_user: Some("alice".to_string()),
        key_file: None,
        ssh_mode: SshMode::OpenSsh,
        ssh_bin: vec![
            "ssh".to_string(),
            "-F".to_string(),
            "/tmp/ssh_config".to_string(),
        ],
        audio_base_url: "https://audio.example".to_string(),
        audio_output_device: None,
        api_base_url: "https://api.example".to_string(),
        verbose: false,
    }
}

#[test]
fn parse_cli_banner_extracts_token_and_consumed_bytes() {
    let buf = b"LATE_SESSION_TOKEN=abc-123\r\n\x1b[?1049h";
    match parse_cli_banner(buf) {
        BannerState::Token { token, consumed } => {
            assert_eq!(token, "abc-123");
            assert_eq!(consumed, 28);
        }
        _ => panic!("expected token banner"),
    }
}

#[test]
fn parse_cli_banner_passthroughs_regular_output() {
    let buf = b"hello\r\nworld";
    match parse_cli_banner(buf) {
        BannerState::Passthrough { consumed } => assert_eq!(consumed, 7),
        _ => panic!("expected passthrough"),
    }
}

#[test]
fn openssh_master_command_uses_control_master_and_identity() {
    let config = test_config();
    let spec = openssh_master_command_spec(
        &config,
        Some(Path::new("/home/alice/.ssh/id_ed25519_sk")),
        Path::new("/tmp/late-ssh-test/ctl"),
    )
    .unwrap();

    assert_eq!(spec.program, OsString::from("ssh"));
    assert_eq!(
        spec.args_as_strings(),
        vec![
            "-F",
            "/tmp/ssh_config",
            "-M",
            "-S",
            "/tmp/late-ssh-test/ctl",
            "-f",
            "-N",
            "-i",
            "/home/alice/.ssh/id_ed25519_sk",
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-p",
            "2222",
            "-l",
            "alice",
            "late.example",
        ]
    );
}

#[test]
fn openssh_master_command_allows_config_or_agent_identity() {
    let config = test_config();
    let spec =
        openssh_master_command_spec(&config, None, Path::new("/tmp/late-ssh-test/ctl")).unwrap();

    assert!(!spec.args_as_strings().contains(&"-i".to_string()));
}

#[test]
fn openssh_token_command_uses_control_socket_and_exec_handshake() {
    let config = test_config();
    let spec = openssh_token_command_spec(&config, Path::new("/tmp/late-ssh-test/ctl")).unwrap();

    assert_eq!(
        spec.args_as_strings(),
        vec![
            "-F",
            "/tmp/ssh_config",
            "-S",
            "/tmp/late-ssh-test/ctl",
            "-o",
            "BatchMode=yes",
            "-p",
            "2222",
            "-l",
            "alice",
            "late.example",
            CLI_TOKEN_REQUEST,
        ]
    );
}

#[test]
fn openssh_shell_command_uses_control_socket_and_tty() {
    let config = test_config();
    let spec = openssh_shell_command_spec(&config, Path::new("/tmp/late-ssh-test/ctl")).unwrap();

    assert_eq!(
        spec.args_as_strings(),
        vec![
            "-F",
            "/tmp/ssh_config",
            "-S",
            "/tmp/late-ssh-test/ctl",
            "-o",
            "BatchMode=yes",
            "-tt",
            "-p",
            "2222",
            "-l",
            "alice",
            "late.example",
        ]
    );
}

#[test]
fn openssh_cleanup_command_exits_control_master() {
    let config = test_config();
    let spec = openssh_cleanup_command_spec(&config, Path::new("/tmp/late-ssh-test/ctl")).unwrap();

    assert_eq!(
        spec.args_as_strings(),
        vec![
            "-F",
            "/tmp/ssh_config",
            "-S",
            "/tmp/late-ssh-test/ctl",
            "-O",
            "exit",
            "-p",
            "2222",
            "-l",
            "alice",
            "late.example",
        ]
    );
}

#[test]
fn parse_session_token_response_accepts_valid_json() {
    let token = parse_session_token_response(br#"{"session_token":"token-123"}"#, "test handshake")
        .unwrap();
    assert_eq!(token, "token-123");
}

#[test]
fn parse_session_token_response_rejects_empty_token() {
    let err =
        parse_session_token_response(br#"{"session_token":"  "}"#, "test handshake").unwrap_err();
    assert!(err.to_string().contains("empty session token"));
}

#[test]
fn parse_target_supports_user_and_port() {
    let parsed = ParsedTarget::parse("alice@late.sh:2222").unwrap();
    assert_eq!(parsed.user.as_deref(), Some("alice"));
    assert_eq!(parsed.host, "late.sh");
    assert_eq!(parsed.port, Some(2222));
}

#[test]
fn parse_target_supports_bracketed_ipv6() {
    let parsed = ParsedTarget::parse("alice@[::1]:2222").unwrap();
    assert_eq!(parsed.user.as_deref(), Some("alice"));
    assert_eq!(parsed.host, "::1");
    assert_eq!(parsed.port, Some(2222));
}
