use crate::test_helpers::test_config;

fn valid_config() -> crate::config::Config {
    test_config(late_core::db::DbConfig::default())
}

#[test]
fn baseline_test_config_passes_validation() {
    valid_config().validate().expect("valid config");
}

#[test]
fn ssh_proxy_protocol_without_trusted_cidrs_is_rejected() {
    let mut config = valid_config();
    config.ssh_proxy_protocol = true;
    config.ssh_proxy_trusted_cidrs = Vec::new();
    let error = config.validate().expect_err("must reject");
    assert!(error.to_string().contains("ssh proxy protocol"));
}

#[test]
fn irc_proxy_protocol_without_trusted_cidrs_is_rejected() {
    let mut config = valid_config();
    config.irc.enabled = true;
    config.irc.proxy_protocol = true;
    config.irc.proxy_trusted_cidrs = Vec::new();
    let error = config.validate().expect_err("must reject");
    assert!(error.to_string().contains("irc proxy protocol"));
}

#[test]
fn irc_tls_cert_without_key_is_rejected() {
    let mut config = valid_config();
    config.irc.enabled = true;
    config.irc.tls_cert_path = Some(std::path::PathBuf::from("/tmp/tls.crt"));
    config.irc.tls_key_path = None;
    let error = config.validate().expect_err("must reject");
    assert!(error.to_string().contains("cert is set without a key"));
}

#[test]
fn irc_tls_key_without_cert_is_rejected() {
    let mut config = valid_config();
    config.irc.enabled = true;
    config.irc.tls_cert_path = None;
    config.irc.tls_key_path = Some(std::path::PathBuf::from("/tmp/tls.key"));
    let error = config.validate().expect_err("must reject");
    assert!(error.to_string().contains("key is set without a cert"));
}

#[test]
fn ai_enabled_without_api_key_is_rejected() {
    let mut config = valid_config();
    config.ai.enabled = true;
    config.ai.api_key = None;
    let error = config.validate().expect_err("must reject");
    assert!(error.to_string().contains("ai is enabled"));
}

#[test]
fn enabled_door_game_without_secret_is_rejected() {
    let mut config = valid_config();
    config.nethack_enabled = true;
    config.nethack_secret = String::new();
    let error = config.validate().expect_err("must reject");
    assert!(error.to_string().contains("nethack"));
}

#[test]
fn zork_secret_is_required_only_when_entry_is_enabled() {
    let mut config = valid_config();
    config.zork_secret.clear();
    config.zork_enabled = false;
    config
        .validate()
        .expect("a disabled door may omit its secret");
    config.zork_enabled = true;
    assert!(config.validate().unwrap_err().to_string().contains("zork"));
}

#[test]
fn dev_files_with_both_credentials_uses_the_prod_bucket() {
    let files = crate::config::dev_files(Some("key".to_string()), Some("secret".to_string()))
        .expect("must accept")
        .expect("must be configured");
    assert_eq!(files.bucket, "late-sh-r-files");
    assert_eq!(files.public_base_url, "https://files.late.sh");
    assert_eq!(files.access_key_id, "key");
    assert_eq!(files.secret_access_key, "secret");
}

#[test]
fn dev_files_without_credentials_disables_uploads() {
    let files = crate::config::dev_files(None, None).expect("must accept");
    assert!(files.is_none());
}

#[test]
fn dev_files_with_only_access_key_is_rejected() {
    let error = crate::config::dev_files(Some("key".to_string()), None).expect_err("must reject");
    assert!(
        error
            .to_string()
            .contains("LATE_FILES_S3_ACCESS_KEY_ID is set without")
    );
}

#[test]
fn dev_files_with_only_secret_key_is_rejected() {
    let error =
        crate::config::dev_files(None, Some("secret".to_string())).expect_err("must reject");
    assert!(
        error
            .to_string()
            .contains("LATE_FILES_S3_SECRET_ACCESS_KEY is set without")
    );
}

#[test]
fn dev_ai_with_key_is_enabled() {
    let ai = crate::config::dev_ai(Some("key".to_string()));
    assert!(ai.enabled);
    assert_eq!(ai.api_key.as_deref(), Some("key"));
}

#[test]
fn dev_ai_without_key_is_disabled() {
    let ai = crate::config::dev_ai(None);
    assert!(!ai.enabled);
    assert!(ai.api_key.is_none());
}

