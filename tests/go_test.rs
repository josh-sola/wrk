mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use assert_cmd::prelude::*;
use common::Env;
use predicates::prelude::*;

fn clone_default(env: &Env) -> String {
    let bare = env.origin("proj", &[]);
    let url = env.origin_url(&bare);
    env.cmd(&["clone", &url]).assert().success();
    "proj".to_string()
}

/// Writes an executable script that records its cwd, its `WRK_*` env, and
/// its args into files under the returned directory, and points a `fake`
/// harness at it via `config.toml`.
fn setup_fake_harness(env: &Env) -> PathBuf {
    let out_dir = env.work.path().join("harness-out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let script_path = env.work.path().join("fake-harness.sh");
    std::fs::write(
        &script_path,
        format!(
            "#!/bin/sh\npwd > {out}/cwd\nenv | grep '^WRK_' | sort > {out}/env\nprintf '%s\\n' \"$@\" > {out}/args\n",
            out = out_dir.display()
        ),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script_path, perms).unwrap();

    std::fs::write(
        env.root_path().join("config.toml"),
        format!(
            "[harnesses.fake]\ncommand = [\"{}\"]\n",
            script_path.display()
        ),
    )
    .unwrap();

    out_dir
}

#[test]
fn go_creates_a_missing_tree_and_execs_the_harness_in_it() {
    let env = Env::new();
    let repo = clone_default(&env);
    let out = setup_fake_harness(&env);

    env.cmd(&["go", &repo, "t1", "fake", "--", "--foo", "bar"])
        .assert()
        .success();

    let cwd = std::fs::read_to_string(out.join("cwd")).unwrap();
    let tree_path = env.tree_path(&repo, "t1");
    assert_eq!(
        std::fs::canonicalize(cwd.trim()).unwrap(),
        std::fs::canonicalize(&tree_path).unwrap()
    );

    let recorded_env = std::fs::read_to_string(out.join("env")).unwrap();
    assert!(recorded_env.contains(&format!("WRK_ROOT={}", env.root_path().display())));
    assert!(recorded_env.contains(&format!("WRK_REPO={repo}")));
    assert!(recorded_env.contains("WRK_TREE=t1"));
    assert!(recorded_env.contains(&format!("WRK_TREE_PATH={}", tree_path.display())));
    assert!(recorded_env.contains("WRK_BRANCH=t1"));

    let args = std::fs::read_to_string(out.join("args")).unwrap();
    assert_eq!(args.trim(), "--foo\nbar");
}

#[test]
fn go_reuses_an_existing_tree_via_a_unique_prefix() {
    let env = Env::new();
    let repo = clone_default(&env);
    let out = setup_fake_harness(&env);
    env.cmd(&["new", &repo, "feature-one", "--wait"])
        .assert()
        .success();

    env.cmd(&["go", &repo, "feature", "fake"])
        .assert()
        .success();

    let cwd = std::fs::read_to_string(out.join("cwd")).unwrap();
    let tree_path = env.tree_path(&repo, "feature-one");
    assert_eq!(
        std::fs::canonicalize(cwd.trim()).unwrap(),
        std::fs::canonicalize(&tree_path).unwrap()
    );
}

#[test]
fn go_ambiguous_tree_prefix_errors_and_creates_nothing() {
    let env = Env::new();
    let repo = clone_default(&env);
    let out = setup_fake_harness(&env);
    env.cmd(&["new", &repo, "feature-one", "--wait"])
        .assert()
        .success();
    env.cmd(&["new", &repo, "feature-two", "--wait"])
        .assert()
        .success();

    env.cmd(&["go", &repo, "feature", "fake"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("ambiguous"));

    assert!(!env.tree_path(&repo, "feature").exists());
    assert!(!out.join("cwd").exists());
}

#[test]
fn go_new_flag_creates_even_when_a_prefix_would_match() {
    let env = Env::new();
    let repo = clone_default(&env);
    let out = setup_fake_harness(&env);
    env.cmd(&["new", &repo, "feature-one", "--wait"])
        .assert()
        .success();

    env.cmd(&["go", &repo, "feature", "fake", "--new"])
        .assert()
        .success();

    assert!(env.tree_path(&repo, "feature").exists());
    let cwd = std::fs::read_to_string(out.join("cwd")).unwrap();
    assert_eq!(
        std::fs::canonicalize(cwd.trim()).unwrap(),
        std::fs::canonicalize(env.tree_path(&repo, "feature")).unwrap()
    );
}

#[test]
fn go_resolves_the_harness_by_prefix() {
    let env = Env::new();
    let repo = clone_default(&env);
    let out = setup_fake_harness(&env);

    env.cmd(&["go", &repo, "t1", "fa"]).assert().success();

    assert!(out.join("cwd").exists());
}

#[test]
fn go_unknown_harness_errors_and_creates_no_tree() {
    let env = Env::new();
    let repo = clone_default(&env);
    setup_fake_harness(&env);

    env.cmd(&["go", &repo, "t1", "nope"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("no harness"));

    assert!(!env.tree_path(&repo, "t1").exists());
}

#[test]
fn go_hook_failure_blocks_the_harness_and_exits_one() {
    let env = Env::new();
    let repo = clone_default(&env);
    let out = setup_fake_harness(&env);
    env.write_hook(&repo, "on_create", "#!/bin/sh\nexit 3\n");

    env.cmd(&["go", &repo, "t1", "fake"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("log at"));

    assert!(!out.join("cwd").exists());
}

#[test]
fn go_with_one_positional_is_a_usage_error() {
    let env = Env::new();

    env.cmd(&["go", "repo-only"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("all three"));
}

#[test]
fn go_with_two_positionals_is_a_usage_error() {
    let env = Env::new();

    env.cmd(&["go", "repo", "tree"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("all three"));
}

#[test]
fn go_with_no_args_and_no_terminal_fails_with_a_usage_error() {
    let env = Env::new();

    env.cmd(&["go"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("needs a terminal"));
}

#[test]
fn go_new_without_positionals_is_a_usage_error() {
    let env = Env::new();
    env.cmd(&["go", "--new"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--new needs"));
}
