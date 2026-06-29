use anyhow::{Context, Result};
use shlex::Shlex;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    env, fs,
    fs::OpenOptions,
    io::IsTerminal,
    path::{Path, PathBuf},
};
use tracing_subscriber::EnvFilter;

/// CLI version. Stamped from the release tag by build.rs (`LATE_CLI_VERSION`),
/// falling back to the Cargo.toml version for local builds. Matches the
/// `VERSION` file published to cli.late.sh byte-for-byte on real releases.
pub(super) const VERSION: &str = env!("LATE_CLI_VERSION");

pub(super) const DEFAULT_SSH_TARGET: &str = "late.sh";
// Legacy fallback only: current servers send authoritative stream URLs over
// set_playback_source. Points at the late-web /stream proxy (resolve_stream_url
// appends /stream) rather than raw Icecast, so the fallback survives mount
// reshuffles and gets the proxy's silence-injection resilience.
pub(super) const DEFAULT_AUDIO_BASE_URL: &str = "https://late.sh";
pub(super) const DEFAULT_API_BASE_URL: &str = "https://api.late.sh";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SshMode {
    Subprocess,
    OpenSsh,
    Native,
}

#[derive(Debug, Clone)]
pub(super) struct Config {
    pub(super) ssh_target: String,
    pub(super) ssh_port: Option<u16>,
    pub(super) ssh_user: Option<String>,
    pub(super) key_file: Option<PathBuf>,
    pub(super) ssh_mode: SshMode,
    pub(super) ssh_bin: Vec<String>,
    pub(super) audio_base_url: String,
    pub(super) audio_output_device: Option<String>,
    pub(super) api_base_url: String,
    pub(super) verbose: bool,
}

#[derive(Debug, Default, Clone)]
struct ConfigLayer {
    ssh_target: Option<String>,
    ssh_port: Option<u16>,
    ssh_user: Option<String>,
    key_file: Option<PathBuf>,
    ssh_mode: Option<SshMode>,
    ssh_bin: Option<Vec<String>>,
    audio_base_url: Option<String>,
    audio_output_device: Option<String>,
    api_base_url: Option<String>,
    verbose: Option<bool>,
}

impl Config {
    pub(super) fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let args = args.into_iter().collect::<Vec<_>>();
        let (config_path, arg_layer) = parse_arg_layer(args)?;
        let file_layer = load_config_layer(config_path.as_deref());
        let env_layer = env_config_layer()?;
        Ok(resolve_config(file_layer, env_layer, arg_layer))
    }
}

fn resolve_config(
    file_layer: ConfigLayer,
    env_layer: ConfigLayer,
    arg_layer: ConfigLayer,
) -> Config {
    let mut config = Config {
        ssh_target: DEFAULT_SSH_TARGET.to_string(),
        ssh_port: None,
        ssh_user: None,
        key_file: None,
        ssh_mode: SshMode::Native,
        ssh_bin: vec!["ssh".to_string()],
        audio_base_url: DEFAULT_AUDIO_BASE_URL.to_string(),
        audio_output_device: None,
        api_base_url: DEFAULT_API_BASE_URL.to_string(),
        verbose: false,
    };
    apply_layer(&mut config, file_layer);
    apply_layer(&mut config, env_layer);
    apply_layer(&mut config, arg_layer);
    config
}

fn apply_layer(config: &mut Config, layer: ConfigLayer) {
    if let Some(value) = layer.ssh_target {
        config.ssh_target = value;
    }
    if let Some(value) = layer.ssh_port {
        config.ssh_port = Some(value);
    }
    if let Some(value) = layer.ssh_user {
        config.ssh_user = Some(value);
    }
    if let Some(value) = layer.key_file {
        config.key_file = Some(value);
    }
    if let Some(value) = layer.ssh_mode {
        config.ssh_mode = value;
    }
    if let Some(value) = layer.ssh_bin {
        config.ssh_bin = value;
    }
    if let Some(value) = layer.audio_base_url {
        config.audio_base_url = value;
    }
    if let Some(value) = layer.audio_output_device {
        config.audio_output_device = Some(value);
    }
    if let Some(value) = layer.api_base_url {
        config.api_base_url = value;
    }
    if let Some(value) = layer.verbose {
        config.verbose = value;
    }
}

