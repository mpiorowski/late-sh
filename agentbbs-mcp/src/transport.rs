//! Newline-delimited JSON-RPC transport over async byte streams.
//!
//! MCP frames messages as newline-delimited JSON. [`serve_stdio`] reads
//! requests line by line, dispatches each through an [`McpServer`], and writes
//! the responses back. It is generic over any [`AsyncBufRead`] / [`AsyncWrite`]
//! pair, so it drives real stdio in production and a [`tokio::io::duplex`] pipe
//! in tests.

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::jsonrpc::{codes, Request, Response, RpcError};
use crate::server::McpServer;

/// Serve the MCP protocol over a reader/writer pair until EOF.
///
/// Reads newline-delimited JSON-RPC requests from `reader`, dispatches each via
/// [`McpServer::handle`], and writes newline-delimited responses to `writer`.
/// Notifications (requests without an id) are processed but produce no reply.
/// Malformed lines yield a parse-error response with a null id.
pub async fn serve_stdio<R, W>(
    server: Arc<McpServer>,
    reader: R,
    mut writer: W,
) -> std::io::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => {
                let is_notification = req.is_notification();
                let resp = server.handle(req);
                if is_notification {
                    // Notifications get no reply per JSON-RPC.
                    continue;
                }
                resp
            }
            Err(e) => Response::err(
                serde_json::Value::Null,
                RpcError::new(codes::PARSE_ERROR, format!("parse error: {e}")),
            ),
        };
        let mut bytes = serde_json::to_vec(&response)
            .unwrap_or_else(|_| b"{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32603,\"message\":\"serialize failed\"}}".to_vec());
        bytes.push(b'\n');
        writer.write_all(&bytes).await?;
        writer.flush().await?;
    }
    Ok(())
}
