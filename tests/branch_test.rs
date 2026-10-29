mod common;

use std::path::Path;
use std::process::Command;

use assert_cmd::prelude::*;
use common::Env;

fn clone_default(env: &Env) -> String {
    let bare = env.origin("proj", &[]);
    let url = env.origin_url(&bare);
    env.cmd(&["clone", &url]).assert().success();
    "proj".to_string()
}

fn git(env: &Env, dir: &Path, args: &[&str]) {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(dir);
    env.apply_env(&mut cmd);
    assert!(cmd.status().unwrap().success(), "git {args:?} failed");
}

fn listed_branch(env: &Env, repo: &str, tree: &str) -> String {
    let out = env.cmd(&["ls", repo, "--json"]).output().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let trees = value["repos"][0]["trees"].as_array().unwrap().clone();
    let found = trees.iter().find(|t| t["name"] == tree).unwrap();
    found["branch"].as_str().unwrap().to_string()
}

#[test]
fn ls_shows_a_branch_name_with_a_slash() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();
    git(
        &env,
        &env.tree_path(&repo, "t1"),
        &["branch", "-m", "user/topic"],
    );
    assert_eq!(listed_branch(&env, &repo, "t1"), "user/topic");
}

#[test]
fn ls_shows_head_for_a_detached_tree() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();
    git(
        &env,
        &env.tree_path(&repo, "t1"),
        &["checkout", "-q", "--detach"],
    );
    assert_eq!(listed_branch(&env, &repo, "t1"), "HEAD");
}

#[test]
fn current_branch_reads_a_plain_clone_with_a_git_directory() {
    let env = Env::new();
    let dir = env.work.path().join("plain");
    std::fs::create_dir_all(&dir).unwrap();
    git(&env, &dir, &["init", "-q", "-b", "trunk"]);
    assert_eq!(wrk::git::current_branch(&dir).unwrap(), "trunk");
}

#[test]
fn current_branch_follows_a_relative_gitdir_file() {
    let env = Env::new();
    let repo_dir = env.work.path().join("main-checkout");
    std::fs::create_dir_all(&repo_dir).unwrap();
    git(&env, &repo_dir, &["init", "-q", "-b", "main"]);
    let tree = env.work.path().join("linked");
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::rename(repo_dir.join(".git"), env.work.path().join("real.git")).unwrap();
    std::fs::write(tree.join(".git"), "gitdir: ../real.git\n").unwrap();
    assert_eq!(wrk::git::current_branch(&tree).unwrap(), "main");
}

#[test]
fn current_branch_falls_back_to_git_and_reports_its_error() {
    let env = Env::new();
    let dir = env.work.path().join("not-a-repo");
    std::fs::create_dir_all(&dir).unwrap();
    assert!(wrk::git::current_branch(&dir).is_err());
}
