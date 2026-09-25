mod common;

use assert_cmd::prelude::*;
use common::Env;
use predicates::prelude::*;

fn clone_default(env: &Env) -> String {
    let bare = env.origin("proj", &["feature"]);
    let url = env.origin_url(&bare);
    env.cmd(&["clone", &url]).assert().success();
    "proj".to_string()
}

#[test]
fn new_on_default_branch_name_works_despite_detached_clone() {
    let env = Env::new();
    let repo = clone_default(&env);

    env.cmd(&["new", &repo, "main"])
        .assert()
        .success()
        .stdout(predicate::str::contains("trees/proj/main"));
    assert!(env.tree_path(&repo, "main").join("README.md").exists());
}

#[test]
fn new_tracks_existing_remote_branch() {
    let env = Env::new();
    let repo = clone_default(&env);

    env.cmd(&["new", &repo, "feature"]).assert().success();
    let tree = env.tree_path(&repo, "feature");
    assert!(tree.exists());

    let branch = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&tree)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&branch.stdout).trim(), "feature");
}

#[test]
fn new_creates_branch_from_default_when_neither_exists() {
    let env = Env::new();
    let repo = clone_default(&env);

    env.cmd(&["new", &repo, "brand-new"]).assert().success();
    assert!(env.tree_path(&repo, "brand-new").exists());

    let branch = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(env.tree_path(&repo, "brand-new"))
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&branch.stdout).trim(), "brand-new");
}

#[test]
fn new_refuses_when_tree_already_exists() {
    let env = Env::new();
    let repo = clone_default(&env);

    env.cmd(&["new", &repo, "main"]).assert().success();
    env.cmd(&["new", &repo, "main"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn new_rejects_invalid_tree_names() {
    let env = Env::new();
    let repo = clone_default(&env);

    env.cmd(&["new", &repo, "has/slash"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("invalid"));
}

#[test]
fn new_resolves_repo_by_prefix() {
    let env = Env::new();
    let repo = clone_default(&env);

    env.cmd(&["new", &repo[..2], "main"]).assert().success();
    assert!(env.tree_path(&repo, "main").exists());
}
