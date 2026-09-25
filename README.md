# wrk

`wrk` manages git worktrees. It owns its repos, runs an optional hook when a tree is created or removed, and launches an agent harness inside a tree.

## Install

```sh
cargo install --path .
```

## Layout

Everything lives under `$WRK_ROOT`, which defaults to `~/Library/wrk`:

```
repos/<repo>/              clones (no checkout, detached HEAD)
trees/<repo>/<tree>/       worktrees; branch <tree>
hooks/<repo>/on_create     optional executable, runs in the background after `new`
hooks/<repo>/on_destroy    optional executable, runs in the foreground during `rm`
config.toml                optional harness config
state/                     hook status, locks, and logs
```

Hooks run with the tree as their working directory. They get `WRK_ROOT`, `WRK_REPO`, `WRK_TREE`, `WRK_TREE_PATH`, `WRK_REPO_PATH`, and `WRK_BRANCH`.

## Commands

```sh
wrk clone <url> [repo]
wrk new <repo> <tree> [--wait]
wrk rm <repo> <tree> [--force]
wrk wait <repo> <tree> [--timeout SECS]
wrk go [<repo> <tree> <harness>] [--new] [--wait] [-- <harness args>]
wrk pick [--json]
wrk ls [repo]
wrk logs <repo> <tree> [-f]
wrk path <repo> [tree]
```

- Any `<repo>`, `<tree>`, or `<harness>` argument accepts a unique prefix. An ambiguous prefix is an error that lists the matches.
- `new` checks out branch `<tree>`. It uses the local branch if one exists, then `origin/<tree>`, and otherwise branches from origin's default branch.
- `go` opens an existing tree that matches, or creates one when nothing matches. `--new` always creates a tree. `go` launches the harness in the tree right away, even while `on_create` is still running, and warns you if the hook is running or failed. With `--wait` it waits for `on_create` first and doesn't launch if the hook failed. With no arguments it opens a picker.
- `pick` opens the same picker but only prints the selection: the tree path, plus the `wrk go` command to open it with `--json`. It changes nothing, so a tool that places sessions itself can run the command wherever it wants. The picker draws on `/dev/tty`, so `$(wrk pick --json)` captures only the JSON.
- `rm` refuses a tree with uncommitted changes or a running hook unless you pass `--force`. It keeps the branch.
- `wait` exits 0 when the hook succeeded or there was none, 1 when it failed, and 2 when it crashed or timed out.
- Every command except `go` takes `--json` and prints one JSON document. Errors print as `{"error":{"code","message"}}`.

## Harnesses

`claude`, `codex`, `pi`, and `devin` are built in. Add or override harnesses in `config.toml`:

```toml
default_harness = "claude"

[harnesses.claude]
command = ["claude", "--dangerously-skip-permissions"]
```
