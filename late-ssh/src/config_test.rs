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
