//! A minimal MCP client for AgentBBS *egress*.
//!
//! When an AgentBBS agent wants to call out to an external MCP server, it uses
//! an [`McpClient`] over a byte-stream pair. The caller is responsible for
//! checking [`Caps::MCP_EGRESS`] before constructing/using the client; the
//! client itself is transport-only.

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, Lines};
use tokio::sync::Mutex;

use serde_json::{json, Value};

use crate::jsonrpc::{Request, Response};
use crate::server::PROTOCOL_VERSION;

/// Errors from the MCP client.
#[derive(Debug)]
pub enum ClientError {
    /// Underlying transport I/O failed.
    Io(std::io::Error),
    /// A JSON (de)serialization error.
    Serde(serde_json::Error),
    /// The peer closed before responding.
    Closed,
    /// The peer returned a JSON-RPC error.
    Rpc(crate::jsonrpc::RpcError),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Io(e) => write!(f, "io: {e}"),
            ClientError::Serde(e) => write!(f, "serde: {e}"),
            ClientError::Closed => write!(f, "connection closed"),
            ClientError::Rpc(e) => write!(f, "rpc error {}: {}", e.code, e.message),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<std::io::Error> for ClientError {
    fn from(e: std::io::Error) -> Self {
        ClientError::Io(e)
    }
}
impl From<serde_json::Error> for ClientError {
    fn from(e: serde_json::Error) -> Self {
        ClientError::Serde(e)
    }
}

/// A JSON-RPC over newline-delimited streams MCP client.
pub struct McpClient<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    reader: Mutex<Lines<BufReader<R>>>,
    writer: Mutex<W>,
    next_id: std::sync::atomic::AtomicI64,
}

impl<R, W> McpClient<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    /// Connect over an existing reader/writer pair.
    pub fn new(reader: R, writer: W) -> Self {
        McpClient {
            reader: Mutex::new(BufReader::new(reader).lines()),
            writer: Mutex::new(writer),
            next_id: std::sync::atomic::AtomicI64::new(1),
        }
    }

    fn alloc_id(&self) -> i64 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Send a request and await the matching response result.
    async fn request(&self, method: &str, params: Value) -> Result<Value, ClientError> {
        let id = self.alloc_id();
        let req = Request::new(id, method, params);
        let mut line = serde_json::to_vec(&req)?;
        line.push(b'\n');
        {
            let mut w = self.writer.lock().await;
            w.write_all(&line).await?;
            w.flush().await?;
        }
        let mut r = self.reader.lock().await;
        let resp_line = r.next_line().await?.ok_or(ClientError::Closed)?;
        let resp: Response = serde_json::from_str(&resp_line)?;
        if let Some(err) = resp.error {
            return Err(ClientError::Rpc(err));
        }
        Ok(resp.result.unwrap_or(Value::Null))
    }

    /// Perform the `initialize` handshake, returning the server's reply.
    pub async fn initialize(&self) -> Result<Value, ClientError> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "clientInfo": { "name": "agentbbs-mcp-client", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": {}
            }),
        )
        .await
    }

    /// List the tools the server exposes.
    pub async fn list_tools(&self) -> Result<Value, ClientError> {
        self.request("tools/list", Value::Null).await
    }

    /// Call a tool by name with the given arguments.
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<Value, ClientError> {
        self.request("tools/call", json!({ "name": name, "arguments": args }))
            .await
    }

    /// List the resources the server exposes.
    pub async fn list_resources(&self) -> Result<Value, ClientError> {
        self.request("resources/list", Value::Null).await
    }

    /// Read a resource by uri.
    pub async fn read_resource(&self, uri: &str) -> Result<Value, ClientError> {
        self.request("resources/read", json!({ "uri": uri })).await
    }
}
