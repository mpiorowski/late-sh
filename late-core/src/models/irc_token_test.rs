use super::*;

#[test]
fn generated_tokens_have_expected_shape() {
    let token = generate_token().unwrap();
    assert!(token.starts_with(TOKEN_PREFIX));
    assert_eq!(token.len(), TOKEN_PREFIX.len() + TOKEN_RANDOM_LEN);
    assert!(
        token[TOKEN_PREFIX.len()..]
            .bytes()
            .all(|b| TOKEN_ALPHABET.contains(&b))
    );
}

#[test]
fn generated_tokens_are_unique() {
    assert_ne!(generate_token().unwrap(), generate_token().unwrap());
}

#[test]
fn hash_is_stable_hex_sha256() {
    let h = hash_token("late-irc-TEST");
    assert_eq!(h.len(), 64);
    assert_eq!(h, hash_token("late-irc-TEST"));
    assert_ne!(h, hash_token("late-irc-TEST2"));
}
