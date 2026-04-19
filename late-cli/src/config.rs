use anyhow::{Context, Result};
use shlex::Shlex;
use std::{env, path::PathBuf};
use tracing_subscriber::EnvFilter;

pub(super) const DEFAULT_SSH_TARGET: &str = "late.sh";
pub(super) const DEFAULT_AUDIO_BASE_URL: &str = "https://audio.late.sh";
pub(super) const DEFAULT_API_BASE_URL: &str = "https://api.late.sh";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SshMode {
    Subprocess,
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
    pub(super) api_base_url: String,
    pub(super) verbose: bool,
}

impl Config {
    pub(super) fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut ssh_target =
            env::var("LATE_SSH_TARGET").unwrap_or_else(|_| DEFAULT_SSH_TARGET.to_string());
        let mut ssh_port = env::var("LATE_SSH_PORT")
            .ok()
            .map(|value| value.parse())
            .transpose()
            .context("invalid LATE_SSH_PORT")?;
        let mut ssh_user = env::var("LATE_SSH_USER")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let mut key_file = env::var_os("LATE_KEY_FILE")
            .or_else(|| env::var_os("LATE_IDENTITY_FILE"))
            .map(PathBuf::from);
        let mut ssh_mode = env::var("LATE_SSH_MODE")
            .ok()
            .map(|value| SshMode::parse(&value))
            .transpose()?
            .unwrap_or(SshMode::Native);
        let mut ssh_bin =
            parse_ssh_bin_spec(&env::var("LATE_SSH_BIN").unwrap_or_else(|_| "ssh".to_string()))?;
        let mut audio_base_url =
            env::var("LATE_AUDIO_BASE_URL").unwrap_or_else(|_| DEFAULT_AUDIO_BASE_URL.to_string());
        let mut api_base_url =
            env::var("LATE_API_BASE_URL").unwrap_or_else(|_| DEFAULT_API_BASE_URL.to_string());
        let mut verbose = false;

        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--ssh-target" => ssh_target = next_value(&mut args, "--ssh-target")?,
                "--ssh-port" => {
                    ssh_port = Some(
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
                    ssh_user = Some(value);
                }
                "--key" | "--identity-file" => {
                    let value = next_value(&mut args, "--key")?;
                    if value.trim().is_empty() {
                        anyhow::bail!("--key cannot be blank");
                    }
                    key_file = Some(PathBuf::from(value));
                }
                "--ssh-mode" => {
                    ssh_mode = SshMode::parse(&next_value(&mut args, "--ssh-mode")?)?;
                }
                "--ssh-bin" => ssh_bin = parse_ssh_bin_spec(&next_value(&mut args, "--ssh-bin")?)?,
                "--audio-base-url" => audio_base_url = next_value(&mut args, "--audio-base-url")?,
                "--api-base-url" => api_base_url = next_value(&mut args, "--api-base-url")?,
                "--verbose" | "-v" => verbose = true,
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => anyhow::bail!("unknown argument '{other}'"),
            }
        }

        Ok(Self {
            ssh_target,
            ssh_port,
            ssh_user,
            key_file,
            ssh_mode,
            ssh_bin,
            audio_base_url,
            api_base_url,
            verbose,
        })
    }
}

pub(super) fn init_logging(verbose: bool) -> Result<()> {
    let env_filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) if verbose => EnvFilter::new("warn,symphonia=error,late=debug"),
        Err(_) => return Ok(()),
    };

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .try_init()
        .map_err(|err| anyhow::anyhow!("failed to initialize logging: {err}"))?;

    Ok(())
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
           --ssh-target <host>        SSH target (default: late.sh)\n\
           --ssh-port <port>          SSH port override\n\
           --ssh-user <user>          SSH username override\n\
           --key <path>               SSH identity file override\n\
           --ssh-mode <mode>          SSH transport: native (default) or old\n\
           --ssh-bin <command>        SSH client command, including optional args (default: ssh)\n\
           --audio-base-url <url>     Audio base URL, without or with /stream\n\
           --api-base-url <url>       API base URL used for /api/ws/pair\n\
           -v, --verbose              Enable debug logging to stderr\n\
         \n\
         Runtime hotkeys:\n\
           No local audio hotkeys; use the paired TUI client controls.\n"
    );
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
            "native" => Ok(Self::Native),
            other => anyhow::bail!("invalid ssh mode '{other}'; expected 'native' or 'old'"),
        }
    }

    pub(super) fn client_state_label(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Subprocess => "old",
        }
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
        assert_eq!(SshMode::parse("native").unwrap(), SshMode::Native);
    }

    #[test]
    fn config_defaults_to_native_ssh_mode() {
        let config = Config::from_args(Vec::<String>::new()).unwrap();
        assert_eq!(config.ssh_mode, SshMode::Native);
    }
}