#[test]
fn herald_networks_need_their_credentials_and_urls() {
    let mut config = valid_config();
    config.herald.mastodon = Some(crate::config::MastodonConfig {
        instance: "https://fosstodon.org".to_string(),
        token: String::new(),
        visibility: crate::config::HeraldVisibility::Public,
    });
    let error = config.validate().expect_err("must reject");
    assert!(
        error
            .to_string()
            .contains("mastodon is configured without a token")
    );

    config.herald.mastodon = Some(crate::config::MastodonConfig {
        instance: "fosstodon.org".to_string(),
        token: "token".to_string(),
        visibility: crate::config::HeraldVisibility::Public,
    });
    let error = config.validate().expect_err("must reject");
    assert!(error.to_string().contains("without an instance url"));

    config.herald.mastodon = Some(crate::config::MastodonConfig {
        instance: "https://fosstodon.org".to_string(),
        token: "token".to_string(),
        visibility: crate::config::HeraldVisibility::Public,
    });
    config.herald.bluesky = Some(crate::config::BlueskyConfig {
        pds: crate::config::BLUESKY_PDS.to_string(),
        identifier: "late.sh".to_string(),
        app_password: String::new(),
    });
    let error = config.validate().expect_err("must reject");
    assert!(error.to_string().contains("bluesky is configured without"));

    config.herald.bluesky = Some(crate::config::BlueskyConfig {
        pds: "bsky.social".to_string(),
        identifier: "late.sh".to_string(),
        app_password: "pass".to_string(),
    });
    let error = config.validate().expect_err("must reject");
    assert!(error.to_string().contains("without a pds url"));

    config.herald.bluesky = Some(crate::config::BlueskyConfig {
        pds: crate::config::BLUESKY_PDS.to_string(),
        identifier: "late.sh".to_string(),
        app_password: "pass".to_string(),
    });
    config.validate().expect("both networks pass");
    assert!(config.herald.enabled());
}

#[test]
fn dev_herald_switches_each_network_on_by_its_pair() {
    let both = crate::config::dev_herald(
        (
            Some("https://instance.test/".to_string()),
            Some("token".to_string()),
        ),
        (Some("late.test".to_string()), Some("pass".to_string())),
        3000,
    )
    .expect("must accept");
    assert!(both.enabled());
    let mastodon = both.mastodon.expect("mastodon");
    assert_eq!(mastodon.instance, "https://instance.test");
    assert_eq!(mastodon.token, "token");
    assert_eq!(
        mastodon.visibility,
        crate::config::HeraldVisibility::Unlisted
    );
    let bluesky = both.bluesky.expect("bluesky");
    assert_eq!(bluesky.pds, crate::config::BLUESKY_PDS);
    assert_eq!(bluesky.identifier, "late.test");
    assert_eq!(bluesky.app_password, "pass");
    assert_eq!(both.web_internal_url, "http://service-web:3000");

    let one = crate::config::dev_herald(
        (None, None),
        (Some("late.test".to_string()), Some("pass".to_string())),
        3000,
    )
    .expect("must accept");
    assert!(one.mastodon.is_none());
    assert!(one.bluesky.is_some());
    assert!(one.enabled());

    let none = crate::config::dev_herald((None, None), (None, None), 3000).expect("must accept");
    assert!(!none.enabled());
}

#[test]
fn dev_herald_half_set_pairs_are_rejected() {
    let error = crate::config::dev_herald(
        (Some("https://instance.test".to_string()), None),
        (None, None),
        3000,
    )
    .expect_err("must reject");
    assert!(
        error
            .to_string()
            .contains("LATE_HERALD_MASTODON_INSTANCE is set without")
    );
    let error = crate::config::dev_herald((None, Some("token".to_string())), (None, None), 3000)
        .expect_err("must reject");
    assert!(
        error
            .to_string()
            .contains("LATE_HERALD_MASTODON_TOKEN is set without")
    );
    let error =
        crate::config::dev_herald((None, None), (Some("late.test".to_string()), None), 3000)
            .expect_err("must reject");
    assert!(
        error
            .to_string()
            .contains("LATE_HERALD_BLUESKY_IDENTIFIER is set without")
    );
    let error = crate::config::dev_herald((None, None), (None, Some("pass".to_string())), 3000)
        .expect_err("must reject");
    assert!(
        error
            .to_string()
            .contains("LATE_HERALD_BLUESKY_PASSWORD is set without")
    );
}
