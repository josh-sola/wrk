mod common;

use assert_cmd::prelude::*;
use common::Env;
use predicates::prelude::*;

/// Whether *this* process can open `/dev/tty`. A child spawned by
/// `assert_cmd` shares our session (nothing calls `setsid`), so this
/// predicts what the child will see without ever spawning it.
fn dev_tty_is_available() -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .is_ok()
}

#[test]
fn pick_json_without_a_tty_fails_cleanly() {
    if dev_tty_is_available() {
        eprintln!(
            "skipping pick_json_without_a_tty_fails_cleanly: /dev/tty is reachable in this test environment"
        );
        return;
    }

    let env = Env::new();
    env.cmd(&["pick", "--json"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("\"code\":\"usage\""))
        .stdout(predicate::str::contains("wrk pick needs a terminal"));
}

#[test]
fn pick_without_a_tty_fails_cleanly_in_human_mode() {
    if dev_tty_is_available() {
        eprintln!(
            "skipping pick_without_a_tty_fails_cleanly_in_human_mode: /dev/tty is reachable in this test environment"
        );
        return;
    }

    let env = Env::new();
    env.cmd(&["pick"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("wrk pick needs a terminal"));
}
