use std::path::Path;
use std::process::Command;

use crate::output::WrkError;

fn run(args: &[&str], cwd: Option<&Path>) -> Result<std::process::Output, WrkError> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    cmd.output().map_err(|e| WrkError::Io(e.to_string()))
}

/// Runs a git command and turns a non-zero exit into `git_failed`, carrying
/// git's stderr.
fn run_ok(args: &[&str], cwd: Option<&Path>) -> Result<String, WrkError> {
    let output = run(args, cwd)?;
    if !output.status.success() {
        return Err(WrkError::GitFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn check_ref_format(name: &str) -> Result<bool, WrkError> {
    let output = run(&["check-ref-format", "--branch", name], None)?;
    Ok(output.status.success())
}

pub fn clone_no_checkout(url: &str, dest: &Path) -> Result<(), WrkError> {
    let dest = dest.to_string_lossy().into_owned();
    run_ok(&["clone", "--no-checkout", url, &dest], None)?;
    Ok(())
}

pub fn rev_parse_head(repo_path: &Path) -> Result<String, WrkError> {
    run_ok(&["rev-parse", "HEAD"], Some(repo_path))
}

pub fn detach_head(repo_path: &Path, sha: &str) -> Result<(), WrkError> {
    run_ok(&["update-ref", "--no-deref", "HEAD", sha], Some(repo_path))?;
    Ok(())
}

pub fn remote_head_exists(repo_path: &Path) -> Result<bool, WrkError> {
    let output = run(
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        Some(repo_path),
    )?;
    Ok(output.status.success())
}

pub fn set_remote_head_auto(repo_path: &Path) -> Result<(), WrkError> {
    run_ok(&["remote", "set-head", "origin", "--auto"], Some(repo_path))?;
    Ok(())
}

/// The short default branch name, e.g. `main`, read from `origin/HEAD`.
pub fn default_branch(repo_path: &Path) -> Result<String, WrkError> {
    let short = run_ok(
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        Some(repo_path),
    )?;
    Ok(short.strip_prefix("origin/").unwrap_or(&short).to_string())
}

pub fn fetch_prune(repo_path: &Path) -> Result<(), WrkError> {
    run_ok(&["fetch", "origin", "--prune"], Some(repo_path))?;
    Ok(())
}

pub fn local_branch_exists(repo_path: &Path, branch: &str) -> Result<bool, WrkError> {
    let output = run(
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
        Some(repo_path),
    )?;
    Ok(output.status.success())
}

pub fn remote_branch_exists(repo_path: &Path, branch: &str) -> Result<bool, WrkError> {
    let output = run(
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/remotes/origin/{branch}"),
        ],
        Some(repo_path),
    )?;
    Ok(output.status.success())
}

pub fn worktree_add(repo_path: &Path, tree_path: &Path, branch: &str) -> Result<(), WrkError> {
    let tree_path = tree_path.to_string_lossy().into_owned();
    run_ok(&["worktree", "add", &tree_path, branch], Some(repo_path))?;
    Ok(())
}

pub fn worktree_add_track(
    repo_path: &Path,
    tree_path: &Path,
    branch: &str,
    remote_branch: &str,
) -> Result<(), WrkError> {
    let tree_path = tree_path.to_string_lossy().into_owned();
    run_ok(
        &[
            "worktree",
            "add",
            "--track",
            "-b",
            branch,
            &tree_path,
            remote_branch,
        ],
        Some(repo_path),
    )?;
    Ok(())
}

pub fn worktree_add_new(
    repo_path: &Path,
    tree_path: &Path,
    branch: &str,
    base: &str,
) -> Result<(), WrkError> {
    let tree_path = tree_path.to_string_lossy().into_owned();
    run_ok(
        &[
            "worktree",
            "add",
            "--no-track",
            "-b",
            branch,
            &tree_path,
            base,
        ],
        Some(repo_path),
    )?;
    Ok(())
}

pub fn worktree_remove_force(repo_path: &Path, tree_path: &Path) -> Result<(), WrkError> {
    let tree_path = tree_path.to_string_lossy().into_owned();
    run_ok(
        &["worktree", "remove", "--force", &tree_path],
        Some(repo_path),
    )?;
    Ok(())
}

pub fn worktree_prune(repo_path: &Path) -> Result<(), WrkError> {
    run_ok(&["worktree", "prune"], Some(repo_path))?;
    Ok(())
}

pub fn status_porcelain(tree_path: &Path) -> Result<String, WrkError> {
    run_ok(&["status", "--porcelain"], Some(tree_path))
}

pub fn current_branch(tree_path: &Path) -> Result<String, WrkError> {
    run_ok(&["rev-parse", "--abbrev-ref", "HEAD"], Some(tree_path))
}

/// For example `3 months ago by Ada Lovelace`.
pub fn remote_branch_summary(repo_path: &Path, branch: &str) -> Result<String, WrkError> {
    run_ok(
        &[
            "log",
            "-1",
            "--format=%cr by %an",
            &format!("refs/remotes/origin/{branch}"),
        ],
        Some(repo_path),
    )
}
