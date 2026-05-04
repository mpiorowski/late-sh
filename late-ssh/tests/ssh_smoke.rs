mod helpers;

use getrandom::SysRng;
use helpers::{new_test_db, test_app_state, test_config};
use late_ssh::ssh::run_with_listener;
use russh::keys::signature::rand_core::UnwrapErr;
use russh::{
    ChannelMsg, client,
    keys::{PrivateKey, PrivateKeyWithHashAlg},
};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn emits_ssh_banner_when_client_connects_over_tcp() {
    let test_db = new_test_db().await;
    let config = test_config(test_db.db.config().clone());
    let state = test_app_state(test_db.db.clone(), config);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");

    let handle = tokio::spawn(async move {
        let _ = run_with_listener(listener, state, None).await;
    });

    let connect = timeout(Duration::from_secs(2), TcpStream::connect(addr)).await;
    assert!(connect.is_ok(), "tcp connect timed out");
    let mut stream = connect.unwrap().expect("tcp connect failed");

    let mut banner = [0u8; 64];
    let n = timeout(Duration::from_secs(2), stream.read(&mut banner))
        .await
        .expect("banner read timeout")
        .expect("banner read");
    assert!(n > 0, "expected ssh banner bytes");
    assert!(
        std::str::from_utf8(&banner[..n])
            .unwrap_or("")
            .starts_with("SSH-2.0-"),
        "expected SSH identification banner"
    );

    handle.abort();
}

struct TestClient;

impl client::Handler for TestClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

#[tokio::test]
async fn rejects_second_auth_when_ssh_attempt_rate_limit_is_one() {
    let test_db = new_test_db().await;
    let mut config = test_config(test_db.db.config().clone());
    config.max_conns_per_ip = 100;
    config.ssh_max_attempts_per_ip = 1;
    let state = test_app_state(test_db.db.clone(), config);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        let _ = run_with_listener(listener, state, None).await;
    });

    let user = "rate-limit-user";
    let key = Arc::new(
        PrivateKey::random(
            &mut UnwrapErr(SysRng),
            russh::keys::ssh_key::Algorithm::Ed25519,
        )
        .expect("generate client key"),
    );

    let mut c1 = client::connect(Arc::new(client::Config::default()), addr, TestClient)
        .await
        .expect("connect client 1");
    let auth1 = c1
        .authenticate_publickey(
            user,
            PrivateKeyWithHashAlg::new(
                key.clone(),
                c1.best_supported_rsa_hash()
                    .await
                    .expect("rsa hash")
                    .flatten(),
            ),
        )
        .await
        .expect("auth client 1")
        .success();
    assert!(auth1, "first auth should succeed");
    c1.disconnect(russh::Disconnect::ByApplication, "", "en")
        .await
        .expect("disconnect client 1");

    let mut c2 = client::connect(Arc::new(client::Config::default()), addr, TestClient)
        .await
        .expect("connect client 2");
    let auth2 = c2
        .authenticate_publickey(
            user,
            PrivateKeyWithHashAlg::new(
                key.clone(),
                c2.best_supported_rsa_hash()
                    .await
                    .expect("rsa hash")
                    .flatten(),
            ),
        )
        .await
        .expect("auth client 2")
        .success();
    assert!(!auth2, "second auth should be rejected by ssh rate limiter");

    handle.abort();
}

#[tokio::test]
async fn closing_token_exec_channel_does_not_close_interactive_shell() {
    let test_db = new_test_db().await;
    let config = test_config(test_db.db.config().clone());
    let state = test_app_state(test_db.db.clone(), config);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        let _ = run_with_listener(listener, state, None).await;
    });

    let user = "token-channel-user";
    let key = Arc::new(
        PrivateKey::random(
            &mut UnwrapErr(SysRng),
            russh::keys::ssh_key::Algorithm::Ed25519,
        )
        .expect("generate client key"),
    );
    let mut client = client::connect(Arc::new(client::Config::default()), addr, TestClient)
        .await
        .expect("connect client");
    let auth = client
        .authenticate_publickey(
            user,
            PrivateKeyWithHashAlg::new(
                key,
                client
                    .best_supported_rsa_hash()
                    .await
                    .expect("rsa hash")
                    .flatten(),
            ),
        )
        .await
        .expect("auth client")
        .success();
    assert!(auth, "auth should succeed");

    let mut token_channel = client
        .channel_open_session()
        .await
        .expect("open token channel");
    token_channel
        .exec(true, "late-cli-token-v1")
        .await
        .expect("exec token request");
    let mut token_payload = Vec::new();
    while token_payload.is_empty() {
        match timeout(Duration::from_secs(2), token_channel.wait())
            .await
            .expect("token response timeout")
            .expect("token channel closed before data")
        {
            ChannelMsg::Data { data } => token_payload.extend_from_slice(data.as_ref()),
            ChannelMsg::Close => panic!("token channel closed before data"),
            _ => {}
        }
    }
    assert!(
        std::str::from_utf8(&token_payload)
            .expect("token payload utf8")
            .contains("session_token"),
        "token exec should return session JSON"
    );

    let mut shell_channel = client
        .channel_open_session()
        .await
        .expect("open shell channel");
    shell_channel
        .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[])
        .await
        .expect("request pty");
    shell_channel
        .request_shell(true)
        .await
        .expect("request shell");
    expect_shell_data(&mut shell_channel).await;

    token_channel.close().await.expect("close token channel");
    expect_shell_data(&mut shell_channel).await;

    client
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await
        .expect("disconnect client");
    handle.abort();
}

async fn expect_shell_data(channel: &mut russh::Channel<client::Msg>) {
    loop {
        match timeout(Duration::from_secs(2), channel.wait()).await {
            Ok(Some(ChannelMsg::Data { .. })) => return,
            Ok(Some(ChannelMsg::Close)) => panic!("interactive shell closed unexpectedly"),
            Ok(Some(_)) => {}
            Ok(None) => panic!("interactive shell channel ended unexpectedly"),
            Err(_) => panic!("timed out waiting for interactive shell data"),
        }
    }
}
