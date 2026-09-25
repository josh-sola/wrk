mod common;

use assert_cmd::prelude::*;
use common::Env;

fn clone_default(env: &Env) -> String {
    let bare = env.origin("proj", &[]);
    let url = env.origin_url(&bare);
    env.cmd(&["clone", &url]).assert().success();
    "proj".to_string()
}

#[test]
fn wait_times_out_on_a_slow_hook() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.write_hook(&repo, "on_create", "#!/bin/sh\nsleep 30\nexit 0\n");
    env.cmd(&["new", &repo, "t1"]).assert().success();

    let output = env
        .cmd(&["wait", &repo, "t1", "--timeout", "1", "--json"])
        .assert()
        .failure()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["outcome"], "timed_out");
    assert!(value["log_path"].as_str().unwrap().ends_with("t1.log"));
}

#[test]
fn wait_on_a_hookless_tree_exits_zero() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.cmd(&["new", &repo, "t1"]).assert().success();

    env.cmd(&["wait", &repo, "t1"]).assert().success();
}

#[test]
fn wait_on_a_failed_hook_exits_one() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.write_hook(&repo, "on_create", "#!/bin/sh\nexit 9\n");
    env.cmd(&["new", &repo, "t1"]).assert().success();

    env.cmd(&["wait", &repo, "t1"]).assert().failure().code(1);
}

#[test]
fn wait_resolves_repo_and_tree_by_prefix() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.cmd(&["new", &repo, "t1"]).assert().success();

    env.cmd(&["wait", &repo[..2], "t"]).assert().success();
}
