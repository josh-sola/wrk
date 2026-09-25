mod common;

use assert_cmd::prelude::*;
use common::Env;
use predicates::prelude::*;

fn clone_named(env: &Env, name: &str) {
    let bare = env.origin(name, &[]);
    let url = env.origin_url(&bare);
    env.cmd(&["clone", &url, name]).assert().success();
}

#[test]
fn resolve_exact_match_wins() {
    let env = Env::new();
    clone_named(&env, "alpha");
    clone_named(&env, "alphabet");

    env.cmd(&["path", "alpha"]).assert().success().stdout(
        predicate::str::contains("repos/alpha")
            .and(predicate::str::contains("repos/alphabet").not()),
    );
}

#[test]
fn resolve_unique_prefix() {
    let env = Env::new();
    clone_named(&env, "alpha");
    clone_named(&env, "beta");

    env.cmd(&["path", "al"])
        .assert()
        .success()
        .stdout(predicate::str::contains("repos/alpha"));
}

#[test]
fn resolve_ambiguous_prefix_reports_candidates() {
    let env = Env::new();
    clone_named(&env, "alpha");
    clone_named(&env, "alphabet");

    let output = env
        .cmd(&["path", "al", "--json"])
        .assert()
        .failure()
        .code(1);
    let value: serde_json::Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(value["error"]["code"], "ambiguous_prefix");
    assert_eq!(
        value["error"]["candidates"],
        serde_json::json!(["alpha", "alphabet"])
    );
}

#[test]
fn resolve_no_match_reports_not_found() {
    let env = Env::new();
    clone_named(&env, "alpha");

    let output = env
        .cmd(&["path", "zzz", "--json"])
        .assert()
        .failure()
        .code(1);
    let value: serde_json::Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(value["error"]["code"], "not_found");
}
