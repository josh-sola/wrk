use std::io::Write;
use std::time::Duration;

use serde::Serialize;

use crate::output::{self, WrkError};
use crate::paths::Paths;
use crate::resolve;
use crate::state::{self, HookStatus};

const POLL_INTERVAL: Duration = Duration::from_millis(100);

fn is_finished(paths: &Paths, repo: &str, tree: &str) -> Result<bool, WrkError> {
    let state = state::read_status(paths, repo, tree)?;
    match state.status {
        HookStatus::Succeeded | HookStatus::Failed | HookStatus::None => Ok(true),
        HookStatus::Pending => Ok(false),
        HookStatus::Running => {
            let lock_path = paths.tree_lock(repo, tree);
            Ok(state::Lock::try_acquire_shared(&lock_path)?.is_some())
        }
    }
}

#[derive(Debug, Serialize)]
struct LogsOutput {
    path: std::path::PathBuf,
    content: String,
}

pub fn run(paths: &Paths, repo_prefix: &str, tree_prefix: &str, follow: bool, json: bool) -> i32 {
    if json && follow {
        output::emit_error(
            json,
            &WrkError::Usage("--json is not supported with -f".to_string()),
        );
        return 1;
    }

    let repo = match resolve::resolve_repo(paths, repo_prefix) {
        Ok(r) => r,
        Err(e) => {
            output::emit_error(json, &e);
            return 1;
        }
    };
    let tree = match resolve::resolve_tree(paths, &repo, tree_prefix) {
        Ok(t) => t,
        Err(e) => {
            output::emit_error(json, &e);
            return 1;
        }
    };

    let log_path = paths.tree_log(&repo, &tree);

    if !follow {
        let content = match state::read_log(&log_path) {
            Ok(c) => c,
            Err(e) => {
                output::emit_error(json, &WrkError::from(e));
                return 1;
            }
        };
        if json {
            output::print_json(&LogsOutput {
                path: log_path,
                content,
            });
        } else {
            print!("{content}");
        }
        return 0;
    }

    let mut offset = 0u64;
    loop {
        match state::read_log_from(&log_path, offset) {
            Ok((chunk, new_offset)) => {
                if !chunk.is_empty() {
                    print!("{chunk}");
                    let _ = std::io::stdout().flush();
                    offset = new_offset;
                }
            }
            Err(e) => {
                output::emit_error(json, &WrkError::from(e));
                return 1;
            }
        }
        match is_finished(paths, &repo, &tree) {
            Ok(true) => return 0,
            Ok(false) => std::thread::sleep(POLL_INTERVAL),
            Err(e) => {
                output::emit_error(json, &e);
                return 1;
            }
        }
    }
}
