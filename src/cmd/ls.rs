use std::path::PathBuf;

use serde::Serialize;

use crate::git;
use crate::output::{self, WrkError};
use crate::paths::Paths;
use crate::resolve;
use crate::state::{self, HookStatus};

#[derive(Debug, Serialize)]
struct TreeInfo {
    name: String,
    path: PathBuf,
    branch: String,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct RepoInfo {
    name: String,
    path: PathBuf,
    trees: Vec<TreeInfo>,
}

#[derive(Debug, Serialize)]
struct Listing {
    repos: Vec<RepoInfo>,
}

/// The hook status to report for a tree: `running` becomes `crashed` when
/// its lock is free, since nothing is left holding it.
pub(crate) fn effective_status(
    paths: &Paths,
    repo: &str,
    tree: &str,
) -> Result<&'static str, WrkError> {
    let state = state::read_status(paths, repo, tree)?;
    if state.status == HookStatus::Running {
        let lock_path = paths.tree_lock(repo, tree);
        if state::Lock::try_acquire_shared(&lock_path)?.is_some() {
            return Ok("crashed");
        }
    }
    Ok(state.status.as_str())
}

fn tree_info(paths: &Paths, repo: &str, tree: &str) -> Result<TreeInfo, WrkError> {
    let path = paths.tree(repo, tree);
    let branch = git::current_branch(&path).unwrap_or_else(|_| tree.to_string());
    let status = effective_status(paths, repo, tree)?;
    Ok(TreeInfo {
        name: tree.to_string(),
        path,
        branch,
        status,
    })
}

fn repo_info(paths: &Paths, repo: &str) -> Result<RepoInfo, WrkError> {
    let trees = resolve::trees(paths, repo)?
        .into_iter()
        .map(|t| tree_info(paths, repo, &t))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RepoInfo {
        name: repo.to_string(),
        path: paths.repo(repo),
        trees,
    })
}

fn list(paths: &Paths, repo_prefix: Option<&str>) -> Result<Listing, WrkError> {
    let repos = match repo_prefix {
        Some(prefix) => vec![resolve::resolve_repo(paths, prefix)?],
        None => resolve::repos(paths)?,
    };
    let repos = repos
        .iter()
        .map(|r| repo_info(paths, r))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Listing { repos })
}

fn print_human(listing: &Listing) {
    let rows: Vec<(String, String, String, String)> = listing
        .repos
        .iter()
        .flat_map(|repo| {
            repo.trees.iter().map(move |tree| {
                (
                    repo.name.clone(),
                    tree.name.clone(),
                    tree.branch.clone(),
                    tree.status.to_string(),
                )
            })
        })
        .collect();

    if rows.is_empty() {
        return;
    }

    let header = (
        "REPO".to_string(),
        "TREE".to_string(),
        "BRANCH".to_string(),
        "STATUS".to_string(),
    );
    let mut widths = [
        header.0.len(),
        header.1.len(),
        header.2.len(),
        header.3.len(),
    ];
    for row in &rows {
        widths[0] = widths[0].max(row.0.len());
        widths[1] = widths[1].max(row.1.len());
        widths[2] = widths[2].max(row.2.len());
        widths[3] = widths[3].max(row.3.len());
    }

    let print_row = |a: &str, b: &str, c: &str, d: &str| {
        println!(
            "{:w0$}  {:w1$}  {:w2$}  {:w3$}",
            a,
            b,
            c,
            d,
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2],
            w3 = widths[3]
        );
    };
    print_row(&header.0, &header.1, &header.2, &header.3);
    for row in &rows {
        print_row(&row.0, &row.1, &row.2, &row.3);
    }
}

pub fn run(paths: &Paths, repo_prefix: Option<&str>, json: bool) -> i32 {
    match list(paths, repo_prefix) {
        Ok(listing) => {
            if json {
                output::print_json(&listing);
            } else {
                print_human(&listing);
            }
            0
        }
        Err(e) => {
            output::emit_error(json, &e);
            1
        }
    }
}
