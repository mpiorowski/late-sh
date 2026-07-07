use anyhow::Result;
use late_core::{
    models::{
        server_ban::{ServerBan, ServerBanActivation},
        user::User,
    },
    test_utils::{create_test_user, test_db},
};
use std::net::IpAddr;
use std::str::FromStr;

async fn has_active_server_ban(
    client: &tokio_postgres::Client,
    user: &User,
    fingerprint: &str,
    peer_ip: Option<IpAddr>,
) -> Result<bool> {
    if ServerBan::find_active_for_user_id(client, user.id)
        .await?
        .is_some()
    {
        return Ok(true);
    }
    if ServerBan::find_active_for_fingerprint(client, fingerprint)
        .await?
        .is_some()
    {
        return Ok(true);
    }
    let Some(peer_ip) = peer_ip else {
        return Ok(false);
    };
    Ok(
        ServerBan::find_active_for_ip_address(client, &peer_ip.to_string())
            .await?
            .is_some(),
    )
}

#[tokio::test]
async fn has_active_server_ban_matches_user_fingerprint_and_ip() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "ban_actor").await;
    let target = create_test_user(&test_db.db, "ban_target").await;
    let fingerprint_target = create_test_user(&test_db.db, "ban_fp_target").await;
    let ip_target = create_test_user(&test_db.db, "ban_ip_target").await;
    let banned_ip = IpAddr::from_str("203.0.113.10").expect("test ip");

    assert!(
        !has_active_server_ban(&client, &target, &target.fingerprint, None)
            .await
            .expect("ban lookup")
    );

    ServerBan::activate(
        &client,
        ServerBanActivation {
            target_user_id: target.id,
            fingerprint: Some(&target.fingerprint),
            ip_address: None,
            snapshot_username: Some(&target.username),
            actor_user_id: actor.id,
            reason: "test ban",
            expires_at: None,
        },
    )
    .await
    .expect("activate server ban");

    assert!(
        has_active_server_ban(&client, &target, &target.fingerprint, None)
            .await
            .expect("ban lookup")
    );

    assert!(
        !has_active_server_ban(
            &client,
            &fingerprint_target,
            &fingerprint_target.fingerprint,
            None
        )
        .await
        .expect("pre-fingerprint ban lookup")
    );
    ServerBan::activate(
        &client,
        ServerBanActivation {
            target_user_id: fingerprint_target.id,
            fingerprint: Some(&fingerprint_target.fingerprint),
            ip_address: None,
            snapshot_username: Some(&fingerprint_target.username),
            actor_user_id: actor.id,
            reason: "test fingerprint ban",
            expires_at: None,
        },
    )
    .await
    .expect("activate fingerprint ban");
    assert!(
        has_active_server_ban(
            &client,
            &fingerprint_target,
            &fingerprint_target.fingerprint,
            None
        )
        .await
        .expect("fingerprint ban lookup")
    );

    assert!(
        !has_active_server_ban(&client, &ip_target, &ip_target.fingerprint, Some(banned_ip))
            .await
            .expect("pre-ip ban lookup")
    );
    let banned_ip_text = banned_ip.to_string();
    ServerBan::activate(
        &client,
        ServerBanActivation {
            target_user_id: ip_target.id,
            fingerprint: Some(&ip_target.fingerprint),
            ip_address: Some(&banned_ip_text),
            snapshot_username: Some(&ip_target.username),
            actor_user_id: actor.id,
            reason: "test ip ban",
            expires_at: None,
        },
    )
    .await
    .expect("activate ip ban");
    assert!(
        has_active_server_ban(&client, &ip_target, &ip_target.fingerprint, Some(banned_ip))
            .await
            .expect("ip ban lookup")
    );
}