fn parse_arg_layer(
    args: impl IntoIterator<Item = String>,
) -> Result<(Option<PathBuf>, ConfigLayer)> {
    let mut layer = ConfigLayer::default();
    let mut config_path = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config_path = Some(PathBuf::from(next_value(&mut args, "--config")?)),
            "--ssh-target" => layer.ssh_target = Some(next_value(&mut args, "--ssh-target")?),
            "--ssh-port" => {
                layer.ssh_port = Some(
                    next_value(&mut args, "--ssh-port")?
                        .parse()
                        .context("invalid value for --ssh-port")?,
                )
            }
            "--ssh-user" => {
                let value = next_value(&mut args, "--ssh-user")?;
                if value.trim().is_empty() {
                    anyhow::bail!("--ssh-user cannot be blank");
                }
                layer.ssh_user = Some(value);
            }
            "--key" | "--identity-file" => {
                let value = next_value(&mut args, "--key")?;
                if value.trim().is_empty() {
                    anyhow::bail!("--key cannot be blank");
                }
                layer.key_file = Some(PathBuf::from(value));
            }
            "--ssh-mode" => {
                layer.ssh_mode = Some(SshMode::parse(&next_value(&mut args, "--ssh-mode")?)?);
            }
            "--ssh-bin" => {
                layer.ssh_bin = Some(parse_ssh_bin_spec(&next_value(&mut args, "--ssh-bin")?)?)
            }
            "--audio-base-url" => {
                layer.audio_base_url = Some(next_value(&mut args, "--audio-base-url")?)
            }
            "--audio-output-device" => {
                let value = next_value(&mut args, "--audio-output-device")?;
                if value.trim().is_empty() {
                    anyhow::bail!("--audio-output-device cannot be blank");
                }
                layer.audio_output_device = Some(value);
            }
            "--api-base-url" => layer.api_base_url = Some(next_value(&mut args, "--api-base-url")?),
            "--verbose" | "-v" => layer.verbose = Some(true),
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            "--version" | "-V" => {
                println!("late {VERSION}");
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument '{other}'"),
        }
    }

    Ok((config_path, layer))
}

fn env_config_layer() -> Result<ConfigLayer> {
    Ok(ConfigLayer {
        ssh_target: env::var("LATE_SSH_TARGET").ok(),
        ssh_port: env::var("LATE_SSH_PORT")
            .ok()
            .map(|value| value.parse())
            .transpose()
            .context("invalid LATE_SSH_PORT")?,
        ssh_user: env::var("LATE_SSH_USER")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        key_file: env::var_os("LATE_KEY_FILE")
            .or_else(|| env::var_os("LATE_IDENTITY_FILE"))
            .map(PathBuf::from),
        ssh_mode: env::var("LATE_SSH_MODE")
            .ok()
            .map(|value| SshMode::parse(&value))
            .transpose()?,
        ssh_bin: env::var("LATE_SSH_BIN")
            .ok()
            .map(|value| parse_ssh_bin_spec(&value))
            .transpose()?,
        audio_base_url: env::var("LATE_AUDIO_BASE_URL").ok(),
        audio_output_device: env::var("LATE_AUDIO_OUTPUT_DEVICE")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        api_base_url: env::var("LATE_API_BASE_URL").ok(),
        verbose: None,
    })
}

fn load_config_layer(explicit_path: Option<&Path>) -> ConfigLayer {
    let (path, explicit) = match explicit_path {
        Some(path) => (path.to_path_buf(), true),
        None => (default_config_path(), false),
    };
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound && !explicit => {
            return ConfigLayer::default();
        }
        Err(err) => {
            eprintln!(
                "late config warning: could not read {}: {err}",
                path.display()
            );
            return ConfigLayer::default();
        }
    };
    match parse_config_layer(&text) {
        Ok(layer) => layer,
        Err(err) => {
            eprintln!(
                "late config warning: could not parse {}: {err:#}",
                path.display()
            );
            ConfigLayer::default()
        }
    }
}

