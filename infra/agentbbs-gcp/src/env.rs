//! Emulator-aware base-URL selection.
//!
//! In development the Firestore and Pub/Sub emulators advertise themselves via
//! the `FIRESTORE_EMULATOR_HOST` / `PUBSUB_EMULATOR_HOST` environment variables
//! (e.g. `localhost:8080`). When set, all REST traffic is plain `http://` to
//! that host. When unset, we fall back to the real Google API endpoints over
//! `https://`.
//!
//! The helpers accept an explicit override so reporters can be constructed with
//! a fixed base URL in tests without touching process-global env state.

/// Default production Firestore REST endpoint.
pub const FIRESTORE_PROD_BASE: &str = "https://firestore.googleapis.com";
/// Default production Pub/Sub REST endpoint.
pub const PUBSUB_PROD_BASE: &str = "https://pubsub.googleapis.com";

/// Environment variable the Firestore emulator sets.
pub const FIRESTORE_EMULATOR_ENV: &str = "FIRESTORE_EMULATOR_HOST";
/// Environment variable the Pub/Sub emulator sets.
pub const PUBSUB_EMULATOR_ENV: &str = "PUBSUB_EMULATOR_HOST";

/// Resolve a base URL given an optional explicit override, an emulator env
/// variable name, and the production fallback.
///
/// Precedence: explicit `override_base` → `http://{emulator_host}` → `prod`.
pub fn resolve_base(override_base: Option<&str>, emulator_env: &str, prod: &str) -> String {
    if let Some(base) = override_base {
        return trim_trailing_slash(base);
    }
    match std::env::var(emulator_env) {
        Ok(host) if !host.trim().is_empty() => {
            let host = host.trim();
            // The emulator host may or may not include a scheme.
            if host.starts_with("http://") || host.starts_with("https://") {
                trim_trailing_slash(host)
            } else {
                format!("http://{host}")
            }
        }
        _ => trim_trailing_slash(prod),
    }
}

/// Resolve the Firestore base URL (override → emulator → prod).
pub fn firestore_base(override_base: Option<&str>) -> String {
    resolve_base(override_base, FIRESTORE_EMULATOR_ENV, FIRESTORE_PROD_BASE)
}

/// Resolve the Pub/Sub base URL (override → emulator → prod).
pub fn pubsub_base(override_base: Option<&str>) -> String {
    resolve_base(override_base, PUBSUB_EMULATOR_ENV, PUBSUB_PROD_BASE)
}

fn trim_trailing_slash(s: &str) -> String {
    s.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // A unique env var name so this test never races other crates.
    const TEST_ENV: &str = "AGENTBBS_GCP_TEST_EMULATOR_HOST";

    #[test]
    fn explicit_override_wins() {
        let got = resolve_base(Some("http://example/"), TEST_ENV, FIRESTORE_PROD_BASE);
        assert_eq!(got, "http://example");
    }

    #[test]
    fn env_then_removed() {
        // Not set → production fallback.
        std::env::remove_var(TEST_ENV);
        assert_eq!(
            resolve_base(None, TEST_ENV, FIRESTORE_PROD_BASE),
            FIRESTORE_PROD_BASE
        );

        // Set → derives http://{host}.
        std::env::set_var(TEST_ENV, "localhost:8080");
        assert_eq!(
            resolve_base(None, TEST_ENV, FIRESTORE_PROD_BASE),
            "http://localhost:8080"
        );

        // Set with a scheme already → used verbatim (trailing slash trimmed).
        std::env::set_var(TEST_ENV, "http://127.0.0.1:9000/");
        assert_eq!(
            resolve_base(None, TEST_ENV, FIRESTORE_PROD_BASE),
            "http://127.0.0.1:9000"
        );

        // Removed again → back to production fallback.
        std::env::remove_var(TEST_ENV);
        assert_eq!(
            resolve_base(None, TEST_ENV, PUBSUB_PROD_BASE),
            PUBSUB_PROD_BASE
        );
    }

    #[test]
    fn blank_env_falls_back() {
        std::env::set_var(TEST_ENV, "   ");
        assert_eq!(
            resolve_base(None, TEST_ENV, FIRESTORE_PROD_BASE),
            FIRESTORE_PROD_BASE
        );
        std::env::remove_var(TEST_ENV);
    }
}
