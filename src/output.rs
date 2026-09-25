use std::path::PathBuf;

use serde::Serialize;

/// The one error type every command surfaces. `code()` is the stable
/// snake_case identifier printed in JSON error output.
#[derive(Debug, thiserror::Error)]
pub enum WrkError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("ambiguous prefix {prefix:?}: matches {candidates:?}")]
    AmbiguousPrefix {
        prefix: String,
        candidates: Vec<String>,
    },

    #[error("already exists: {0}")]
    AlreadyExists(String),

    #[error("invalid name: {0}")]
    InvalidName(String),

    #[error("tree has uncommitted changes: {0}")]
    DirtyTree(String),

    #[error("hook is running for {0}")]
    HookRunning(String),

    #[error("hook failed with exit code {0}")]
    HookFailed(i32),

    #[error("hook not executable: {0}")]
    HookNotExecutable(String),

    #[error("hook crashed")]
    HookCrashed,

    #[error("timed out waiting for hook")]
    Timeout,

    #[error("invalid config at {path}: {message}")]
    ConfigInvalid { path: PathBuf, message: String },

    #[error("harness not found: {0}")]
    HarnessNotFound(String),

    #[error("git failed: {0}")]
    GitFailed(String),

    #[error("io error: {0}")]
    Io(String),

    #[error("usage error: {0}")]
    Usage(String),

    #[error("no terminal available")]
    NoTerminal,
}

impl WrkError {
    pub fn code(&self) -> &'static str {
        match self {
            WrkError::NotFound(_) => "not_found",
            WrkError::AmbiguousPrefix { .. } => "ambiguous_prefix",
            WrkError::AlreadyExists(_) => "already_exists",
            WrkError::InvalidName(_) => "invalid_name",
            WrkError::DirtyTree(_) => "dirty_tree",
            WrkError::HookRunning(_) => "hook_running",
            WrkError::HookFailed(_) => "hook_failed",
            WrkError::HookNotExecutable(_) => "hook_not_executable",
            WrkError::HookCrashed => "hook_crashed",
            WrkError::Timeout => "timeout",
            WrkError::ConfigInvalid { .. } => "config_invalid",
            WrkError::HarnessNotFound(_) => "harness_not_found",
            WrkError::GitFailed(_) => "git_failed",
            WrkError::Io(_) => "io",
            WrkError::Usage(_) => "usage",
            WrkError::NoTerminal => "no_terminal",
        }
    }

    pub fn candidates(&self) -> Option<&[String]> {
        match self {
            WrkError::AmbiguousPrefix { candidates, .. } => Some(candidates),
            _ => None,
        }
    }
}

impl From<std::io::Error> for WrkError {
    fn from(e: std::io::Error) -> Self {
        WrkError::Io(e.to_string())
    }
}

pub fn print_error_human(err: &WrkError) {
    eprintln!("error: {err}");
    if let Some(candidates) = err.candidates() {
        eprintln!("candidates: {}", candidates.join(", "));
    }
}

pub fn print_error_json(err: &WrkError) {
    let mut error = serde_json::json!({
        "code": err.code(),
        "message": err.to_string(),
    });
    if let Some(candidates) = err.candidates() {
        error["candidates"] = serde_json::json!(candidates);
    }
    println!("{}", serde_json::json!({ "error": error }));
}

pub fn emit_error(json: bool, err: &WrkError) {
    if json {
        print_error_json(err);
    } else {
        print_error_human(err);
    }
}

pub fn print_json<T: Serialize>(value: &T) {
    println!(
        "{}",
        serde_json::to_string(value).expect("value must serialize")
    );
}
