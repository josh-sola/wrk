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
    pub hook_status: &'static str,
}

/// Creates a tree for an already-resolved repo. `tree` is used literally,
/// never prefix-matched.
pub fn create(paths: &Paths, repo: &str, tree: &str) -> Result<Created, WrkError> {
    validate_name(Kind::Tree, tree)?;
    if !git::check_ref_format(tree)? {
        return Err(WrkError::InvalidName(format!(
            "{tree:?} is not a valid branch name"
        )));
    }

    let tree_path = paths.tree(repo, tree);
    let repo_path = paths.repo(repo);
    {
        let lock_path = paths.repo_lock(repo);
        let _lock = crate::state::Lock::acquire_exclusive(&lock_path)?;

        if tree_path.exists() {
            return Err(WrkError::AlreadyExists(tree.to_string()));
        }

        git::fetch_prune(&repo_path)?;
        paths::ensure_dir(&paths.repo_trees_dir(repo))?;

        if git::local_branch_exists(&repo_path, tree)? {
            git::worktree_add(&repo_path, &tree_path, tree)?;
        } else if git::remote_branch_exists(&repo_path, tree)? {
            git::worktree_add_track(&repo_path, &tree_path, tree, &format!("origin/{tree}"))?;
        } else {
            let default = git::default_branch(&repo_path)?;
            git::worktree_add_new(&repo_path, &tree_path, tree, &format!("origin/{default}"))?;
        }
    }

    let hook_status = hooks::start_on_create(paths, repo, tree)?;
    Ok(Created {
        repo: repo.to_string(),
        tree: tree.to_string(),
        path: tree_path,
        branch: tree.to_string(),
        hook_status: hook_status.as_str(),
    })
}

pub fn run(paths: &Paths, repo_prefix: &str, tree: &str, wait_for_hook: bool, json: bool) -> i32 {
    let repo = match resolve::resolve_repo(paths, repo_prefix) {
        Ok(r) => r,
        Err(e) => {
            output::emit_error(json, &e);
            return 1;
        }
    };

    eprintln!("new: creating {repo}/{tree}");
    let mut created = match create(paths, &repo, tree) {
        Ok(c) => c,
        Err(e) => {
            output::emit_error(json, &e);
            return 1;
        }
    };

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
