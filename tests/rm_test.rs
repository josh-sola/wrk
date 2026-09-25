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
fn rm_removes_a_clean_tree_and_keeps_the_branch() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();

    env.cmd(&["rm", &repo, "t1"]).assert().success();
    assert!(!env.tree_path(&repo, "t1").exists());

    let branches = std::process::Command::new("git")
        .args(["branch", "--list", "t1"])
        .current_dir(env.root_path().join("repos").join(&repo))
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&branches.stdout).contains("t1"));
}

#[test]
fn rm_refuses_a_dirty_tree() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();
    std::fs::write(env.tree_path(&repo, "t1").join("dirty.txt"), "x").unwrap();

    env.cmd(&["rm", &repo, "t1"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("uncommitted"));
    assert!(env.tree_path(&repo, "t1").exists());
}

#[test]
fn rm_refuses_while_hook_is_running() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.write_hook(&repo, "on_create", "#!/bin/sh\nsleep 5\nexit 0\n");
    env.cmd(&["new", &repo, "t1"]).assert().success();
    common::wait_for_status(&env, &repo, "t1", "running", Duration::from_secs(5));

    env.cmd(&["rm", &repo, "t1", "--json"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("hook_running"));
    assert!(env.tree_path(&repo, "t1").exists());
}

#[test]
fn rm_force_kills_the_running_hook_and_removes_the_tree() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.write_hook(&repo, "on_create", "#!/bin/sh\nsleep 30\nexit 0\n");
    env.cmd(&["new", &repo, "t1"]).assert().success();
    common::wait_for_status(&env, &repo, "t1", "running", Duration::from_secs(5));

    env.cmd(&["rm", &repo, "t1", "--force"]).assert().success();
    assert!(!env.tree_path(&repo, "t1").exists());
}

#[test]
fn rm_aborts_when_on_destroy_fails() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();
    env.write_hook(&repo, "on_destroy", "#!/bin/sh\necho boom >&2\nexit 3\n");

    env.cmd(&["rm", &repo, "t1", "--json"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("hook_failed"));
    assert!(env.tree_path(&repo, "t1").exists());
}

#[test]
fn rm_runs_on_destroy_and_tees_its_output_to_stderr() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();
    env.write_hook(&repo, "on_destroy", "#!/bin/sh\necho destroying\nexit 0\n");

    env.cmd(&["rm", &repo, "t1"])
        .assert()
        .success()
        .stderr(predicate::str::contains("destroying"));
    assert!(!env.tree_path(&repo, "t1").exists());
}

#[test]
fn rm_refuses_a_fresh_pending_status_with_no_runner_yet() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();

    // Simulate the window between `new` spawning a runner and that runner
    // taking the tree lock: nothing holds the lock, but the status file
    // says a hook is about to run.
    env.write_status(
        &repo,
        "t1",
        &serde_json::json!({
            "hook": "on_create",
            "status": "pending",
            "created_at": now_unix(),
        }),
    );

    env.cmd(&["rm", &repo, "t1", "--json"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("hook_running"));
    assert!(env.tree_path(&repo, "t1").exists());

    env.cmd(&["rm", &repo, "t1", "--force"]).assert().success();
    assert!(!env.tree_path(&repo, "t1").exists());
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn rm_force_overrides_dirty_tree() {
    let env = Env::new();
    let repo = clone_default(&env);
    env.cmd(&["new", &repo, "t1", "--wait"]).assert().success();
    std::fs::write(env.tree_path(&repo, "t1").join("dirty.txt"), "x").unwrap();

    env.cmd(&["rm", &repo, "t1", "--force"]).assert().success();
    assert!(!env.tree_path(&repo, "t1").exists());
}
