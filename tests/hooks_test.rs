mod common;

use std::time::Duration;

use assert_cmd::prelude::*;
use common::Env;
use predicates::prelude::*;

fn clone_default(env: &Env) -> String {
    let bare = env.origin("proj", &[]);
    let url = env.origin_url(&bare);
    env.cmd(&["clone", &url]).assert().success();
    "proj".to_string()
}

#[test]
fn on_create_absent_reports_none() {
    let env = Env::new();
    let repo = clone_default(&env);

    let output = env
        .cmd(&["new", &repo, "t1", "--wait", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["hook_status"], "none");
    assert_eq!(value["outcome"], "none");
}

#[test]
fn on_create_success() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.write_hook(&repo, "on_create", "#!/bin/sh\necho hi\nexit 0\n");

    let output = env
        .cmd(&["new", &repo, "t1", "--wait", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["outcome"], "succeeded");

    let log = std::fs::read_to_string(env.log_path(&repo, "t1")).unwrap();
    assert!(log.contains("hi"));
}

#[test]
fn on_create_failure() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.write_hook(&repo, "on_create", "#!/bin/sh\nexit 7\n");

    env.cmd(&["new", &repo, "t1", "--wait", "--json"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("\"outcome\":\"failed\""));
}

#[test]
fn on_create_not_executable_fails_but_keeps_the_tree() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.write_non_executable_hook(&repo, "on_create", "#!/bin/sh\nexit 0\n");

    env.cmd(&["new", &repo, "t1", "--json"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("hook_not_executable"));

    assert!(env.tree_path(&repo, "t1").exists());
    let status = env.read_status(&repo, "t1").unwrap();
    assert_eq!(status["status"], "failed");
}

#[test]
fn on_create_env_and_cwd() {
    let env = Env::new();
    let repo = clone_default(&env);
    let out_dir = env.work.path().join("hookout");
    std::fs::create_dir_all(&out_dir).unwrap();
    env.write_hook(
        &repo,
        "on_create",
        "#!/bin/sh\nenv > \"$HOOK_OUT/env.txt\"\npwd > \"$HOOK_OUT/cwd.txt\"\n",
    );

    let mut cmd = env.cmd(&["new", &repo, "t1", "--wait"]);
    cmd.env("HOOK_OUT", &out_dir);
    cmd.assert().success();

    let dumped_env = std::fs::read_to_string(out_dir.join("env.txt")).unwrap();
    let tree_path = env.tree_path(&repo, "t1");
    let repo_path = env.root_path().join("repos").join(&repo);

    assert!(dumped_env.contains(&format!("WRK_ROOT={}", env.root_path().display())));
    assert!(dumped_env.contains(&format!("WRK_REPO={repo}")));
    assert!(dumped_env.contains("WRK_TREE=t1"));
    assert!(dumped_env.contains(&format!("WRK_TREE_PATH={}", tree_path.display())));
    assert!(dumped_env.contains(&format!("WRK_REPO_PATH={}", repo_path.display())));
    assert!(dumped_env.contains("WRK_BRANCH=t1"));

    let dumped_cwd = std::fs::read_to_string(out_dir.join("cwd.txt")).unwrap();
    assert_eq!(
        std::fs::canonicalize(dumped_cwd.trim()).unwrap(),
        std::fs::canonicalize(&tree_path).unwrap()
    );
}

#[test]
fn crash_is_detected_when_the_runner_dies() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.write_hook(&repo, "on_create", "#!/bin/sh\nsleep 5\nexit 0\n");

    env.cmd(&["new", &repo, "t1"]).assert().success();

    common::wait_for_status(&env, &repo, "t1", "running", Duration::from_secs(5));
    let status = env.read_status(&repo, "t1").unwrap();
    let pid = status["pid"].as_u64().unwrap();

    let killed = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .unwrap();
    assert!(killed.success());

    let output = env
        .cmd(&["wait", &repo, "t1", "--json"])
        .assert()
        .failure()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["outcome"], "crashed");
}

#[test]
fn a_late_runner_is_a_no_op_once_the_status_moved_on() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.write_hook(&repo, "on_create", "#!/bin/sh\necho ran\nexit 0\n");
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();

    let log_before = std::fs::read_to_string(env.log_path(&repo, "t1")).unwrap();
    assert_eq!(log_before.matches("ran").count(), 1);

    // A duplicate or delayed runner invocation for the same hook must not
    // re-run it or touch its terminal status.
    env.cmd(&["internal", "run-hook", &repo, "t1", "on_create"])
        .assert()
        .success();

    let log_after = std::fs::read_to_string(env.log_path(&repo, "t1")).unwrap();
    assert_eq!(log_after, log_before);
    let status = env.read_status(&repo, "t1").unwrap();
    assert_eq!(status["status"], "succeeded");
}

#[test]
fn a_late_runner_is_a_no_op_once_the_tree_is_gone() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.cmd(&["new", &repo, "t1"]).assert().success();

    env.write_status(
        &repo,
        "t1",
        &serde_json::json!({"hook": "on_create", "status": "pending", "created_at": 0}),
    );
    std::fs::remove_dir_all(env.tree_path(&repo, "t1")).unwrap();

    env.cmd(&["internal", "run-hook", &repo, "t1", "on_create"])
        .assert()
        .success();

    assert!(!env.log_path(&repo, "t1").exists());
    let status = env.read_status(&repo, "t1").unwrap();
    assert_eq!(status["status"], "pending");
}

#[test]
fn new_wait_json_reports_the_final_hook_status() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.write_hook(&repo, "on_create", "#!/bin/sh\nexit 0\n");

    let out = env
        .cmd(&["new", &repo, "t1", "--wait", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["hook_status"], "succeeded");
    assert_eq!(value["outcome"], "succeeded");
}
