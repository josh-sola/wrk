use std::io::IsTerminal;

use crate::cmd::ls;
use crate::cmd::new;
use crate::cmd::wait::{Outcome, wait};
use crate::config;
use crate::harness;
use crate::hooks;
use crate::output::{self, WrkError};
use crate::paths::Paths;
use crate::resolve::{self, TreeMatch};
use crate::state::{self, HookStatus};
use crate::tui::{self, PickInput, TreeRow};

#[derive(Debug, Clone, Copy, Default)]
pub struct GoFlags {
    pub new_tree: bool,
    pub wait: bool,
    pub track: bool,
    pub fill: bool,
}

pub fn run(
    paths: &Paths,
    repo: Option<String>,
    tree: Option<String>,
    harness_name: Option<String>,
    flags: GoFlags,
    extra_args: Vec<String>,
) -> i32 {
    match (repo, tree, harness_name) {
        (None, None, None) if flags.new_tree => {
            let err = WrkError::Usage("--new needs <repo> <tree> <harness>".to_string());
            output::print_error_human(&err);
            1
        }
        (None, None, None) => run_tui(paths, flags, &extra_args),
        (Some(repo), Some(tree), Some(harness_name)) => {
            go(paths, &repo, &tree, &harness_name, flags, &extra_args)
        }
        _ => {
            let err = WrkError::Usage(
                "wrk go needs all three of <repo> <tree> <harness>, or none of them".to_string(),
            );
            output::print_error_human(&err);
            1
        }
    }
}

fn run_tui(paths: &Paths, flags: GoFlags, extra_args: &[String]) -> i32 {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        let err =
            WrkError::Usage("wrk go needs a terminal; pass <repo> <tree> <harness>".to_string());
        output::print_error_human(&err);
        return 1;
    }

    let input = match pick_input(paths) {
        Ok(i) => i,
        Err(e) => {
            output::print_error_human(&e);
            return 1;
        }
    };

    let sizing = if flags.fill {
        tui::Sizing::Fill
    } else {
        tui::Sizing::Centered
    };
    let target = match tui::pick(input, sizing) {
        Ok(Some(target)) => target,
        Ok(None) => return 130,
        Err(e) => {
            output::print_error_human(&e);
            return 1;
        }
    };

    let flags = GoFlags {
        new_tree: target.new,
        ..flags
    };
    go(
        paths,
        &target.repo,
        &target.tree,
        &target.harness,
        flags,
        extra_args,
    )
}

/// Shared by `go` and `pick` so both list trees and statuses the same way.
pub(crate) fn pick_input(paths: &Paths) -> Result<PickInput, WrkError> {
    let config = config::load(paths)?;
    let trees = tree_rows(paths)?;
    let repos = resolve::repos(paths)?;
    let mut harnesses: Vec<String> = config.harnesses.keys().cloned().collect();
    harnesses.sort();

    Ok(PickInput {
        trees,
        repos,
        harnesses,
        default_harness: Some(config.default_harness.clone()),
    })
}

/// Every tree across every repo, with the status `ls` would show for it.
fn tree_rows(paths: &Paths) -> Result<Vec<TreeRow>, WrkError> {
    let mut rows = Vec::new();
    for repo in resolve::repos(paths)? {
        for tree in resolve::trees(paths, &repo)? {
            let status = ls::effective_status(paths, &repo, &tree)?.to_string();
            rows.push(TreeRow {
                repo: repo.clone(),
                tree,
                status,
            });
        }
    }
    Ok(rows)
}

/// The only path that launches a harness, whether reached directly or from
/// the TUI.
fn go(
    paths: &Paths,
    repo_prefix: &str,
    tree_prefix: &str,
    harness_prefix: &str,
    flags: GoFlags,
    extra_args: &[String],
) -> i32 {
    let config = match config::load(paths) {
        Ok(c) => c,
        Err(e) => {
            output::print_error_human(&e);
            return 1;
        }
    };
    let repo = match resolve::resolve_repo(paths, repo_prefix) {
        Ok(r) => r,
        Err(e) => {
            output::print_error_human(&e);
            return 1;
        }
    };
    let harness = match harness::resolve(&config, harness_prefix) {
        Ok(h) => h,
        Err(e) => {
            output::print_error_human(&e);
            return 1;
        }
    };

    let tree_match = match resolve::resolve_or_new(paths, &repo, tree_prefix, flags.new_tree) {
        Ok(m) => m,
        Err(e) => {
            output::print_error_human(&e);
            return 1;
        }
    };
    let tree = match tree_match {
        TreeMatch::Existing(name) => name,
        TreeMatch::Create(name) => {
            eprintln!("go: creating {repo}/{name}");
            match new::create(paths, &repo, &name, flags.track) {
                Ok(created) => new::print_ignored_remote_hint("go", &created),
                Err(e) => {
                    output::print_error_human(&e);
                    return 1;
                }
            }
            name
        }
    };

    let result = if flags.wait {
        wait_before_launch(paths, &repo, &tree)
    } else {
        note_hook_status(paths, &repo, &tree)
    };
    if let Err(code) = result {
        return code;
    }

    let tree_path = paths.tree(&repo, &tree);
    let env = hooks::hook_env(paths, &repo, &tree);
    let err = harness::exec_in(&harness, &tree_path, &tree, env, extra_args);
    output::print_error_human(&err);
    1
}

fn wait_before_launch(paths: &Paths, repo: &str, tree: &str) -> Result<(), i32> {
    let status = state::read_status(paths, repo, tree).map_err(|e| {
        output::print_error_human(&e);
        1
    })?;
    if matches!(status.status, HookStatus::Pending | HookStatus::Running) {
        eprintln!("go: waiting for on_create…");
    }
    let outcome = wait(paths, repo, tree, None).map_err(|e| {
        output::print_error_human(&e);
        1
    })?;
    if matches!(outcome, Outcome::Failed | Outcome::Crashed) {
        eprintln!("go: log at {}", paths.tree_log(repo, tree).display());
        return Err(outcome.exit_code());
    }
    Ok(())
}

fn note_hook_status(paths: &Paths, repo: &str, tree: &str) -> Result<(), i32> {
    let status = ls::effective_status(paths, repo, tree).map_err(|e| {
        output::print_error_human(&e);
        1
    })?;
    let log = paths.tree_log(repo, tree);
    match status {
        "pending" | "running" => {
            eprintln!("go: on_create is still running; follow it with `wrk logs -f {repo} {tree}`")
        }
        "failed" | "crashed" => {
            eprintln!("go: on_create {status}; log at {}", log.display())
        }
        _ => {}
    }
    Ok(())
}
