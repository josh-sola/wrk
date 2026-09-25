use std::path::PathBuf;

use serde::Serialize;

use crate::output::{self, WrkError};
use crate::paths::Paths;
use crate::resolve;

#[derive(Debug, Serialize)]
struct PathOutput {
    path: PathBuf,
}

fn resolve_path(
    paths: &Paths,
    repo_prefix: &str,
    tree_prefix: Option<&str>,
) -> Result<PathBuf, WrkError> {
    let repo = resolve::resolve_repo(paths, repo_prefix)?;
    match tree_prefix {
        Some(tree_prefix) => {
            let tree = resolve::resolve_tree(paths, &repo, tree_prefix)?;
            Ok(paths.tree(&repo, &tree))
        }
        None => Ok(paths.repo(&repo)),
    }
}

pub fn run(paths: &Paths, repo_prefix: &str, tree_prefix: Option<&str>, json: bool) -> i32 {
    match resolve_path(paths, repo_prefix, tree_prefix) {
        Ok(path) => {
            if json {
                output::print_json(&PathOutput { path });
            } else {
                println!("{}", path.display());
            }
            0
        }
        Err(e) => {
            output::emit_error(json, &e);
            1
        }
    }
}
