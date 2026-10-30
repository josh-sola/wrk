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

/// How many commits `branch` has that `origin/<base>` does not.
pub fn commits_ahead_of_remote(
    repo_path: &Path,
    base: &str,
    branch: &str,
) -> Result<u64, WrkError> {
    let range = format!("refs/remotes/origin/{base}..refs/heads/{branch}");
    let count = run_ok(&["rev-list", "--count", &range], Some(repo_path))?;
    count
        .parse()
        .map_err(|_| WrkError::GitFailed(format!("unexpected rev-list output: {count:?}")))
}

/// Deletes a local branch even if it is unmerged. Git refuses one that is
/// checked out in a worktree.
pub fn delete_branch(repo_path: &Path, branch: &str) -> Result<(), WrkError> {
    run_ok(&["branch", "-D", branch], Some(repo_path))?;
    Ok(())
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

/// Reads `HEAD` from the files, because `wrk ls` asks for every tree and a
/// git process per tree is slow. Anything unexpected falls back to git.
pub fn current_branch(tree_path: &Path) -> Result<String, WrkError> {
    match read_head_branch(tree_path) {
        Some(branch) => Ok(branch),
        None => run_ok(&["rev-parse", "--abbrev-ref", "HEAD"], Some(tree_path)),
    }
}

fn read_head_branch(tree_path: &Path) -> Option<String> {
    let dot_git = tree_path.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else {
        let text = std::fs::read_to_string(&dot_git).ok()?;
        let target = text.trim().strip_prefix("gitdir:")?.trim();
        tree_path.join(target)
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    if let Some(name) = head.strip_prefix("ref: refs/heads/") {
        return (!name.is_empty()).then(|| name.to_string());
    }
    let is_sha = matches!(head.len(), 40 | 64) && head.chars().all(|c| c.is_ascii_hexdigit());
    is_sha.then(|| "HEAD".to_string())
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
