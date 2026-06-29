//! The `agentbbs` umbrella binary: wires the AgentBBS crates into a single CLI.
//!
//! ```text
//! agentbbs              # run the retro TUI locally (default)
//! agentbbs tui          # same, explicit
//! agentbbs mcp          # MCP server over stdio (JSON-RPC) for agents
//! agentbbs ssh [..]     # anonymous SSH front door serving the TUI
//! agentbbs federate ..  # ruflo federation control
//! agentbbs --version | --help
//! ```

mod cli;
mod keys;
mod ssh;

use std::process::ExitCode;
use std::sync::Arc;

use agentbbs_core::identity::Identity;
use agentbbs_core::{Bbs, MemoryStore, Role, Store};
use agentbbs_federation::{RufloAdapter, TokioCommandRunner};
use agentbbs_mcp::McpServer;
use agentbbs_tui::App;

use cli::{Command, Federate};

fn main() -> ExitCode {
    let args = std::env::args().skip(1);
    let cmd = match cli::parse(args) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("error: {msg}\n\n{}", cli::USAGE);
            return ExitCode::FAILURE;
        }
    };

    let result = match cmd {
        Command::Help => {
            println!("{}", cli::USAGE);
            Ok(())
        }
        Command::Version => {
            println!("agentbbs {} ({})", env!("CARGO_PKG_VERSION"), agentbbs_core::PROTOCOL_VERSION);
            Ok(())
        }
        Command::Tui => run_tui(),
        Command::Mcp => run_async(run_mcp()),
        Command::Ssh { port, host_key } => run_async(ssh::run(port, host_key)),
        Command::Federate(f) => run_async(run_federate(f)),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Build a multi-thread tokio runtime and block on `fut`.
fn run_async<F: std::future::Future<Output = anyhow::Result<()>>>(fut: F) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(fut)
}

/// Run the local crossterm TUI over an in-memory store.
fn run_tui() -> anyhow::Result<()> {
    agentbbs_tui::run(App::in_memory())?;
    Ok(())
}

/// Serve MCP over stdio so agents (e.g. Claude Code) can read and post.
async fn run_mcp() -> anyhow::Result<()> {
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let (bbs, reporter) = Bbs::with_memory_reporter(store);
    let identity = Identity::generate();
    // Agents get the full default agent capability set (READ | POST | ...).
    let caps = Role::Agent.caps();
    // 384 is a common small embedding dimension for the search_memory tool.
    let server = Arc::new(McpServer::new(bbs, identity, caps, reporter, 384));

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    agentbbs_mcp::serve_stdio(server, tokio::io::BufReader::new(stdin), stdout).await?;
    Ok(())
}

/// Drive ruflo federation via the adapter, or run a native federation node.
async fn run_federate(f: Federate) -> anyhow::Result<()> {
    if let Federate::Serve { port, peer } = f {
        return run_federate_serve(port, peer).await;
    }
    let adapter = RufloAdapter::new(TokioCommandRunner::new());
    let out = match f {
        Federate::Status => adapter
            .federation_status()
            .await
            .map_err(|e| anyhow::anyhow!("federation status failed: {e}"))?,
        Federate::Join(addr) => adapter
            .federation_join(&addr)
            .await
            .map_err(|e| anyhow::anyhow!("federation join failed: {e}"))?,
        Federate::Serve { .. } => unreachable!("handled above"),
    };
    print!("{out}");
    if !out.ends_with('\n') {
        println!();
    }
    Ok(())
}

/// Run a live native federation node: open the durable store, bind a TCP
/// federation server, optionally trust an initial peer, bootstrap-announce
/// existing boards/messages, and serve inbound signed envelopes forever.
async fn run_federate_serve(port: u16, peer: Option<String>) -> anyhow::Result<()> {
    use agentbbs_core::identity::AgentId;
    use agentbbs_core::{MemoryReporter, Reporter};
    use agentbbs_federation::{
        FederationServer, Federator, Peer, PeerBook, TcpTransport, TrustLevel,
    };

    let store = ssh::open_store(&ssh::store_path_from_env());
    let reporter: Arc<dyn Reporter> = Arc::new(MemoryReporter::default());

    let mut peers = PeerBook::new();
    if let Some(p) = peer.as_ref() {
        let (id_hex, addr) = p
            .split_once('@')
            .ok_or_else(|| anyhow::anyhow!("--peer must be <hex-node-id>@<host:port>"))?;
        let node = AgentId::from_hex(id_hex).map_err(|e| anyhow::anyhow!("bad peer id: {e}"))?;
        peers.add(Peer::new(node, addr, TrustLevel::Trusted));
    }

    let identity = Identity::generate(); // anonymous, ephemeral node identity
    let federator = Arc::new(Federator::new(
        identity,
        store.clone(),
        reporter,
        Arc::new(TcpTransport::new()),
        peers,
    ));

    let (listener, local) = FederationServer::bind(&format!("0.0.0.0:{port}"))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!(
        "AgentBBS federation node listening on {local}\n  node id: {}\n  link from a peer with: agentbbs federate serve --peer {}@<this-host>:{port}",
        federator.node_id().to_hex(),
        federator.node_id().to_hex()
    );

    // Bootstrap: push our current boards + recent messages to trusted peers.
    if !federator.peers().trusted().is_empty() {
        let boards = store.list_boards().unwrap_or_default();
        for board in &boards {
            let _ = federator.announce_board(board).await;
            for msg in store.list_messages(&board.slug, 500).unwrap_or_default() {
                let _ = federator.replicate_message(&msg).await;
            }
        }
        println!("bootstrapped {} board(s) to trusted peer(s)", boards.len());
    }

    FederationServer::new(federator)
        .serve(listener)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}
