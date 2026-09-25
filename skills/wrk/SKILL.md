---
name: wrk
description: Use when you need a git worktree for a repo that wrk manages (check with `wrk ls`), such as monorepo or helm. Covers creating a tree, waiting for its setup hook, finding a tree's path, reading hook logs, and removing a tree. Read before running `git worktree add` or cloning a repo by hand.
---

# wrk

`wrk` manages git worktrees. Each repo it manages has one clone and many trees. Each tree is a worktree on its own branch. A hook sets up each new tree in the background: installs, builds, and env files.

## Where things live

Everything is under `$WRK_ROOT`, which defaults to `~/Library/wrk`:

```
repos/<repo>/          the clone; never work here
trees/<repo>/<tree>/   one worktree per tree, on branch <tree>
hooks/<repo>/          on_create and on_destroy hooks
state/trees/<repo>/    hook status and logs
```

If your working directory is under `trees/<repo>/<tree>/`, you are in a wrk tree, and those two path parts are its repo and tree names.

## Commands for agents

Pass `--json` to get one JSON document on stdout. Progress lines go to stderr.

| Command | Use |
| --- | --- |
| `wrk ls [repo] --json` | List repos and their trees: name, path, branch, and hook status. |
| `wrk new <repo> <tree> --json` | Create a tree on branch `<tree>`. Returns before the hook finishes. |
| `wrk new <repo> <tree> --wait --json` | Create a tree and wait for its hook. |
| `wrk wait <repo> <tree> [--timeout SECS] --json` | Wait for a tree's hook to finish. |
| `wrk path <repo> [tree] --json` | Print the path of a repo's clone or a tree. |
| `wrk logs <repo> <tree>` | Print the hook's log. |
| `wrk rm <repo> <tree> --json` | Remove a tree. Keeps its branch. |
| `wrk clone <url> [repo] --json` | Add a repo to wrk. |

Every `<repo>` and `<tree>` accepts a unique prefix. An ambiguous prefix is an error that lists the matches. Pass the full name when you mean one tree.

## Do not run `wrk go` or `wrk pick`

Both are for people at a terminal. `wrk go` replaces its own process with an interactive agent session. `wrk pick` opens a picker that waits for keys. Use `wrk new`, `wrk path`, and `wrk ls` instead.

## Create a tree and work in it

```sh
wrk new monorepo fix-login --wait --json
cd "$(wrk path monorepo fix-login --json | jq -r .path)"
```

- **Branches:** `new` creates branch `<tree>` from origin's default branch. If a local branch of that name exists, it reuses it. To check out an existing remote branch instead, pass `--track`. Without `--track`, `new` never checks out `origin/<tree>`, even when it exists.
- **Names:** The tree name is also the branch name, so pick a short, specific name.
- **Setup:** Without `--wait`, the tree exists at once but dependencies may still be installing. Run `wrk wait` before you build or test.

## Hook results

`wrk wait` and `wrk new --wait` exit with:

- `0`: the hook succeeded, or the repo has no hook.
- `1`: the hook failed. Run `wrk logs <repo> <tree>` to see why.
- `2`: the hook crashed or the wait timed out.

The JSON from `wait` has `outcome`, `exit_code`, and `log_path`. A failed hook leaves the tree in place. Often you can fix the cause, then run the failed step by hand inside the tree.

## Remove a tree

`wrk rm` refuses a tree with uncommitted changes or a running hook. Commit or push your work first. Use `--force` only when the user tells you to throw the changes away. The branch stays after removal.

## Errors

In JSON mode, errors print to stdout as `{"error":{"code","message"}}`. The codes are stable:

`not_found`, `ambiguous_prefix`, `already_exists`, `invalid_name`, `dirty_tree`, `hook_running`, `hook_failed`, `hook_not_executable`, `hook_crashed`, `timeout`, `config_invalid`, `harness_not_found`, `git_failed`, `io`, `usage`, `no_terminal`.

## Rules

- Never run `git worktree add` for a wrk repo. Use `wrk new`, so the hook runs and `wrk` can find the tree.
- Never edit files in `repos/<repo>/`. It is the shared clone, not a working checkout.
- Never delete a tree folder by hand. Use `wrk rm`, so the `on_destroy` hook runs.
- The folders are the source of truth. There is no registry to update.
