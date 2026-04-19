use anyhow::{Context, Result};
use shlex::Shlex;
use std::env;
use tracing_subscriber::EnvFilter;

pub(super) const DEFAULT_SSH_TARGET: &str = "late.sh";
pub(super) const DEFAULT_AUDIO_BASE_URL: &str = "https://audio.late.sh";
pub(super) const DEFAULT_API_BASE_URL: &str = "https://api.late.sh";

#[derive(Debug, Clone)]
pub(super) struct Config {
    pub(super) ssh_target: String,
    pub(super) ssh_bin: Vec<String>,
    pub(super) audio_base_url: String,
    pub(super) api_base_url: String,
    pub(super) verbose: bool,
}

impl Config {
    pub(super) fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut ssh_target =
            env::var("LATE_SSH_TARGET").unwrap_or_else(|_| DEFAULT_SSH_TARGET.to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ssh_bin_spec_splits_command_and_args() {
        assert_eq!(
            parse_ssh_bin_spec("ssh -p 2222").unwrap(),
            vec!["ssh".to_string(), "-p".to_string(), "2222".to_string()]
        );
    }
}
