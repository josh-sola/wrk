use std::time::{Duration, Instant};

use serde::Serialize;

use crate::output::{self, WrkError};
use crate::paths::Paths;
use crate::resolve;
use crate::state::{self, HookStatus, TreeState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    None,
    Failed,
    Crashed,
    TimedOut,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Succeeded => "succeeded",
            Outcome::None => "none",
            Outcome::Failed => "failed",
            Outcome::Crashed => "crashed",
            Outcome::TimedOut => "timed_out",
        }
    }

    pub fn exit_code(self) -> i32 {
        match self {
            Outcome::Succeeded | Outcome::None => 0,
            Outcome::Failed => 1,
            Outcome::Crashed | Outcome::TimedOut => 2,
        }
    }
}

const POLL_INTERVAL: Duration = Duration::from_millis(100);

fn outcome_after_release(state: &TreeState) -> Outcome {
    match state.status {
        HookStatus::Succeeded => Outcome::Succeeded,
        HookStatus::Failed => Outcome::Failed,
        HookStatus::None => Outcome::None,
        // The lock is free but the runner never wrote a terminal status: it
        // died mid-run.
        HookStatus::Running | HookStatus::Pending => Outcome::Crashed,
    }
}

/// Waits for a tree's hook to reach a terminal state. Blocks on the tree's
/// flock when no timeout is set, so it wakes the instant the runner exits.
pub fn wait(
    paths: &Paths,
    repo: &str,
    tree: &str,
    timeout: Option<Duration>,
) -> Result<Outcome, WrkError> {
    let lock_path = paths.tree_lock(repo, tree);
    let deadline = timeout.map(|t| Instant::now() + t);
    let mut pending_since: Option<Instant> = None;

    loop {
        let current = state::read_status(paths, repo, tree)?;
        match current.status {
            HookStatus::Succeeded => return Ok(Outcome::Succeeded),
            HookStatus::None => return Ok(Outcome::None),
            HookStatus::Failed => return Ok(Outcome::Failed),
            HookStatus::Running => {
                return match deadline {
                    None => {
                        let _lock = state::Lock::acquire_shared(&lock_path)?;
                        drop(_lock);
                        let after = state::read_status(paths, repo, tree)?;
                        Ok(outcome_after_release(&after))
                    }
                    Some(dl) if Instant::now() >= dl => Ok(Outcome::TimedOut),
                    Some(dl) => {
                        let now = Instant::now();
                        match state::try_acquire_shared_timeout(&lock_path, dl - now)? {
                            Some(lock) => {
                                drop(lock);
                                let after = state::read_status(paths, repo, tree)?;
                                Ok(outcome_after_release(&after))
                            }
                            None => Ok(Outcome::TimedOut),
                        }
                    }
                };
            }
            HookStatus::Pending => {
                let since = *pending_since.get_or_insert_with(Instant::now);
                if let Some(dl) = deadline
                    && Instant::now() >= dl
                {
                    return Ok(Outcome::TimedOut);
                }
                if since.elapsed() > state::PENDING_GRACE
                    && state::Lock::try_acquire_shared(&lock_path)?.is_some()
                {
                    return Ok(Outcome::Crashed);
                }
                std::thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct WaitOutput {
    repo: String,
    tree: String,
    outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    log_path: std::path::PathBuf,
}

pub fn run(
    paths: &Paths,
    repo_prefix: &str,
    tree_prefix: &str,
    timeout_secs: Option<u64>,
    json: bool,
) -> i32 {
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

    let timeout = timeout_secs.map(Duration::from_secs);
    let outcome = match wait(paths, &repo, &tree, timeout) {
        Ok(o) => o,
        Err(e) => {
            output::emit_error(json, &e);
            return 1;
        }
    };

    let final_state = state::read_status(paths, &repo, &tree).unwrap_or(TreeState::none(""));
    let log_path = paths.tree_log(&repo, &tree);

    if matches!(outcome, Outcome::Failed | Outcome::Crashed) && !json {
        eprintln!("wait: log at {}", log_path.display());
    }

    if json {
        output::print_json(&WaitOutput {
            repo,
            tree,
            outcome: outcome.as_str(),
            exit_code: final_state.exit_code,
            log_path,
        });
    } else {
        println!("{}", outcome.as_str());
    }

    outcome.exit_code()
}