pub(super) fn init_logging(verbose: bool) -> Result<Option<PathBuf>> {
    let env_filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) if verbose => EnvFilter::new("warn,symphonia=error,late=debug"),
        Err(_) => return Ok(None),
    };

    if env_flag("LATE_LOG_STDERR") || !std::io::stderr().is_terminal() {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .try_init()
            .map_err(|err| anyhow::anyhow!("failed to initialize logging: {err}"))?;
        return Ok(None);
    }

    let path = cli_log_path();
    ensure_log_dir(&path)?;
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        options.mode(0o600).custom_flags(nix::libc::O_NOFOLLOW);
    }
    let file = options
        .open(&path)
        .with_context(|| format!("failed to open CLI log at {}", path.display()))?;
    #[cfg(unix)]
    {
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    let writer = move || {
        file.try_clone()
            .expect("failed to clone late CLI log file handle")
    };
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(writer)
        .try_init()
        .map_err(|err| anyhow::anyhow!("failed to initialize logging: {err}"))?;

    Ok(Some(path))
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .with_context(|| format!("missing value for {flag}"))
}

fn print_help() {
    println!(
        "late\n\
         \n\
         Minimal local launcher for late.sh.\n\
         \n\
         Options:\n\
           --config <path>           Config file override (default: ~/.config/late/config.toml)\n\
           --ssh-target <host>        SSH target (default: late.sh)\n\
           --ssh-port <port>          SSH port override\n\
           --ssh-user <user>          SSH username override\n\
           --key <path>               SSH identity file override\n\
           --ssh-mode <mode>          SSH transport: native (default), openssh, or old\n\
           --ssh-bin <command>        SSH client command, including optional args (default: ssh)\n\
           --audio-base-url <url>     Audio base URL, without or with /stream\n\
           --audio-output-device <n>  Audio output device name (default: system default)\n\
           --api-base-url <url>       API base URL used for /api/ws/pair\n\
           -v, --verbose              Enable debug logging (file-backed on interactive terminals)\n\
           -V, --version              Print version and exit\n\
         \n\
         Runtime hotkeys:\n\
           No local audio hotkeys; use the paired TUI client controls.\n"
    );
}

fn default_config_path() -> PathBuf {
    if let Some(base) = nonempty_os_env("XDG_CONFIG_HOME") {
        return PathBuf::from(base).join("late").join("config.toml");
    }
    if let Some(home) = nonempty_os_env("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("late")
            .join("config.toml");
    }
    env::temp_dir().join("late").join("config.toml")
}

fn parse_config_layer(text: &str) -> Result<ConfigLayer> {
    let mut layer = ConfigLayer::default();
    for (line_index, raw_line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        let line = strip_toml_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            anyhow::bail!("line {line_number}: sections are not supported");
        }
        let Some((key, raw_value)) = line.split_once('=') else {
            anyhow::bail!("line {line_number}: expected key = value");
        };
        let key = key.trim();
        let raw_value = raw_value.trim();
        match key {
            "ssh-target" => layer.ssh_target = Some(parse_toml_string(raw_value, line_number)?),
            "ssh-port" => {
                layer.ssh_port = Some(
                    raw_value
                        .parse()
                        .with_context(|| format!("line {line_number}: invalid ssh-port"))?,
                );
            }
            "ssh-user" => {
                let value = parse_toml_string(raw_value, line_number)?;
                if value.trim().is_empty() {
                    anyhow::bail!("line {line_number}: ssh-user cannot be blank");
                }
                layer.ssh_user = Some(value);
            }
            "ssh-mode" => {
                layer.ssh_mode = Some(SshMode::parse(&parse_toml_string(raw_value, line_number)?)?);
            }
            "key" => {
                layer.key_file = Some(PathBuf::from(parse_toml_string(raw_value, line_number)?))
            }
            "audio-base-url" => {
                layer.audio_base_url = Some(parse_toml_string(raw_value, line_number)?);
            }
            "api-base-url" => layer.api_base_url = Some(parse_toml_string(raw_value, line_number)?),
            "audio-output-device" => {
                let value = parse_toml_string(raw_value, line_number)?;
                if value.trim().is_empty() {
                    anyhow::bail!("line {line_number}: audio-output-device cannot be blank");
                }
                layer.audio_output_device = Some(value);
            }
            "verbose" => layer.verbose = Some(parse_toml_bool(raw_value, line_number)?),
            other => anyhow::bail!("line {line_number}: unsupported config key '{other}'"),
        }
    }
    Ok(layer)
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => {}
        }
    }
    line
}

