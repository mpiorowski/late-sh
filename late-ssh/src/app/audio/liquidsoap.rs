use anyhow::{Context, Result};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

#[derive(Clone, Debug)]
pub struct LiquidsoapController {
    addr: String,
}

impl LiquidsoapController {
    pub fn new(addr: String) -> Self {
        Self { addr }
    }

    pub async fn send_command(&self, command: &str) -> Result<()> {
        send_command(&self.addr, command).await
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }
}

pub async fn send_command(addr: &str, command: &str) -> Result<()> {
    tracing::debug!(addr, command, "sending liquidsoap command");
    let connect = TcpStream::connect(addr);
    let mut stream = tokio::time::timeout(std::time::Duration::from_millis(1000), connect)
        .await
        .context("connection timeout")?
        .context("connection failed")?;

    let write = async {
        stream.write_all(command.as_bytes()).await?;
        stream.write_all(b"\n").await
    };
    tokio::time::timeout(std::time::Duration::from_millis(500), write)
        .await
        .context("write timeout")?
        .context("write failed")?;

    let mut buf = [0u8; 256];
    let read = stream.read(&mut buf);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(500), read)
        .await
        .context("read timeout")?
        .context("read failed")?;

    Ok(())
}
