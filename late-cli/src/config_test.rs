use super::*;

#[test]
fn from_args_accepts_identity_file_override() {
    let config = Config::from_args(["--key".to_string(), "/tmp/late-key".to_string()]).unwrap();
    assert_eq!(config.key_file, Some(PathBuf::from("/tmp/late-key")));
}

#[test]
fn from_args_accepts_audio_output_device_override() {
    let config = Config::from_args([
        "--audio-output-device".to_string(),
        "Built-in Audio".to_string(),
    ])
    .unwrap();
    assert_eq!(
        config.audio_output_device,
        Some("Built-in Audio".to_string())
    );
}

#[test]
fn config_layers_resolve_file_then_env_then_args() {
    let file_layer = ConfigLayer {
        ssh_target: Some("file.example".to_string()),
        ssh_port: Some(2200),
        ssh_user: Some("file-user".to_string()),
        key_file: Some(PathBuf::from("/tmp/file-key")),
        ssh_mode: Some(SshMode::OpenSsh),
        audio_base_url: Some("https://audio.file".to_string()),
        audio_output_device: Some("File Device".to_string()),
        api_base_url: Some("https://api.file".to_string()),
        verbose: Some(true),
        ..ConfigLayer::default()
    };
    let env_layer = ConfigLayer {
        ssh_target: Some("env.example".to_string()),
        ssh_user: Some("env-user".to_string()),
        ssh_mode: Some(SshMode::Native),
        api_base_url: Some("https://api.env".to_string()),
        ..ConfigLayer::default()
    };
    let (_, arg_layer) = parse_arg_layer([
        "--ssh-target".to_string(),
        "arg.example".to_string(),
        "--key".to_string(),
        "/tmp/arg-key".to_string(),
        "--verbose".to_string(),
    ])
    .unwrap();

    let config = resolve_config(file_layer, env_layer, arg_layer);

    assert_eq!(config.ssh_target, "arg.example");
    assert_eq!(config.ssh_port, Some(2200));
    assert_eq!(config.ssh_user.as_deref(), Some("env-user"));
    assert_eq!(config.key_file, Some(PathBuf::from("/tmp/arg-key")));
    assert_eq!(config.ssh_mode, SshMode::Native);
    assert_eq!(config.audio_base_url, "https://audio.file");
    assert_eq!(config.audio_output_device.as_deref(), Some("File Device"));
    assert_eq!(config.api_base_url, "https://api.env");
    assert!(config.verbose);
}

#[test]
fn parse_config_layer_accepts_supported_flat_keys() {
    let layer = parse_config_layer(
        r#"
        # local defaults
        ssh-target = "late.example"
        ssh-port = 2222
        ssh-user = "alice"
        ssh-mode = "openssh"
        key = "/home/alice/.ssh/id_late"
        audio-base-url = "https://audio.example"
        api-base-url = "https://api.example"
        audio-output-device = "Built-in Audio"
        verbose = true
        "#,
    )
    .unwrap();

    assert_eq!(layer.ssh_target.as_deref(), Some("late.example"));
    assert_eq!(layer.ssh_port, Some(2222));
    assert_eq!(layer.ssh_user.as_deref(), Some("alice"));
    assert_eq!(layer.ssh_mode, Some(SshMode::OpenSsh));
    assert_eq!(
        layer.key_file,
        Some(PathBuf::from("/home/alice/.ssh/id_late"))
    );
    assert_eq!(
        layer.audio_base_url.as_deref(),
        Some("https://audio.example")
    );
    assert_eq!(layer.api_base_url.as_deref(), Some("https://api.example"));
    assert_eq!(layer.audio_output_device.as_deref(), Some("Built-in Audio"));
    assert_eq!(layer.verbose, Some(true));
}

#[test]
fn parse_arg_layer_extracts_config_path_without_affecting_merge() {
    let (path, layer) = parse_arg_layer([
        "--config".to_string(),
        "/tmp/laterc.toml".to_string(),
        "--ssh-mode".to_string(),
        "openssh".to_string(),
    ])
    .unwrap();

    assert_eq!(path, Some(PathBuf::from("/tmp/laterc.toml")));
    assert_eq!(layer.ssh_mode, Some(SshMode::OpenSsh));
}

#[test]
fn parse_ssh_bin_spec_splits_command_and_args() {
    assert_eq!(
        parse_ssh_bin_spec("ssh -p 2222").unwrap(),
        vec!["ssh".to_string(), "-p".to_string(), "2222".to_string()]
    );
}

#[test]
fn ssh_mode_parser_accepts_supported_values() {
    assert_eq!(SshMode::parse("old").unwrap(), SshMode::Subprocess);
    assert_eq!(SshMode::parse("subprocess").unwrap(), SshMode::Subprocess);
    assert_eq!(SshMode::parse("openssh").unwrap(), SshMode::OpenSsh);
    assert_eq!(SshMode::parse("native").unwrap(), SshMode::Native);
}

#[test]
fn config_defaults_to_native_ssh_mode() {
    let config = Config::from_args(Vec::<String>::new()).unwrap();
    assert_eq!(config.ssh_mode, SshMode::Native);
}
