//! Emit one signed `Message` as JSON on stdout — a fixture for piping into the
//! `agentbbs-bridge` binary:
//!
//! ```text
//! cargo run -q --example sample_message | \
//!   cargo run -q --bin agentbbs-bridge -- --config bridge.json --dry-run
//! ```
use agentbbs_core::{Identity, Message, MessageBody};
use chrono::Utc;

fn main() {
    let id = Identity::generate();
    let body = MessageBody {
        board: std::env::args().nth(1).unwrap_or_else(|| "general".into()),
        parent: None,
        subject: String::new(),
        body: "hello from a real signed AgentBBS message".into(),
        author: id.id(),
        handle: "alice".into(),
        created_at: Utc::now(),
    };
    let msg = Message::sign(&id, body).expect("sign");
    println!("{}", serde_json::to_string(&msg).expect("serialize"));
}