fn parse_toml_string(raw: &str, line_number: usize) -> Result<String> {
    let raw = raw.trim();
    if raw.starts_with('"') {
        if !raw.ends_with('"') || raw.len() < 2 {
            anyhow::bail!("line {line_number}: unterminated string");
        }
        let inner = &raw[1..raw.len() - 1];
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                out.push(ch);
                continue;
            }
            let Some(escaped) = chars.next() else {
                anyhow::bail!("line {line_number}: invalid string escape");
            };
            match escaped {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                other => anyhow::bail!("line {line_number}: unsupported string escape '\\{other}'"),
            }
        }
        return Ok(out);
    }
    if raw.is_empty() {
        anyhow::bail!("line {line_number}: string value cannot be blank");
    }
    Ok(raw.to_string())
}

fn parse_toml_bool(raw: &str, line_number: usize) -> Result<bool> {
    match raw.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => anyhow::bail!("line {line_number}: expected true or false"),
    }
}

fn cli_log_path() -> PathBuf {
    if let Some(path) = nonempty_os_env("LATE_LOG_FILE") {
        return PathBuf::from(path);
    }

    #[cfg(unix)]
    {
        if let Some(base) = nonempty_os_env("XDG_STATE_HOME") {
            return PathBuf::from(base).join("late").join("late.log");
        }
        if let Some(home) = nonempty_os_env("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("late")
                .join("late.log");
        }
        if let Some(base) = nonempty_os_env("XDG_RUNTIME_DIR") {
            return PathBuf::from(base).join("late").join("late.log");
        }
        env::temp_dir()
            .join(format!("late-{}", effective_user_id()))
            .join("late.log")
    }

    #[cfg(windows)]
    {
        if let Some(base) = nonempty_os_env("LOCALAPPDATA") {
            return PathBuf::from(base).join("late").join("late.log");
        }
        if let Some(profile) = nonempty_os_env("USERPROFILE") {
            return PathBuf::from(profile)
                .join("AppData")
                .join("Local")
                .join("late")
                .join("late.log");
        }
        return env::temp_dir().join("late").join("late.log");
    }

    #[cfg(not(any(unix, windows)))]
    {
        env::temp_dir().join("late").join("late.log")
    }
}

fn ensure_log_dir(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .context("CLI log path has no parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create CLI log directory at {}", parent.display()))?;
    #[cfg(unix)]
    {
        let metadata = fs::symlink_metadata(parent).with_context(|| {
            format!(
                "failed to inspect CLI log directory at {}",
                parent.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            anyhow::bail!(
                "CLI log directory is not a real directory: {}",
                parent.display()
            );
        }
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

fn nonempty_os_env(key: &str) -> Option<std::ffi::OsString> {
    env::var_os(key).filter(|value| !value.is_empty())
}

fn env_flag(key: &str) -> bool {
    let Some(value) = env::var_os(key) else {
        return false;
    };
    let value = value.to_string_lossy();
    let normalized = value.trim().to_ascii_lowercase();
    !matches!(normalized.as_str(), "" | "0" | "false" | "no" | "off")
}

#[cfg(unix)]
fn effective_user_id() -> u32 {
    // SAFETY: geteuid has no preconditions and does not modify memory.
    unsafe { nix::libc::geteuid() }
}

fn parse_ssh_bin_spec(spec: &str) -> Result<Vec<String>> {
    let parts: Vec<String> = Shlex::new(spec).collect();
    if parts.is_empty() {
        anyhow::bail!("ssh client command cannot be empty");
    }
    Ok(parts)
}

impl SshMode {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "old" | "subprocess" => Ok(Self::Subprocess),
            "openssh" => Ok(Self::OpenSsh),
            "native" => Ok(Self::Native),
            other => {
                anyhow::bail!("invalid ssh mode '{other}'; expected 'native', 'openssh', or 'old'")
            }
        }
    }

    pub(super) fn client_state_label(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::OpenSsh => "openssh",
            Self::Subprocess => "old",
        }
    }

    pub(super) fn uses_cli_raw_mode(self) -> bool {
        !matches!(self, Self::OpenSsh)
    }
}

#[cfg(test)]
mod tests {
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
}
