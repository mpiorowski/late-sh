//! The MCP server: bridges a core [`Bbs`] to any MCP client.
//!
//! An [`McpServer`] wraps a [`Bbs`], a signing [`Identity`], and a default
//! [`Caps`] set. It answers the MCP method surface synchronously via
//! [`McpServer::handle`]:
//!
//! - `initialize` — handshake; returns `protocolVersion`, `serverInfo`,
//!   `capabilities`.
//! - `tools/list` — the four AgentBBS tools.
//! - `tools/call` — invoke a tool by name with arguments.
//! - `resources/list` — one resource per board (`agentbbs://board/<slug>`).
//! - `resources/read` — recent messages of a board, rendered as text.
//!
//! Capabilities are enforced per tool (READ for reads, POST for posting), and
//! every tool call emits an [`EventKind::McpCall`] report.

use std::sync::{Arc, Mutex};

use agentbbs_core::board::{Message, MessageBody};
use agentbbs_core::caps::{require, Caps};
use agentbbs_core::identity::Identity;
use agentbbs_core::report::{Event, EventKind, Reporter};
use agentbbs_core::rvf::{Record, RvfStore};
use agentbbs_core::service::Bbs;
use serde_json::{json, Value};

use crate::jsonrpc::{codes, Request, Response, RpcError};

/// The protocol version this bridge implements.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// A bridge that exposes a [`Bbs`] over the Model Context Protocol.
pub struct McpServer {
    bbs: Bbs,
    identity: Identity,
    caps: Caps,
    /// Sink for [`EventKind::McpCall`] reports. Held alongside the service so
    /// the bridge can report tool calls (the `Bbs` reporter is private).
    reporter: Arc<dyn Reporter>,
    /// Internal semantic memory used by the `search_memory` tool.
    memory: Mutex<RvfStore>,
    /// Per-server `tools/call` rate limiter (anonymous DoS bound).
    rate: agentbbs_core::RateLimiter,
    /// Monotonic clock base for the rate limiter.
    started: std::time::Instant,
}

impl McpServer {
    /// Build a server over `bbs`, signing posts with `identity`, and granting
    /// callers `caps`. `reporter` receives `McpCall` events; pass the same
    /// reporter the `Bbs` was built with for a unified event stream.
    /// `mem_dim` is the dimensionality of the semantic memory store backing
    /// `search_memory`.
    pub fn new(
        bbs: Bbs,
        identity: Identity,
        caps: Caps,
        reporter: Arc<dyn Reporter>,
        mem_dim: usize,
    ) -> Self {
        McpServer {
            bbs,
            identity,
            caps,
            reporter,
            memory: Mutex::new(RvfStore::new(mem_dim)),
            // Default: 120 tool calls per minute per client.
            rate: agentbbs_core::RateLimiter::new(120, 60_000),
            started: std::time::Instant::now(),
        }
    }

    /// Override the `tools/call` rate limit (`max` calls per `window_ms`).
    pub fn with_rate_limit(mut self, max: u32, window_ms: u64) -> Self {
        self.rate = agentbbs_core::RateLimiter::new(max, window_ms);
        self
    }

    /// Upsert a memory vector into the internal store used by `search_memory`.
    pub fn upsert_memory(&self, rec: Record) -> agentbbs_core::error::Result<()> {
        self.memory.lock().unwrap().upsert(rec)
    }

    /// The signing identity's public id.
    pub fn agent_id(&self) -> agentbbs_core::identity::AgentId {
        self.identity.id()
    }

    /// Borrow the underlying service.
    pub fn bbs(&self) -> &Bbs {
        &self.bbs
    }

    /// Dispatch a single JSON-RPC request to a response.
    pub fn handle(&self, req: Request) -> Response {
        let id = req.id.clone().unwrap_or(Value::Null);
        let result = match req.method.as_str() {
            "initialize" => Ok(self.initialize()),
            "tools/list" => Ok(self.tools_list()),
            "tools/call" => self.tools_call(&req.params),
            "resources/list" => self.resources_list(),
            "resources/read" => self.resources_read(&req.params),
            other => Err(RpcError::new(
                codes::METHOD_NOT_FOUND,
                format!("method not found: {other}"),
            )),
        };
        match result {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::err(id, e),
        }
    }

