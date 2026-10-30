use std::path::PathBuf;

use serde::Serialize;

use crate::cmd::wait::{Outcome, wait};
use crate::git;
use crate::hooks;
use crate::output::{self, WrkError};
use crate::paths::{self, Paths};
use crate::resolve::{self, Kind, validate_name};
use crate::state::HookStatus;

#[derive(Debug, Serialize)]
pub struct Created {
    pub repo: String,
    pub tree: String,
    pub path: PathBuf,
    pub branch: String,
    pub branch_source: BranchSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignored_remote_branch: Option<IgnoredRemote>,
    pub hook_status: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchSource {
    Local,
    Remote,
    New,
}

/// A same-named `origin` branch that a fresh tree did not use.
#[derive(Debug, Serialize)]
pub struct IgnoredRemote {
    pub name: String,
    pub last_commit: String,
}

/// Creates a tree for an already-resolved repo. `tree` is used literally,
/// never prefix-matched. The branch is `branch`, or `tree` when it is `None`.
/// A same-named `origin` branch is only used with `track`, because common
/// names often match someone's stale branch. A local branch with no commits
/// beyond origin's default is replaced, since it is only a stale pointer.
pub fn create(
    paths: &Paths,
    repo: &str,
    tree: &str,
    branch: Option<&str>,
    track: bool,
) -> Result<Created, WrkError> {
    validate_name(Kind::Tree, tree)?;
    let branch = branch.unwrap_or(tree);
    if !git::check_ref_format(branch)? {
        return Err(WrkError::InvalidName(format!(
            "{branch:?} is not a valid branch name"
        )));
    }

    let tree_path = paths.tree(repo, tree);
    let repo_path = paths.repo(repo);
    let mut ignored_remote_branch = None;
    let branch_source = {
        let lock_path = paths.repo_lock(repo);
        let _lock = crate::state::Lock::acquire_exclusive(&lock_path)?;

        if tree_path.exists() {
            return Err(WrkError::AlreadyExists(tree.to_string()));
        }

        git::fetch_prune(&repo_path)?;
        paths::ensure_dir(&paths.repo_trees_dir(repo))?;

        let default = git::default_branch(&repo_path)?;
        let remote_exists = git::remote_branch_exists(&repo_path, branch)?;
        let reuse_local = git::local_branch_exists(&repo_path, branch)? && {
            let ahead = git::commits_ahead_of_remote(&repo_path, &default, branch)?;
            if ahead == 0 {
                eprintln!("new: replacing local branch {branch}, which has no commits of its own");
                git::delete_branch(&repo_path, branch)?;
            }
            ahead > 0
        };
        if reuse_local {
            git::worktree_add(&repo_path, &tree_path, branch)?;
            BranchSource::Local
        } else if track {
            if !remote_exists {
                return Err(WrkError::NotFound(format!("origin/{branch} to track")));
            }
            git::worktree_add_track(&repo_path, &tree_path, branch, &format!("origin/{branch}"))?;
            BranchSource::Remote
        } else {
            if remote_exists {
                ignored_remote_branch = Some(IgnoredRemote {
                    name: format!("origin/{branch}"),
                    last_commit: git::remote_branch_summary(&repo_path, branch)?,
                });
            }
            git::worktree_add_new(&repo_path, &tree_path, branch, &format!("origin/{default}"))?;
            BranchSource::New
        }
    };

    let hook_status = hooks::start_on_create(paths, repo, tree)?;
    Ok(Created {
        repo: repo.to_string(),
        tree: tree.to_string(),
        path: tree_path,
        branch: branch.to_string(),
        branch_source,
        ignored_remote_branch,
        hook_status: hook_status.as_str(),
    })
}

pub fn run(
    paths: &Paths,
    repo_prefix: &str,
    tree: &str,
    branch: Option<&str>,
    track: bool,
    wait_for_hook: bool,
    json: bool,
) -> i32 {
    let repo = match resolve::resolve_repo(paths, repo_prefix) {
        Ok(r) => r,
        Err(e) => {
            output::emit_error(json, &e);
            return 1;
        }
    };

    eprintln!("new: creating {repo}/{tree}");
    let mut created = match create(paths, &repo, tree, branch, track) {
        Ok(c) => c,
        Err(e) => {
            output::emit_error(json, &e);
            return 1;
        }
    };
    print_ignored_remote_hint("new", &created);

    if !wait_for_hook {
        emit_created(&created, None, json);
        return 0;
    }

    eprintln!("new: waiting for on_create");
    match wait(paths, &repo, tree, None) {
        Ok(outcome) => {
            let code = outcome.exit_code();
            created.hook_status = outcome.as_str();
            if matches!(outcome, Outcome::Failed | Outcome::Crashed) {
                eprintln!("new: log at {}", paths.tree_log(&repo, tree).display());
            }
            emit_created(&created, Some(outcome), json);
            code
        }
        Err(e) => {
            output::emit_error(json, &e);
            1
        }
    }
}

#[derive(Serialize)]
struct CreatedWithOutcome<'a> {
    #[serde(flatten)]
    created: &'a Created,
    outcome: &'static str,
}

fn emit_created(created: &Created, outcome: Option<Outcome>, json: bool) {
    if json {
        match outcome {
            Some(outcome) => output::print_json(&CreatedWithOutcome {
                created,
                outcome: outcome.as_str(),
            }),
            None => output::print_json(created),
        }
        return;
    }
    println!("{}", created.path.display());
    if let Some(outcome) = outcome {
        eprintln!("new: hook {}", outcome.as_str());
    } else if created.hook_status != HookStatus::None.as_str() {
        eprintln!("new: hook {}", created.hook_status);
    }
}

pub fn print_ignored_remote_hint(prefix: &str, created: &Created) {
    if let Some(remote) = &created.ignored_remote_branch {
        eprintln!(
            "{prefix}: made a fresh branch; {} exists (last commit {}). Use --track to check it out instead.",
            remote.name, remote.last_commit
        );
    }
}
