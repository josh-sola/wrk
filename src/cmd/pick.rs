use std::path::PathBuf;

use serde::Serialize;

use crate::cmd::go;
use crate::output::{self, WrkError};
use crate::paths::Paths;
use crate::tui::{self, GoTarget};

#[derive(Debug, Serialize)]
struct Selection {
    repo: String,
    tree: String,
    harness: String,
    new: bool,
    path: PathBuf,
    command: Vec<String>,
}

pub fn run(paths: &Paths, json: bool) -> i32 {
    let input = match go::pick_input(paths) {
        Ok(i) => i,
        Err(e) => {
            output::emit_error(json, &e);
            return 1;
        }
    };

    let target = match tui::pick(input) {
        Ok(Some(target)) => target,
        Ok(None) => {
            if json {
                println!("{}", serde_json::json!({ "cancelled": true }));
            }
            return 130;
        }
        Err(WrkError::NoTerminal) => {
            let err = WrkError::Usage("wrk pick needs a terminal".to_string());
            output::emit_error(json, &err);
            return 1;
        }
        Err(e) => {
            output::emit_error(json, &e);
            return 1;
        }
    };

    let path = paths.tree(&target.repo, &target.tree);
    let selection = selection_from(&target, path);
    if json {
        output::print_json(&selection);
    } else {
        println!("{}", selection.path.display());
    }
    0
}

/// Maps a picker result to what `pick` prints: the tree path and the `wrk
/// go` invocation that would launch it, so another tool can place the
/// session itself instead of `wrk go` doing it inline.
fn selection_from(target: &GoTarget, path: PathBuf) -> Selection {
    let mut command = vec![
        "wrk".to_string(),
        "go".to_string(),
        target.repo.clone(),
        target.tree.clone(),
        target.harness.clone(),
    ];
    if target.new {
        command.push("--new".to_string());
    }
    Selection {
        repo: target.repo.clone(),
        tree: target.tree.clone(),
        harness: target.harness.clone(),
        new: target.new,
        path,
        command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(new: bool) -> GoTarget {
        GoTarget {
            repo: "wrk".to_string(),
            tree: "feature".to_string(),
            harness: "claude".to_string(),
            new,
        }
    }

    #[test]
    fn existing_tree_has_no_new_flag_in_command() {
        let selection = selection_from(&target(false), PathBuf::from("/root/trees/wrk/feature"));
        assert_eq!(
            selection.command,
            vec!["wrk", "go", "wrk", "feature", "claude"]
        );
        assert_eq!(selection.path, PathBuf::from("/root/trees/wrk/feature"));
        assert!(!selection.new);
    }

    #[test]
    fn new_tree_appends_new_flag_to_command() {
        let selection = selection_from(&target(true), PathBuf::from("/root/trees/wrk/feature"));
        assert_eq!(
            selection.command,
            vec!["wrk", "go", "wrk", "feature", "claude", "--new"]
        );
        assert!(selection.new);
    }

    #[test]
    fn selection_serializes_the_documented_shape() {
        let selection = selection_from(&target(false), PathBuf::from("/root/trees/wrk/feature"));
        let value = serde_json::to_value(&selection).unwrap();
        assert_eq!(value["repo"], "wrk");
        assert_eq!(value["tree"], "feature");
        assert_eq!(value["harness"], "claude");
        assert_eq!(value["new"], false);
        assert_eq!(value["path"], "/root/trees/wrk/feature");
        assert_eq!(
            value["command"],
            serde_json::json!(["wrk", "go", "wrk", "feature", "claude"])
        );
    }
}
