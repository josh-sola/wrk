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

fn upstream(tree: &std::path::Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "@{u}"])
        .current_dir(tree)
        .output()
        .unwrap();
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn json_of(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn new_ignores_a_same_named_remote_branch_by_default() {
    let env = Env::new();
    let repo = clone_default(&env);

    let out = env
        .cmd(&["new", &repo, "feature", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value = json_of(&out);
    assert_eq!(value["branch"], "feature");
    assert_eq!(value["branch_source"], "new");
    assert_eq!(value["ignored_remote_branch"]["name"], "origin/feature");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("Use --track"),
        "stderr should hint at --track"
    );
    assert_eq!(upstream(&env.tree_path(&repo, "feature")), None);
}

#[test]
fn new_track_checks_out_the_remote_branch() {
    let env = Env::new();
    let repo = clone_default(&env);

    let out = env
        .cmd(&["new", &repo, "feature", "--track", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value = json_of(&out);
    assert_eq!(value["branch_source"], "remote");
    assert!(value.get("ignored_remote_branch").is_none());
    assert_eq!(
        upstream(&env.tree_path(&repo, "feature")).as_deref(),
        Some("origin/feature")
    );
}

#[test]
fn new_track_without_a_remote_branch_is_not_found() {
    let env = Env::new();
    let repo = clone_default(&env);

    let out = env
        .cmd(&["new", &repo, "nothing-here", "--track", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(json_of(&out)["error"]["code"], "not_found");
    assert!(!env.tree_path(&repo, "nothing-here").exists());
}

#[test]
fn new_creates_branch_from_default_when_neither_exists() {
    let env = Env::new();
    let repo = clone_default(&env);

    let out = env
        .cmd(&["new", &repo, "brand-new", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value = json_of(&out);
    assert_eq!(value["branch_source"], "new");
    assert!(value.get("ignored_remote_branch").is_none());
    assert!(!String::from_utf8_lossy(&out.stderr).contains("--track"));
    assert_eq!(upstream(&env.tree_path(&repo, "brand-new")), None);
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