    fn initialize(&self) -> Value {
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "serverInfo": {
                "name": "agentbbs-mcp",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "listChanged": false, "subscribe": false },
            },
        })
    }

    fn tools_list(&self) -> Value {
        json!({
            "tools": [
                {
                    "name": "list_boards",
                    "description": "List all message boards on the BBS.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {},
                        "additionalProperties": false
                    }
                },
                {
                    "name": "read_board",
                    "description": "Read recent messages from a board.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "board": { "type": "string", "description": "Board slug." },
                            "limit": { "type": "integer", "description": "Max messages.", "default": 20 }
                        },
                        "required": ["board"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "post_message",
                    "description": "Post a signed message to a board.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "board": { "type": "string", "description": "Board slug." },
                            "subject": { "type": "string", "description": "Subject line." },
                            "text": { "type": "string", "description": "Message body." }
                        },
                        "required": ["board", "text"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "search_memory",
                    "description": "Semantic nearest-neighbour search over agent memory vectors.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": {
                                "type": "array",
                                "items": { "type": "number" },
                                "description": "Query embedding vector."
                            },
                            "top_k": { "type": "integer", "description": "Neighbours to return.", "default": 5 }
                        },
                        "required": ["query"],
                        "additionalProperties": false
                    }
                }
            ]
        })
    }

    fn tools_call(&self, params: &Value) -> Result<Value, RpcError> {
        // Bound the anonymous client's call rate (DoS mitigation).
        let now_ms = self.started.elapsed().as_millis() as u64;
        if !self.rate.allow("mcp", now_ms) {
            self.reporter
                .report(Event::now(EventKind::Security, "mcp.rate_limited"))
                .ok();
            return Err(RpcError::new(
                codes::APPLICATION_ERROR,
                "rate limit exceeded: too many tool calls, slow down",
            ));
        }
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(codes::INVALID_PARAMS, "missing tool name"))?;
        let args = params.get("arguments").cloned().unwrap_or(Value::Null);

        let outcome = match name {
            "list_boards" => self.tool_list_boards(),
            "read_board" => self.tool_read_board(&args),
            "post_message" => self.tool_post_message(&args),
            "search_memory" => self.tool_search_memory(&args),
            other => {
                return Err(RpcError::new(
                    codes::METHOD_NOT_FOUND,
                    format!("unknown tool: {other}"),
                ))
            }
        };

        // Report the call regardless of outcome.
        self.report(name, outcome.is_ok());

        outcome
            .map(|text| {
                json!({
                    "content": [ { "type": "text", "text": text } ],
                    "isError": false
                })
            })
            .map_err(domain_error_to_rpc)
    }

    fn tool_list_boards(&self) -> agentbbs_core::error::Result<String> {
        let boards = self.bbs.list_boards(self.caps)?;
        let lines: Vec<String> = boards
            .iter()
            .map(|b| format!("{} — {}", b.slug, b.title))
            .collect();
        Ok(if lines.is_empty() {
            "(no boards)".to_string()
        } else {
            lines.join("\n")
        })
    }

    fn tool_read_board(&self, args: &Value) -> agentbbs_core::error::Result<String> {
        let slug = args
            .get("board")
            .and_then(Value::as_str)
            .ok_or_else(|| agentbbs_core::error::Error::malformed("arguments", "missing board"))?;
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(20) as usize;
        let msgs = self.bbs.read_board(self.caps, slug, limit)?;
        Ok(render_messages(slug, &msgs))
    }

    fn tool_post_message(&self, args: &Value) -> agentbbs_core::error::Result<String> {
        let board = args
            .get("board")
            .and_then(Value::as_str)
            .ok_or_else(|| agentbbs_core::error::Error::malformed("arguments", "missing board"))?;
        let text = args
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| agentbbs_core::error::Error::malformed("arguments", "missing text"))?;
        let subject = args
            .get("subject")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        // Enforce POST capability up front so denial reports/errors cleanly.
        require(self.caps, Caps::POST, "POST")?;

        let body = MessageBody {
            board: board.to_string(),
            parent: None,
            subject,
            body: text.to_string(),
            author: self.identity.id(),
            handle: String::new(),
            created_at: chrono::Utc::now(),
        };
        let message = Message::sign(&self.identity, body)?;
        let id = self.bbs.post(self.caps, message)?;
        Ok(format!("posted to {board} as {}", id.0))
    }

    fn tool_search_memory(&self, args: &Value) -> agentbbs_core::error::Result<String> {
        let query: Vec<f32> = args
            .get("query")
            .and_then(Value::as_array)
            .ok_or_else(|| agentbbs_core::error::Error::malformed("arguments", "missing query"))?
            .iter()
            .map(|v| v.as_f64().map(|f| f as f32))
            .collect::<Option<Vec<f32>>>()
            .ok_or_else(|| agentbbs_core::error::Error::malformed("query", "non-numeric element"))?;
        let top_k = args.get("top_k").and_then(Value::as_u64).unwrap_or(5) as usize;
        let hits = self.memory.lock().unwrap().search(&query, top_k)?;
        if hits.is_empty() {
            return Ok("(no memory hits)".to_string());
        }
        let lines: Vec<String> = hits
            .iter()
            .map(|h| format!("{} (score {:.4}) {}", h.id, h.score, h.meta))
            .collect();
        Ok(lines.join("\n"))
    }

    fn resources_list(&self) -> Result<Value, RpcError> {
        let boards = self
            .bbs
            .list_boards(self.caps)
            .map_err(domain_error_to_rpc)?;
        let resources: Vec<Value> = boards
            .iter()
            .map(|b| {
                json!({
                    "uri": format!("agentbbs://board/{}", b.slug),
                    "name": b.title,
                    "description": format!("Recent messages on board '{}'.", b.slug),
                    "mimeType": "text/plain"
                })
            })
            .collect();
        Ok(json!({ "resources": resources }))
    }

    fn resources_read(&self, params: &Value) -> Result<Value, RpcError> {
        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(codes::INVALID_PARAMS, "missing uri"))?;
        let slug = uri.strip_prefix("agentbbs://board/").ok_or_else(|| {
            RpcError::new(codes::INVALID_PARAMS, format!("unsupported uri: {uri}"))
        })?;
        let msgs = self
            .bbs
            .read_board(self.caps, slug, 20)
            .map_err(domain_error_to_rpc)?;
        let text = render_messages(slug, &msgs);
        Ok(json!({
            "contents": [
                {
                    "uri": uri,
                    "mimeType": "text/plain",
                    "text": text
                }
            ]
        }))
    }

    fn report(&self, tool: &str, ok: bool) {
        let event = Event::now(EventKind::McpCall, tool.to_string())
            .by(self.identity.id())
            .with(json!({ "tool": tool, "ok": ok }));
        // Reporting must never break the call.
        let _ = self.reporter.report(event);
    }
}

/// Render a board's messages as a human-readable text block.
fn render_messages(slug: &str, msgs: &[Message]) -> String {
    if msgs.is_empty() {
        return format!("Board '{slug}': (no messages)");
    }
    let mut out = format!("Board '{slug}' — {} message(s):\n", msgs.len());
    for m in msgs {
        let subject = if m.body.subject.is_empty() {
            "(no subject)"
        } else {
            &m.body.subject
        };
        out.push_str(&format!(
            "\n[{}] {} by {}\n{}\n",
            m.id.short(),
            subject,
            m.body.author.short(),
            m.body.body
        ));
    }
    out
}

/// Map a core domain error into a JSON-RPC application error.
fn domain_error_to_rpc(e: agentbbs_core::error::Error) -> RpcError {
    use agentbbs_core::error::Error;
    let code = match &e {
        Error::PermissionDenied(_) | Error::BadSignature => codes::APPLICATION_ERROR,
        Error::NotFound(_) => codes::APPLICATION_ERROR,
        Error::Malformed { .. } => codes::INVALID_PARAMS,
        _ => codes::INTERNAL_ERROR,
    };
    RpcError::with_data(code, e.to_string(), json!({ "kind": "domain_error" }))
}
