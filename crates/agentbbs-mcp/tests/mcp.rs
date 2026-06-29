//! Integration tests for the AgentBBS MCP bridge.

use std::sync::Arc;

use agentbbs_core::board::Board;
use agentbbs_core::caps::{Caps, Role};
use agentbbs_core::identity::Identity;
use agentbbs_core::rvf::Record;
use agentbbs_core::service::Bbs;
use agentbbs_core::store::MemoryStore;
use agentbbs_mcp::jsonrpc::Request;
use agentbbs_mcp::{McpClient, McpServer};

use serde_json::{json, Value};

/// Build a server with one board ("general") and an agent identity holding
/// `caps`. Returns the server plus the founder/agent identity used.
fn server_with_board(caps: Caps) -> Arc<McpServer> {
    let store = Arc::new(MemoryStore::new());
    let (bbs, reporter) = Bbs::with_memory_reporter(store);
    let sysop = Identity::generate();
    bbs.create_board(Role::Sysop.caps(), Board::new("general", "General", sysop.id()))
        .unwrap();
    let agent = Identity::generate();
    Arc::new(McpServer::new(bbs, agent, caps, reporter, 4))
}

fn req(id: i64, method: &str, params: Value) -> Request {
    Request::new(id, method, params)
}

#[test]
fn initialize_handshake() {
    let server = server_with_board(Caps::default());
    let resp = server.handle(req(1, "initialize", Value::Null));
    let result = resp.result.expect("result");
    assert_eq!(result["protocolVersion"], json!("2024-11-05"));
    assert_eq!(result["serverInfo"]["name"], json!("agentbbs-mcp"));
    assert!(result["capabilities"]["tools"].is_object());
}

#[test]
fn tools_list_returns_four_tools() {
    let server = server_with_board(Caps::default());
    let resp = server.handle(req(2, "tools/list", Value::Null));
    let tools = resp.result.unwrap()["tools"].clone();
    let names: Vec<String> = tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names.len(), 4);
    for want in ["list_boards", "read_board", "post_message", "search_memory"] {
        assert!(names.contains(&want.to_string()), "missing {want}");
    }
}

#[test]
fn post_then_read_reflects_message() {
    let server = server_with_board(Caps::default());

    let post = server.handle(req(
        3,
        "tools/call",
        json!({
            "name": "post_message",
            "arguments": { "board": "general", "subject": "hi", "text": "first agent post" }
        }),
    ));
    assert!(post.error.is_none(), "post errored: {:?}", post.error);

    let read = server.handle(req(
        4,
        "tools/call",
        json!({ "name": "read_board", "arguments": { "board": "general", "limit": 10 } }),
    ));
    let text = read.result.unwrap()["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(text.contains("first agent post"), "got: {text}");
    assert!(text.contains("hi"));
}

#[test]
fn resources_list_and_read() {
    let server = server_with_board(Caps::default());
    // Post one message so the read has content.
    server.handle(req(
        1,
        "tools/call",
        json!({ "name": "post_message", "arguments": { "board": "general", "text": "hello board" } }),
    ));

    let list = server.handle(req(2, "resources/list", Value::Null));
    let resources = list.result.unwrap()["resources"].clone();
    let uris: Vec<String> = resources
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap().to_string())
        .collect();
    assert!(uris.contains(&"agentbbs://board/general".to_string()));

    let read = server.handle(req(
        3,
        "resources/read",
        json!({ "uri": "agentbbs://board/general" }),
    ));
    let text = read.result.unwrap()["contents"][0]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(text.contains("hello board"), "got: {text}");
}

#[test]
fn denied_post_without_caps() {
    // Guest holds only READ — post_message must be denied.
    let server = server_with_board(Role::Guest.caps());
    let resp = server.handle(req(
        1,
        "tools/call",
        json!({ "name": "post_message", "arguments": { "board": "general", "text": "nope" } }),
    ));
    let err = resp.error.expect("expected an error");
    assert!(
        err.message.to_lowercase().contains("permission")
            || err.message.to_lowercase().contains("post"),
        "unexpected: {}",
        err.message
    );
}

#[test]
fn search_memory_tool() {
    let server = server_with_board(Caps::default());
    server
        .upsert_memory(Record {
            id: "mem-a".into(),
            vector: vec![1.0, 0.0, 0.0, 0.0],
            meta: json!({ "note": "first" }),
        })
        .unwrap();
    server
        .upsert_memory(Record {
            id: "mem-b".into(),
            vector: vec![0.0, 1.0, 0.0, 0.0],
            meta: json!({ "note": "second" }),
        })
        .unwrap();

    let resp = server.handle(req(
        1,
        "tools/call",
        json!({ "name": "search_memory", "arguments": { "query": [1.0, 0.0, 0.0, 0.0], "top_k": 1 } }),
    ));
    let text = resp.result.unwrap()["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(text.contains("mem-a"), "got: {text}");
}

#[tokio::test]
async fn client_server_roundtrip_over_duplex() {
    let server = server_with_board(Caps::default());

    // Wire the server's reads to one half of a duplex pipe and its writes to
    // the other, mirrored for the client.
    let (client_io, server_io) = tokio::io::duplex(8192);
    let (server_read, server_write) = tokio::io::split(server_io);
    let (client_read, client_write) = tokio::io::split(client_io);

    let srv = server.clone();
    let handle = tokio::spawn(async move {
        agentbbs_mcp::serve_stdio(srv, server_read, server_write)
            .await
            .unwrap();
    });

    let client = McpClient::new(client_read, client_write);

    let init = client.initialize().await.unwrap();
    assert_eq!(init["protocolVersion"], json!("2024-11-05"));

    // Post a message via a tool call, then read it back.
    let posted = client
        .call_tool(
            "post_message",
            json!({ "board": "general", "subject": "rt", "text": "roundtrip body" }),
        )
        .await
        .unwrap();
    assert_eq!(posted["isError"], json!(false));

    let read = client
        .call_tool("read_board", json!({ "board": "general" }))
        .await
        .unwrap();
    let text = read["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("roundtrip body"), "got: {text}");

    drop(client);
    // Server loop ends at EOF once the client side is dropped.
    let _ = handle.await;
}

#[test]
fn tools_call_is_rate_limited() {
    use agentbbs_core::store::MemoryStore;
    let store = Arc::new(MemoryStore::new());
    let (bbs, reporter) = Bbs::with_memory_reporter(store);
    let agent = Identity::generate();
    // Allow only 2 tool calls per minute.
    let server = McpServer::new(bbs, agent, Caps::default(), reporter, 4).with_rate_limit(2, 60_000);
    let call = || server.handle(req(1, "tools/call", json!({ "name": "list_boards", "arguments": {} })));
    assert!(call().result.is_some(), "1st call allowed");
    assert!(call().result.is_some(), "2nd call allowed");
    let third = call();
    assert!(third.result.is_none(), "3rd call blocked");
    assert!(third.error.unwrap().message.contains("rate limit"));
}
