use std::time::{Duration, Instant};

use rustix::process::{Pid, Signal, kill_process_group};
use serde::Serialize;

use crate::git;
use crate::hooks;
use crate::output::{self, WrkError};
use crate::paths::Paths;
use crate::resolve;
use crate::state::{self, HookStatus, Lock};

const FORCE_KILL_GRACE: Duration = Duration::from_secs(2);
const FORCE_POLL_INTERVAL: Duration = Duration::from_millis(50);

fn signal_runner_group(pid: u32, sig: Signal) {
    if let Some(pid) = Pid::from_raw(pid as i32) {
        let _ = kill_process_group(pid, sig);
    }
}

/// Acquires the tree lock exclusively, refusing (unless `force`) when a
/// runner currently holds it. With `force`, kills the runner's process
/// group and waits for the lock to free, escalating to `SIGKILL`.
fn acquire_tree_lock(paths: &Paths, repo: &str, tree: &str, force: bool) -> Result<Lock, WrkError> {
    let lock_path = paths.tree_lock(repo, tree);
    if let Some(lock) = Lock::try_acquire_exclusive(&lock_path)? {
        return Ok(lock);
    }
    if !force {
        return Err(WrkError::HookRunning(tree.to_string()));
    }

    let runner_pid = state::read_status(paths, repo, tree)?.pid;
    if let Some(pid) = runner_pid {
        signal_runner_group(pid, Signal::TERM);
    }

    let deadline = Instant::now() + FORCE_KILL_GRACE;
    loop {
        if let Some(lock) = Lock::try_acquire_exclusive(&lock_path)? {
            return Ok(lock);
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(FORCE_POLL_INTERVAL);
    }

    if let Some(pid) = runner_pid {
        signal_runner_group(pid, Signal::KILL);
    }
    Ok(Lock::acquire_exclusive(&lock_path)?)
}

/// Keeps the branch.
pub fn remove(paths: &Paths, repo: &str, tree: &str, force: bool) -> Result<(), WrkError> {
    let tree_path = paths.tree(repo, tree);
    if !tree_path.exists() {
        return Err(WrkError::NotFound(format!("tree {tree:?} in {repo:?}")));
    }

    let _tree_lock = acquire_tree_lock(paths, repo, tree, force)?;

    // A runner spawned moments ago may not hold the lock yet. If we remove
    // the tree anyway, the runner sees it gone and does nothing.
    let status = state::read_status(paths, repo, tree)?;
    if status.status == HookStatus::Pending {
        let age = status
            .created_at
            .map(|t| state::now_unix().saturating_sub(t));
        let still_within_grace = age.is_none_or(|age| age < state::PENDING_GRACE.as_secs());
        if still_within_grace && !force {
            return Err(WrkError::HookRunning(tree.to_string()));
        }
    }

    let dirty = !git::status_porcelain(&tree_path)?.trim().is_empty();
    if dirty && !force {
        return Err(WrkError::DirtyTree(tree.to_string()));
    }

    match hooks::run_on_destroy(paths, repo, tree)? {
        Some(status) if !status.success() && !force => {
            return Err(WrkError::HookFailed(status.code().unwrap_or(-1)));
        }
        Some(_) | None => {}
    }

    let repo_path = paths.repo(repo);
    {
        let repo_lock_path = paths.repo_lock(repo);
        let _repo_lock = Lock::acquire_exclusive(&repo_lock_path)?;
        if git::worktree_remove_force(&repo_path, &tree_path).is_err() {
            std::fs::remove_dir_all(&tree_path)?;
            git::worktree_prune(&repo_path)?;
        }
    }

    drop(_tree_lock);
    let _ = std::fs::remove_file(paths.tree_status(repo, tree));
    let _ = std::fs::remove_file(paths.tree_lock(repo, tree));
    let _ = std::fs::remove_file(paths.tree_log(repo, tree));
    Ok(())
}

#[derive(Debug, Serialize)]
struct Removed {
    repo: String,
    tree: String,
    removed: bool,
}

pub fn run(paths: &Paths, repo_prefix: &str, tree_prefix: &str, force: bool, json: bool) -> i32 {
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

    match remove(paths, &repo, &tree, force) {
        Ok(()) => {
            if json {
                output::print_json(&Removed {
                    repo,
                    tree,
                    removed: true,
                });
            } else {
                println!("removed {repo}/{tree}");
            }
            0
        }
        Err(e) => {
            output::emit_error(json, &e);
            1
        }
    }
}
