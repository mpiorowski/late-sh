use super::*;

#[test]
fn affirmative_prompt_accepts_expected_inputs() {
    assert!(is_affirmative("y"));
    assert!(is_affirmative("Y"));
    assert!(is_affirmative("yes"));
    assert!(!is_affirmative("n"));
    assert!(!is_affirmative(""));
}

#[test]
fn home_dir_prefers_home_then_windows_fallbacks() {
    assert_eq!(
        home_dir_from_env(
            Some("/tmp/home".into()),
            Some("C:\\Users\\mat".into()),
            Some("C:".into()),
            Some("\\Users\\mat".into()),
        )
        .unwrap(),
        PathBuf::from("/tmp/home")
    );
    assert_eq!(
        home_dir_from_env(None, Some("C:\\Users\\mat".into()), None, None).unwrap(),
        PathBuf::from("C:\\Users\\mat")
    );
    assert_eq!(
        home_dir_from_env(None, None, Some("C:".into()), Some("\\Users\\mat".into())).unwrap(),
        PathBuf::from("C:\\Users\\mat")
    );
}

#[test]
fn ssh_key_setup_hint_includes_generate_and_reconnect_commands() {
    let hint = ssh_key_setup_hint(Path::new("/home/alice/.ssh/id_late_sh_ed25519"));

    assert!(hint.contains("ssh-keygen -t ed25519"));
    assert!(hint.contains("-f /home/alice/.ssh/id_late_sh_ed25519"));
    assert!(hint.contains("late --key /home/alice/.ssh/id_late_sh_ed25519"));
}
