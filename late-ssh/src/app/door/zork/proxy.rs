use super::identity::derive_client_key;
use super::protocol::{Action, Catalogue, Edition};
use crate::render_signal::RenderSignal;
use anyhow::{Context, Result};
use russh::client::{self, Handler};
use russh::keys::PublicKey;
use russh::{ChannelMsg, Disconnect, Sig};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;

const SETUP_TIMEOUT: Duration = Duration::from_secs(15);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyStatus {
    Connecting,
    Running,
    Closed,
}
#[derive(Clone, Default)]
pub struct Outcome {
    pub code: Option<u32>,
    pub message: String,
}
struct AcceptHost;
impl Handler for AcceptHost {
    type Error = russh::Error;
    async fn check_server_key(&mut self, _: &PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }
}
#[derive(Clone)]
pub struct Connection {
    pub host: String,
    pub port: u16,
    pub secret: String,
    pub user_id: uuid::Uuid,
    pub repaint: Option<Arc<RenderSignal>>,
}
pub fn session_label(user_id: uuid::Uuid) -> String {
    let simple = user_id.simple().to_string();
    format!("late_{}", &simple[8..])
}
async fn connect(cfg: &Connection) -> Result<client::Handle<AcceptHost>> {
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(3600)),
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    });
    let mut session = client::connect(config, (cfg.host.as_str(), cfg.port), AcceptHost).await?;
    let key =
        russh::keys::PrivateKeyWithHashAlg::new(Arc::new(derive_client_key(&cfg.secret)), None);
    let auth = session
        .authenticate_publickey(&session_label(cfg.user_id), key)
        .await?;
    anyhow::ensure!(auth.success(), "Zork host rejected credentials");
    Ok(session)
}
/// Metadata travels on its own request, never through the terminal parser.
pub async fn catalogue(cfg: Connection) -> Result<Catalogue> {
    timeout(SETUP_TIMEOUT, async {
        let session = connect(&cfg).await?;
        let mut channel = session.channel_open_session().await?;
        channel.exec(true, "list").await?;
        let mut bytes = Vec::new();
        let mut code = None;
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => {
                    anyhow::ensure!(bytes.len() + data.len() <= 8192, "Zork catalogue too large");
                    bytes.extend_from_slice(&data);
                }
                ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        let _ = session
            .disconnect(Disconnect::ByApplication, "", "en")
            .await;
        anyhow::ensure!(code == Some(0), "Zork catalogue unavailable");
        let catalogue: Catalogue = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            catalogue.editions.len() == 3
                && Edition::ALL.iter().all(|edition| catalogue
                    .editions
                    .iter()
                    .filter(|s| s.edition == *edition)
                    .count()
                    == 1),
            "Invalid Zork catalogue"
        );
        Ok(catalogue)
    })
    .await
    .context("Zork catalogue timed out")?
}
enum Command {
    Input(Vec<u8>),
    Resize(u16, u16),
    Return,
}
pub struct Process {
    cmd_tx: mpsc::Sender<Command>,
    task: JoinHandle<()>,
    parser: Arc<Mutex<vt100::Parser>>,
    status: Arc<Mutex<ProxyStatus>>,
    outcome: Arc<Mutex<Outcome>>,
}
impl Drop for Process {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Process {
    pub fn spawn(
        cfg: Connection,
        edition: Edition,
        action: Action,
        cols: u16,
        rows: u16,
        term: String,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(256);
        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0)));
        let status = Arc::new(Mutex::new(ProxyStatus::Connecting));
        let outcome = Arc::new(Mutex::new(Outcome::default()));
        let task_parser = parser.clone();
        let task_status = status.clone();
        let task_outcome = outcome.clone();
        let task = tokio::spawn(async move {
            if let Err(error) = run(
                cfg.clone(),
                edition,
                action,
                (cols, rows),
                term,
                cmd_rx,
                (&task_parser, &task_status, &task_outcome),
            )
            .await
            {
                tracing::warn!(?error, "Zork proxy ended with error");
                task_outcome.lock().expect("outcome mutex").message =
                    "Unable to connect to Zork. Please try again.".into();
            }
            *task_status.lock().expect("status mutex") = ProxyStatus::Closed;
            if let Some(repaint) = cfg.repaint {
                repaint.wake();
            }
        });
        Self {
            cmd_tx,
            task,
            parser,
            status,
            outcome,
        }
    }
    pub fn status(&self) -> ProxyStatus {
        *self.status.lock().expect("status mutex")
    }
    pub fn outcome(&self) -> Outcome {
        self.outcome.lock().expect("outcome mutex").clone()
    }
    pub fn return_to_menu(&self) {
        let _ = self.cmd_tx.try_send(Command::Return);
    }
    pub fn send_input(&self, data: Vec<u8>) {
        let _ = self.cmd_tx.try_send(Command::Input(data));
    }
    pub fn with_screen<R>(&self, f: impl FnOnce(&vt100::Screen) -> R) -> R {
        f(self.parser.lock().expect("parser mutex").screen())
    }
    pub fn resize(&self, cols: u16, rows: u16) {
        self.parser
            .lock()
            .expect("parser mutex")
            .screen_mut()
            .set_size(rows, cols);
        let _ = self.cmd_tx.try_send(Command::Resize(cols, rows));
    }
}
async fn run(
    cfg: Connection,
    edition: Edition,
    action: Action,
    (cols, rows): (u16, u16),
    term: String,
    mut commands: mpsc::Receiver<Command>,
    (parser, status, outcome): (&Mutex<vt100::Parser>, &Mutex<ProxyStatus>, &Mutex<Outcome>),
) -> Result<()> {
    let (session, mut channel) = timeout(SETUP_TIMEOUT, async {
        let session = connect(&cfg).await?;
        let channel = session.channel_open_session().await?;
        channel
            .request_pty(true, &term, cols.into(), rows.into(), 0, 0, &[])
            .await?;
        channel
            .exec(true, format!("play {} {}", edition.key(), action.mode()))
            .await?;
        Ok::<_, anyhow::Error>((session, channel))
    })
    .await
    .context("Zork launch timed out")??;
    *status.lock().expect("status mutex") = ProxyStatus::Running;
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Input(bytes)) => channel.data(&bytes[..]).await?,
                Some(Command::Resize(cols, rows)) => channel.window_change(cols.into(), rows.into(), 0, 0).await?,
                Some(Command::Return) => channel.signal(Sig::USR1).await?,
                None => break,
            },
            msg = channel.wait() => match msg {
                Some(ChannelMsg::Data { data }) => {
                    parser.lock().expect("parser mutex").process(&data);
                    if let Some(repaint) = &cfg.repaint { repaint.wake(); }
                }
                Some(ChannelMsg::ExtendedData { data, .. }) => {
                    let mut outcome = outcome.lock().expect("outcome mutex");
                    if outcome.message.len() + data.len() <= 2048 {
                        outcome.message.extend(String::from_utf8_lossy(&data).chars().filter(|ch| !ch.is_control()));
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => outcome.lock().expect("outcome mutex").code = Some(exit_status),
                Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
    }
    let _ = channel.close().await;
    let _ = session
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    Ok(())
}
