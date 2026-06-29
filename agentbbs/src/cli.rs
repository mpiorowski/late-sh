//! Hand-rolled argument parsing and command dispatch for the `agentbbs`
//! umbrella binary.
//!
//! We deliberately avoid `clap`: the surface is tiny and a small parser keeps
//! the dependency tree (and binary) lighter.

/// The parsed command the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Run the retro TUI locally over crossterm. The default with no args.
    Tui,
    /// Run the MCP server over stdio (JSON-RPC).
    Mcp,
    /// Run the anonymous SSH front door.
    Ssh {
        /// TCP port to bind.
        port: u16,
        /// Optional path to a persistent OpenSSH host key. When absent, a key
        /// is loaded from (or created at) the default persisted location
        /// (`$XDG_DATA_HOME/agentbbs/ssh_host_ed25519`, else
        /// `~/.local/share/agentbbs/ssh_host_ed25519`) so the host key is stable
        /// across restarts.
        host_key: Option<String>,
    },
    /// Federation subcommands via the ruflo adapter.
    Federate(Federate),
    /// Print the version banner and exit.
    Version,
    /// Print usage and exit.
    Help,
}

/// Federation subcommand variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Federate {
    /// Report current peer-link state (via the ruflo adapter).
    Status,
    /// Join the peer at `addr` (via the ruflo adapter).
    Join(String),
    /// Run a live native federation node: a TCP server that exchanges signed
    /// envelopes and replicates boards over the durable store.
    Serve {
        /// TCP port to bind the federation server.
        port: u16,
        /// Optional initial trusted peer in `<hex-node-id>@<host:port>` form.
        peer: Option<String>,
    },
}

/// The default SSH port for the anonymous front door.
pub const DEFAULT_SSH_PORT: u16 = 2222;

/// Default TCP port for the native federation server.
pub const DEFAULT_FED_PORT: u16 = 7420;

/// Usage text shown for `--help` and on parse errors.
pub const USAGE: &str = "\
AgentBBS — the first BBS made for agents and humans.

USAGE:
    agentbbs [SUBCOMMAND]

SUBCOMMANDS:
    tui                       Run the retro TUI locally (default)
    mcp                       Run the MCP server over stdio (JSON-RPC)
    ssh [--port N]            Anonymous SSH front door serving the TUI
        [--host-key PATH]
    federate status           Report federation status (ruflo)
    federate join <addr>      Join a federation peer (ruflo)
    federate serve            Run a live native federation node (TCP)
        [--port N]
        [--peer <id>@<addr>]
    --version, -V             Print version
    --help, -h                Print this help
";

/// Parse an iterator of arguments (already excluding argv[0]) into a [`Command`].
///
/// Returns `Err(message)` on an unrecognized or malformed invocation; the
/// caller is expected to print the message plus [`USAGE`] and exit non-zero.
pub fn parse<I, S>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    let mut it = args.iter();

    let Some(first) = it.next() else {
        // No subcommand => default to the local TUI.
        return Ok(Command::Tui);
    };

    match first.as_str() {
        "--help" | "-h" | "help" => Ok(Command::Help),
        "--version" | "-V" | "version" => Ok(Command::Version),
        "tui" => Ok(Command::Tui),
        "mcp" => Ok(Command::Mcp),
        "ssh" => parse_ssh(it.as_slice()),
        "federate" => parse_federate(it.as_slice()),
        other => Err(format!("unknown subcommand: {other}")),
    }
}

fn parse_ssh(rest: &[String]) -> Result<Command, String> {
    let mut port = DEFAULT_SSH_PORT;
    let mut host_key: Option<String> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--port" | "-p" => {
                let v = rest
                    .get(i + 1)
                    .ok_or_else(|| "--port requires a value".to_string())?;
                port = v
                    .parse::<u16>()
                    .map_err(|_| format!("invalid port: {v}"))?;
                i += 2;
            }
            "--host-key" | "-k" => {
                let v = rest
                    .get(i + 1)
                    .ok_or_else(|| "--host-key requires a path".to_string())?;
                host_key = Some(v.clone());
                i += 2;
            }
            other => return Err(format!("unexpected argument to ssh: {other}")),
        }
    }
    Ok(Command::Ssh { port, host_key })
}

