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

fn git_in(dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Adds a commit to the origin's `main` through a scratch clone.
fn advance_origin(env: &Env, bare: &std::path::Path) -> String {
    let scratch = env.work.path().join("scratch");
    git_in(
        env.work.path(),
        &[
            "clone",
            "-q",
            bare.to_str().unwrap(),
            scratch.to_str().unwrap(),
        ],
    );
    std::fs::write(scratch.join("new.txt"), "new\n").unwrap();
    git_in(&scratch, &["add", "new.txt"]);
    git_in(&scratch, &["commit", "-q", "-m", "advance"]);
    git_in(&scratch, &["push", "-q", "origin", "main"]);
    git_in(&scratch, &["rev-parse", "HEAD"])
}

fn setup_origin(env: &Env) -> (String, std::path::PathBuf) {
    let bare = env.origin("proj", &["feature"]);
    let url = env.origin_url(&bare);
    env.cmd(&["clone", &url]).assert().success();
    ("proj".to_string(), bare)
}

#[test]
fn new_branch_flag_checks_out_that_branch_in_the_tree_folder() {
    let env = Env::new();
    let (repo, bare) = setup_origin(&env);
    let tip = advance_origin(&env, &bare);

    let out = env
        .cmd(&[
            "new",
            &repo,
            "ml-1-foo",
            "--branch",
            "josh/ml-1-foo",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value = json_of(&out);
    assert_eq!(value["tree"], "ml-1-foo");
    assert_eq!(value["branch"], "josh/ml-1-foo");
    assert_eq!(value["branch_source"], "new");
    let tree = env.tree_path(&repo, "ml-1-foo");
    assert_eq!(
        git_in(&tree, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "josh/ml-1-foo"
    );
    assert_eq!(git_in(&tree, &["rev-parse", "HEAD"]), tip);
}

#[test]
fn new_rejects_an_invalid_branch_name() {
    let env = Env::new();
    let (repo, _) = setup_origin(&env);

    env.cmd(&["new", &repo, "ok", "--branch", "bad..name"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("not a valid branch name"));
    assert!(!env.tree_path(&repo, "ok").exists());
}

#[test]
fn new_replaces_a_local_branch_with_no_commits_of_its_own() {
    let env = Env::new();
    let (repo, bare) = setup_origin(&env);
    let clone = env.root_path().join("repos").join(&repo);
    git_in(&clone, &["branch", "stale", "origin/main"]);
    let tip = advance_origin(&env, &bare);

    let out = env
        .cmd(&["new", &repo, "stale", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(json_of(&out)["branch_source"], "new");
    assert!(String::from_utf8_lossy(&out.stderr).contains("replacing local branch stale"));
    assert_eq!(
        git_in(&env.tree_path(&repo, "stale"), &["rev-parse", "HEAD"]),
        tip
    );
}

#[test]
fn new_reuses_a_local_branch_with_its_own_commit() {
    let env = Env::new();
    let (repo, _) = setup_origin(&env);
    let clone = env.root_path().join("repos").join(&repo);
    git_in(&clone, &["branch", "work", "origin/main"]);
    let scratch = env.work.path().join("local-work");
    git_in(
        &clone,
        &["worktree", "add", "-q", scratch.to_str().unwrap(), "work"],
    );
    git_in(&scratch, &["commit", "-q", "--allow-empty", "-m", "mine"]);
    let own = git_in(&scratch, &["rev-parse", "HEAD"]);
    git_in(
        &clone,
        &["worktree", "remove", "--force", scratch.to_str().unwrap()],
    );

    let out = env.cmd(&["new", &repo, "work", "--json"]).output().unwrap();
    assert!(out.status.success());
    assert_eq!(json_of(&out)["branch_source"], "local");
    assert_eq!(
        git_in(&env.tree_path(&repo, "work"), &["rev-parse", "HEAD"]),
        own
    );
}

#[test]
fn new_branch_with_track_checks_out_the_remote_branch() {
    let env = Env::new();
    let (repo, _) = setup_origin(&env);

    let out = env
        .cmd(&[
            "new",
            &repo,
            "tree-name",
            "--branch",
            "feature",
            "--track",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value = json_of(&out);
    assert_eq!(value["branch"], "feature");
    assert_eq!(value["branch_source"], "remote");
    assert_eq!(
        upstream(&env.tree_path(&repo, "tree-name")).as_deref(),
        Some("origin/feature")
    );
}
