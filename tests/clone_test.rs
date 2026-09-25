mod common;

use assert_cmd::prelude::*;
use common::Env;
use predicates::prelude::*;

#[test]
fn clone_creates_repo_with_detached_head() {
    let env = Env::new();
    let bare = env.origin("proj", &[]);
    let url = env.origin_url(&bare);

    env.cmd(&["clone", &url])
        .assert()
        .success()
        .stdout(predicate::str::contains("repos/proj"));

    let repo_path = env.root_path().join("repos/proj");
    assert!(repo_path.join(".git/HEAD").exists());

    let head = std::fs::read_to_string(repo_path.join(".git/HEAD")).unwrap();
    assert!(
        !head.starts_with("ref:"),
        "HEAD should be detached, got {head:?}"
    );
}

#[test]
fn clone_accepts_an_explicit_name() {
    let env = Env::new();
    let bare = env.origin("proj", &[]);
    let url = env.origin_url(&bare);

    env.cmd(&["clone", &url, "myrepo"]).assert().success();
    assert!(env.root_path().join("repos/myrepo").exists());
}

#[test]
fn clone_refuses_when_repo_already_exists() {
    let env = Env::new();
    let bare = env.origin("proj", &[]);
    let url = env.origin_url(&bare);

    env.cmd(&["clone", &url]).assert().success();
    env.cmd(&["clone", &url])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn clone_cleans_up_partial_dir_on_failure() {
    let env = Env::new();
    env.cmd(&["clone", "file:///nonexistent/does/not/exist.git", "broken"])
        .assert()
        .failure();
    assert!(!env.root_path().join("repos/broken").exists());
}

#[test]
fn clone_json_shape() {
    let env = Env::new();
    let bare = env.origin("proj", &[]);
    let url = env.origin_url(&bare);

    let output = env
        .cmd(&["clone", &url, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["repo"], "proj");
    assert_eq!(value["url"], url);
    assert!(value["path"].as_str().unwrap().ends_with("repos/proj"));
}
