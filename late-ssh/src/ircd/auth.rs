//! Token authentication for IRC connections. See devdocs/FRD-IRCD.md §5.

use anyhow::Result;
use late_core::{
    db::Db,
    models::{irc_token::IrcToken, server_ban::ServerBan, user::User},
};
use std::net::IpAddr;

pub enum AuthOutcome {
    /// Token valid, user loaded, not server-banned.
    Ok {
        user: Box<User>,
        token_id: uuid::Uuid,
    },
    /// No matching non-revoked token.
    BadToken,
    /// Token valid but the account is server-banned.
    Banned,
}

pub async fn authenticate(db: &Db, token: &str, peer_ip: IpAddr) -> Result<AuthOutcome> {
    let client = db.get().await?;
    if ServerBan::find_active_for_ip_address(&client, &peer_ip.to_string())
        .await?
        .is_some()
    {
        return Ok(AuthOutcome::Banned);
    }
    let Some(row) = IrcToken::find_by_token(&client, token).await? else {
        return Ok(AuthOutcome::BadToken);
    };
    let Some(user) = User::get(&client, row.user_id).await? else {
        return Ok(AuthOutcome::BadToken);
    };
    if ServerBan::find_active_for_user_id(&client, user.id)
        .await?
        .is_some()
    {
        return Ok(AuthOutcome::Banned);
    }
    IrcToken::touch_last_used(&client, row.id).await?;
    Ok(AuthOutcome::Ok {
        user: Box::new(user),
        token_id: row.id,
    })
}