fn parse_federate(rest: &[String]) -> Result<Command, String> {
    let sub = rest
        .first()
        .ok_or_else(|| "federate requires a subcommand: status | join <addr>".to_string())?;
    match sub.as_str() {
        "status" => Ok(Command::Federate(Federate::Status)),
        "join" => {
            let addr = rest
                .get(1)
                .ok_or_else(|| "federate join requires <addr>".to_string())?;
            Ok(Command::Federate(Federate::Join(addr.clone())))
        }
        "serve" => {
            let mut port = DEFAULT_FED_PORT;
            let mut peer: Option<String> = None;
            let mut i = 1;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--port" | "-p" => {
                        let v = rest
                            .get(i + 1)
                            .ok_or_else(|| "--port requires a value".to_string())?;
                        port = v.parse::<u16>().map_err(|_| format!("invalid port: {v}"))?;
                        i += 2;
                    }
                    "--peer" => {
                        let v = rest
                            .get(i + 1)
                            .ok_or_else(|| "--peer requires <hex-id>@<addr>".to_string())?;
                        peer = Some(v.clone());
                        i += 2;
                    }
                    other => return Err(format!("unexpected argument to federate serve: {other}")),
                }
            }
            Ok(Command::Federate(Federate::Serve { port, peer }))
        }
        other => Err(format!("unknown federate subcommand: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_defaults_to_tui() {
        let empty: [&str; 0] = [];
        assert_eq!(parse(empty).unwrap(), Command::Tui);
    }

    #[test]
    fn parses_each_subcommand() {
        assert_eq!(parse(["tui"]).unwrap(), Command::Tui);
        assert_eq!(parse(["mcp"]).unwrap(), Command::Mcp);
        assert_eq!(parse(["--version"]).unwrap(), Command::Version);
        assert_eq!(parse(["-V"]).unwrap(), Command::Version);
        assert_eq!(parse(["--help"]).unwrap(), Command::Help);
        assert_eq!(parse(["-h"]).unwrap(), Command::Help);
    }

    #[test]
    fn ssh_defaults_and_flags() {
        assert_eq!(
            parse(["ssh"]).unwrap(),
            Command::Ssh {
                port: DEFAULT_SSH_PORT,
                host_key: None
            }
        );
        assert_eq!(
            parse(["ssh", "--port", "9000", "--host-key", "/tmp/k"]).unwrap(),
            Command::Ssh {
                port: 9000,
                host_key: Some("/tmp/k".to_string())
            }
        );
    }

    #[test]
    fn ssh_rejects_bad_port() {
        assert!(parse(["ssh", "--port", "not-a-number"]).is_err());
        assert!(parse(["ssh", "--port"]).is_err());
    }

    #[test]
    fn federate_subcommands() {
        assert_eq!(
            parse(["federate", "status"]).unwrap(),
            Command::Federate(Federate::Status)
        );
        assert_eq!(
            parse(["federate", "join", "peer.example:9000"]).unwrap(),
            Command::Federate(Federate::Join("peer.example:9000".to_string()))
        );
        assert!(parse(["federate"]).is_err());
        assert!(parse(["federate", "join"]).is_err());
        assert!(parse(["federate", "bogus"]).is_err());
    }

    #[test]
    fn federate_serve_parsing() {
        assert_eq!(
            parse(["federate", "serve"]).unwrap(),
            Command::Federate(Federate::Serve { port: DEFAULT_FED_PORT, peer: None })
        );
        assert_eq!(
            parse(["federate", "serve", "--port", "8000", "--peer", "ab12@host:9000"]).unwrap(),
            Command::Federate(Federate::Serve {
                port: 8000,
                peer: Some("ab12@host:9000".to_string())
            })
        );
        assert!(parse(["federate", "serve", "--port", "bad"]).is_err());
        assert!(parse(["federate", "serve", "--peer"]).is_err());
    }

    #[test]
    fn unknown_subcommand_errors() {
        assert!(parse(["frobnicate"]).is_err());
    }
}
