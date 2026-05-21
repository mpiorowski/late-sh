//! Exit-banner content shown to the user when their session ends. A small
//! pool that's picked from at random; the original "Stay late. Code safe."
//! stays in the rotation as a familiar default.

use rand_core::{OsRng, RngCore};

/// Pool of farewell messages printed on disconnect. Order is not part of any
/// contract; `pick` always selects via modulo so reordering or extending the
/// list is safe.
pub const FAREWELLS: &[&str] = &[
    "Stay late. Code safe. ✨",
    "Sleep well. The bonsai is in good hands. 🌱",
    "Until next time. ☕",
    "Catch you on the next late shift.",
    "We'll keep the lights on. 🕯",
    "The cat says goodbye. 🐈",
    "Take it easy out there.",
    "Mind the witching hour.",
    "Don't forget to drink water.",
    "Goodnight, friend.",
];

/// Picks a farewell deterministically from `seed`. Pure — suitable for tests
/// and any caller that wants a stable choice.
pub fn pick(seed: u64) -> &'static str {
    let idx = (seed as usize) % FAREWELLS.len();
    FAREWELLS[idx]
}

/// Picks a farewell using OS entropy. Used on disconnect so each user gets a
/// fresh line. Mirrors the entropy source used by `splash_tips::choose_splash_hint`.
pub fn pick_random() -> &'static str {
    pick(OsRng.next_u64())
}

/// Convenience: the same payload as `pick_random` but wrapped in `\r\n` so the
/// SSH/web-tunnel exit handlers can hand it straight to `data` / `Message::Binary`.
pub fn render_exit_payload() -> String {
    format!("\r\n{}\r\n", pick_random())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn farewells_are_non_empty_and_unique() {
        assert!(!FAREWELLS.is_empty());
        for f in FAREWELLS {
            assert!(!f.is_empty(), "empty farewell string");
        }
        let mut sorted: Vec<&&str> = FAREWELLS.iter().collect();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), FAREWELLS.len(), "duplicate farewells");
    }

    #[test]
    fn pick_is_deterministic() {
        for seed in 0..256u64 {
            assert_eq!(pick(seed), pick(seed));
        }
    }

    #[test]
    fn pick_covers_all_indices() {
        for (i, expected) in FAREWELLS.iter().enumerate() {
            assert_eq!(pick(i as u64), *expected);
        }
    }

    #[test]
    fn pick_wraps_modulo() {
        let n = FAREWELLS.len() as u64;
        assert_eq!(pick(n), pick(0));
        assert_eq!(pick(n * 7 + 3), pick(3));
    }

    #[test]
    fn render_exit_payload_wraps_in_crlf() {
        let payload = render_exit_payload();
        assert!(payload.starts_with("\r\n"));
        assert!(payload.ends_with("\r\n"));
        // The body in between must match one of the known farewells exactly.
        let body = payload.trim_start_matches("\r\n").trim_end_matches("\r\n");
        assert!(
            FAREWELLS.contains(&body),
            "rendered body should be one of the known farewells, got: {body:?}"
        );
    }

    #[test]
    fn pick_random_returns_pool_member() {
        for _ in 0..32 {
            let chosen = pick_random();
            assert!(FAREWELLS.contains(&chosen));
        }
    }
}
