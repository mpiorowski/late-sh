use anyhow::{Context, Result};
use serde_json::json;

pub(crate) const CLI_TOKEN_REQUEST: &str = "late-cli-token-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecResponse {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) exit_status: u32,
}

impl ExecResponse {
    pub(crate) fn success(stdout: String) -> Self {
        Self {
            stdout,
            stderr: String::new(),
            exit_status: 0,
        }
    }

    pub(crate) fn failure(stderr: String) -> Self {
        Self {
            stdout: String::new(),
            stderr,
            exit_status: 1,
        }
    }
}

pub(crate) fn handle_exec_command(command: &str, session_token: &str) -> Result<ExecResponse> {
    match command.trim() {
        CLI_TOKEN_REQUEST => {
            let stdout = serde_json::to_string(&json!({ "session_token": session_token }))
                .context("failed to encode cli token exec response")?;
            Ok(ExecResponse::success(stdout))
        }
        other => Ok(ExecResponse::failure(format!(
            "unsupported exec command: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_token_exec_returns_session_token_json() {
        let response = handle_exec_command(CLI_TOKEN_REQUEST, "tok").unwrap();
        assert_eq!(response.exit_status, 0);
        assert_eq!(response.stderr, "");
        assert_eq!(response.stdout, r#"{"session_token":"tok"}"#);
    }

    #[test]
    fn unsupported_command_is_normal_exec_failure() {
        let response = handle_exec_command("other", "tok").unwrap();
        assert_eq!(response.exit_status, 1);
        assert!(response.stdout.is_empty());
        assert!(response.stderr.contains("unsupported exec command"));
    }
}
