use anyhow::{Context, Result};
use rand_core::OsRng;
use russh::keys::{self, PrivateKey};
use std::{
    env, fs,
    io::IsTerminal,
    path::{Path, PathBuf},
};

pub(super) fn ensure_client_identity_at(explicit_path: Option<&Path>) -> Result<PathBuf> {
    let identity_path = match explicit_path {
        Some(path) => path.to_path_buf(),
        None => dedicated_identity_path()?,
    };
    if identity_path.exists() {
        return Ok(identity_path);
    }

    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        anyhow::bail!(
            "no SSH identity found; generate {} manually or rerun in an interactive terminal",
            identity_path.display()
        );
    }

    prompt_generate_identity(&identity_path)?;
    Ok(identity_path)
}

fn ssh_dir() -> Result<PathBuf> {
    let home = home_dir().context("could not determine home directory")?;
    Ok(home.join(".ssh"))
}

fn dedicated_identity_path() -> Result<PathBuf> {
    Ok(ssh_dir()?.join("id_late_sh_ed25519"))
}

fn home_dir() -> Option<PathBuf> {
    home_dir_from_env(
        env::var_os("HOME"),
        env::var_os("USERPROFILE"),
        env::var_os("HOMEDRIVE"),
        env::var_os("HOMEPATH"),
    )
}

fn home_dir_from_env(
    home: Option<std::ffi::OsString>,
    userprofile: Option<std::ffi::OsString>,
    homedrive: Option<std::ffi::OsString>,
    homepath: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if let Some(path) = home.filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path));
    }
    if let Some(path) = userprofile.filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path));
    }
    match (homedrive, homepath) {
        (Some(drive), Some(path)) if !drive.is_empty() && !path.is_empty() => {
            let mut combined = drive;
            combined.push(path);
            Some(PathBuf::from(combined))
        }
        _ => None,
    }
}

fn prompt_generate_identity(path: &Path) -> Result<()> {
    use std::io::Write;

    print!(
        "No SSH key found for late.sh.\n\
         Generate a dedicated Ed25519 key at {}? [y/N]: ",
        path.display()
    );
    std::io::stdout()
        .flush()
        .context("failed to flush prompt")?;

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("failed to read prompt response")?;

    if !is_affirmative(input.trim()) {
        anyhow::bail!("SSH key generation declined");
    }

    generate_identity(path)
}

fn is_affirmative(input: &str) -> bool {
    matches!(input, "y" | "Y" | "yes" | "YES" | "Yes")
}

fn generate_identity(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .context("generated identity path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }

    let key = PrivateKey::random(&mut OsRng, keys::Algorithm::Ed25519)
        .context("failed to generate Ed25519 key")?;
    let encoded = key
        .to_openssh(keys::ssh_key::LineEnding::LF)
        .context("failed to encode OpenSSH private key")?;
    fs::write(path, encoded.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
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
}
