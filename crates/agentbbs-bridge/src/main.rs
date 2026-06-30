//! `agentbbs-bridge` — the runnable surface for ADR-0025 Phase 0 (gap G1).
//!
//! Reads newline-delimited signed `Message` JSON on stdin, plans the outbound
//! mirror for each (per-board allowlist + loop guard, see the lib), and either
//! prints the planned POSTs (`--dry-run`) or delivers them to the configured
//! Slack/Teams webhooks.
//!
//! ```text
//! agentbbs-bridge --config bridge.json [--dry-run]
//! # bridge.json: {"mappings":[{"board":"general","slack_webhook":"https://hooks.slack.com/..."}]}
//! cat messages.ndjson | agentbbs-bridge --config bridge.json --dry-run
//! ```

use agentbbs_bridge::{deliver, Bridge, BridgeConfig};
use agentbbs_core::Message;
use std::io::{self, BufRead};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config_path: Option<String> = None;
    let mut dry_run = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => config_path = args.next(),
            "--dry-run" => dry_run = true,
            "-h" | "--help" => {
                eprintln!(
                    "usage: agentbbs-bridge --config <file.json> [--dry-run]\n\
                     \n\
                     Reads newline-delimited signed Message JSON on stdin and mirrors\n\
                     mapped boards to their configured Slack/Teams webhooks (ADR-0025).\n\
                     --dry-run prints the planned POSTs instead of sending them."
                );
                return Ok(());
            }
            other => {
                eprintln!("agentbbs-bridge: unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }

    let config_path = config_path.ok_or("missing --config <file.json>")?;
    let config = BridgeConfig::from_json(&std::fs::read_to_string(&config_path)?)?;
    let bridge = Bridge::new(config);
    let client = reqwest::Client::new();

    let mut planned = 0u64;
    let mut delivered = 0u64;
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let msg: Message = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("agentbbs-bridge: skipping bad message JSON: {e}");
                continue;
            }
        };
        let posts = bridge.plan(&msg);
        if posts.is_empty() {
            continue;
        }
        planned += posts.len() as u64;
        if dry_run {
            for p in &posts {
                println!(
                    "{}",
                    serde_json::json!({"target": p.target.to_string(), "url": p.url, "payload": p.payload})
                );
            }
        } else {
            match deliver(&client, &posts).await {
                Ok(t) => delivered += t.len() as u64,
                Err(e) => eprintln!("agentbbs-bridge: deliver error: {e}"),
            }
        }
    }

    if dry_run {
        eprintln!("agentbbs-bridge: planned {planned} post(s) (dry-run)");
    } else {
        eprintln!("agentbbs-bridge: planned {planned} post(s), delivered {delivered}");
    }
    Ok(())
}
