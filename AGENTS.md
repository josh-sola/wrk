# AGENTS.md

`wrk` is a Rust CLI that manages git worktrees. The README covers what it does for users. This file covers how to work on it.

## Commands

Run all three before you call a change done:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

To install your build: `cargo install --path .`

To try the picker with fake data: `cargo run --example pick_demo`. It needs a real terminal.

## Layout

- `src/main.rs` holds only the clap definitions and dispatch. Command logic lives in `src/cmd/<command>.rs`.
- `src/paths.rs` is the only place that builds a path. Add a method to `Paths` rather than joining strings anywhere else.
- `src/resolve.rs` owns all name matching: exact match, then unique prefix, then an error. Never pick silently between ambiguous matches.
- `src/state.rs` owns status files, flocks, and logs. Write status files with `write_status_atomic` only.
- `src/hooks.rs` spawns and runs hooks. `src/harness.rs` and `src/config.rs` launch harnesses.
- `src/tui/` is the picker shared by `go` and `pick`. `state.rs` is a pure state machine with no terminal IO, `view.rs` renders it, and `mod.rs` owns the terminal. It draws to `/dev/tty` directly, never stdout, so `pick --json`'s stdout carries only its own JSON. `src/cmd/go.rs::pick_input` builds its `PickInput` from `Paths`; both commands call it the same way.
- `src/output.rs` holds `WrkError`. Its `code()` strings are a stable interface for agents, so add a variant rather than changing an existing code.

## Rules to keep

- **The filesystem is the source of truth.** A repo or tree exists if its folder exists. Don't add a registry.
- **Liveness is the flock, not a PID.** The hook runner holds an exclusive lock on `state/trees/<repo>/<tree>.lock` for its whole life. A `running` status with a free lock means the runner crashed. Don't add PID liveness checks.
- **The runner is double-forked** so it is never a child of `wrk`. `go` execs the harness in its own place, and the harness would never reap the runner. Keep `spawn_runner` that way.
- **`go` has one create → exec path, with an optional wait** (the private `go` function in `src/cmd/go.rs`). The picker returns a `GoTarget` and hands off to it. Don't add a second launch path. `pick` returns the same `GoTarget` but never hands off to it — it only prints the selection.
- **JSON:** each command with `--json` prints exactly one document on stdout, with snake_case keys. Errors in JSON mode also go to stdout. Progress lines always go to stderr. `go` has no `--json`.
- **Out of scope:** submodule handling, Graphite, and terminal or window-manager integrations. Hooks cover provisioning.

## Tests

Integration tests live in `tests/` and use `tests/common/mod.rs`. Each test gets its own `WRK_ROOT` and `HOME`, and git config is isolated. Remotes are local bare repos reached via `file://`. Keep tests hermetic: no network, no real `~/Library/wrk`, no global git config.

Wait on background work with `poll_until` or `wait_for_status` from `tests/common`, never a fixed sleep.
