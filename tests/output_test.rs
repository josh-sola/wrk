mod common;

use assert_cmd::prelude::*;
use common::Env;
use predicates::prelude::*;

fn clone_named(env: &Env, name: &str) -> String {
    let bare = env.origin(name, &[]);
    let url = env.origin_url(&bare);
    env.cmd(&["clone", &url, name]).assert().success();
    name.to_string()
}

#[test]
fn ls_json_shape() {
    let env = Env::new();
    let repo = clone_named(&env, "proj");
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();

    let output = env
        .cmd(&["ls", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let repos = value["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0]["name"], "proj");
    let trees = repos[0]["trees"].as_array().unwrap();
    assert_eq!(trees.len(), 1);
    assert_eq!(trees[0]["name"], "t1");
    assert_eq!(trees[0]["branch"], "t1");
    assert_eq!(trees[0]["status"], "none");
    assert!(
        trees[0]["path"]
            .as_str()
            .unwrap()
            .ends_with("trees/proj/t1")
    );
}

#[test]
fn ls_human_table_has_headers_and_rows() {
    let env = Env::new();
    let repo = clone_named(&env, "proj");
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();

    env.cmd(&["ls"])
        .assert()
        .success()
        .stdout(predicate::str::contains("REPO").and(predicate::str::contains("t1")));
}

#[test]
fn ls_human_lists_repos_that_have_no_trees() {
    let env = Env::new();
    clone_named(&env, "alpha");
    let repo = clone_named(&env, "beta");
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();

    let output = env
        .cmd(&["ls"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let alpha = text.lines().find(|l| l.starts_with("alpha")).unwrap();
    assert_eq!(
        alpha.split_whitespace().collect::<Vec<_>>(),
        ["alpha", "-", "-", "-"]
    );
    assert!(
        text.lines()
            .any(|l| l.starts_with("beta") && l.contains("t1"))
    );
}

#[test]
fn ls_can_be_scoped_to_one_repo_by_prefix() {
    let env = Env::new();
    clone_named(&env, "alpha");
    clone_named(&env, "beta");

    let output = env
        .cmd(&["ls", "al", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let repos = value["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0]["name"], "alpha");
}

#[test]
fn logs_prints_hook_output() {
    let env = Env::new();
    let repo = clone_named(&env, "proj");
    env.write_hook(&repo, "on_create", "#!/bin/sh\necho hello-from-hook\n");
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();

    env.cmd(&["logs", &repo, "t1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello-from-hook"));
}

#[test]
fn logs_json_shape() {
    let env = Env::new();
    let repo = clone_named(&env, "proj");
    env.write_hook(&repo, "on_create", "#!/bin/sh\necho hi\n");
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();

    let output = env
        .cmd(&["logs", &repo, "t1", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(value["path"].as_str().unwrap().ends_with("t1.log"));
    assert!(value["content"].as_str().unwrap().contains("hi"));
}

#[test]
fn logs_json_and_follow_together_is_a_usage_error() {
    let env = Env::new();
    let repo = clone_named(&env, "proj");
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();

    env.cmd(&["logs", &repo, "t1", "--json", "-f"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("usage"));
}

#[test]
fn logs_follow_exits_once_the_hook_finishes() {
    let env = Env::new();
    let repo = clone_named(&env, "proj");
    env.write_hook(
        &repo,
        "on_create",
        "#!/bin/sh\necho line-one\nsleep 0.2\necho line-two\n",
    );
    env.cmd(&["new", &repo, "t1"]).assert().success();

    env.cmd(&["logs", &repo, "t1", "-f"])
        .assert()
        .success()
        .stdout(predicate::str::contains("line-one").and(predicate::str::contains("line-two")));
}

#[test]
fn path_prints_repo_path_without_a_tree() {
    let env = Env::new();
    let repo = clone_named(&env, "proj");

    env.cmd(&["path", &repo]).assert().success().stdout(
        predicate::str::contains("repos/proj").and(predicate::str::contains("trees").not()),
    );
}

#[test]
fn path_prints_tree_path_with_a_tree() {
    let env = Env::new();
    let repo = clone_named(&env, "proj");
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();

    env.cmd(&["path", &repo, "t1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("trees/proj/t1"));
}

#[test]
fn path_json_shape() {
    let env = Env::new();
    let repo = clone_named(&env, "proj");

    let output = env
        .cmd(&["path", &repo, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(value["path"].as_str().unwrap().ends_with("repos/proj"));
}

#[test]
fn rm_json_shape() {
    let env = Env::new();
    let repo = clone_named(&env, "proj");
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();

    let output = env
        .cmd(&["rm", &repo, "t1", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["repo"], "proj");
    assert_eq!(value["tree"], "t1");
    assert_eq!(value["removed"], true);
}
