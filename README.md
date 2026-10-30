# wrk

`wrk` manages git worktrees. It owns its repos, runs an optional hook when a tree is created or removed, and launches an agent harness inside a tree.

## Install

Download the latest macOS build:

```sh
mkdir -p ~/.local/bin
curl -fsSL https://github.com/josh-sola/wrk/releases/latest/download/wrk-macos.tar.gz | tar -xz -C ~/.local/bin
```

Or build from source with `cargo install --path .`

To cut a release, bump `version` in `Cargo.toml` and push to `main`. The `release` workflow sees the new version, then checks, builds, tags, and publishes it. Pushes that don't change the version skip the build.

To set up the Sola `monorepo` and `helm` repos with ready-made hooks, follow `onboarding/README.md`.

`skills/wrk/SKILL.md` is an agent skill that teaches an agent to use `wrk`. Copy the `skills/wrk` folder into `~/.agents/skills/` for Codex and Pi, and link it into `~/.claude/skills/` for Claude. `onboarding/README.md` step 6 has the commands.

## Layout

Everything lives under `$WRK_ROOT`, which defaults to `~/Library/wrk`:

```
repos/<repo>/              clones (no checkout, detached HEAD)
trees/<repo>/<tree>/       worktrees; branch <tree> unless --branch says otherwise
hooks/<repo>/on_create     optional executable, runs in the background after `new`
hooks/<repo>/on_destroy    optional executable, runs in the foreground during `rm`
config.toml                optional harness config
state/                     hook status, locks, and logs
```

Hooks run with the tree as their working directory. They get `WRK_ROOT`, `WRK_REPO`, `WRK_TREE`, `WRK_TREE_PATH`, `WRK_REPO_PATH`, and `WRK_BRANCH`.

## Commands

```sh
wrk clone <url> [repo]
wrk new <repo> <tree> [--branch NAME] [--track] [--wait]
wrk rm <repo> <tree> [--force]
wrk wait <repo> <tree> [--timeout SECS]
wrk go [<repo> <tree> <harness>] [--new] [--wait] [--branch NAME] [--track] [--fill] [-- <harness args>]
wrk pick [--json] [--fill]
wrk ls [repo]
wrk logs <repo> <tree> [-f]
wrk path <repo> [tree]
```

- Any `<repo>`, `<tree>`, or `<harness>` argument accepts a unique prefix. An ambiguous prefix is an error that lists the matches.
- `new` checks out branch `<tree>`, or the branch named by `--branch`. The tree folder keeps the tree name. It reuses a local branch of that name if the branch has commits that origin's default branch lacks. A local branch with no commits of its own is deleted and replaced, and `new` says so. Otherwise it creates a fresh branch from origin's default branch, even when `origin/<branch>` exists, because common names often match someone's stale branch. It tells you when it skipped one. Pass `--track` to check out `origin/<branch>` instead. `go --branch` and `go --track` do the same when `go` creates a tree. `WRK_BRANCH` in hooks is the branch the tree is on.
- `go` opens an existing tree that matches, or creates one when nothing matches. `--new` always creates a tree. `go` launches the harness in the tree right away, even while `on_create` is still running, and warns you if the hook is running or failed. With `--wait` it waits for `on_create` first and doesn't launch if the hook failed. With no arguments it opens a picker. The picker draws a centered panel. `--fill` makes it use the whole terminal, for hosts such as herdr popups that already pad it.
- `pick` opens the same picker but only prints the selection: the tree path, plus the `wrk go` command to open it with `--json`. It changes nothing, so a tool that places sessions itself can run the command wherever it wants. The picker draws on `/dev/tty`, so `$(wrk pick --json)` captures only the JSON.
- `rm` refuses a tree with uncommitted changes or a running hook unless you pass `--force`. It keeps the branch.
- `wait` exits 0 when the hook succeeded or there was none, 1 when it failed, and 2 when it crashed or timed out.
- Every command except `go` takes `--json` and prints one JSON document. Errors print as `{"error":{"code","message"}}`.

## Harnesses

`claude`, `codex`, `pi`, and `devin` are built in. `claude` and `pi` get `--name <tree>` by default, so the session is named after the tree. Add or override harnesses in `config.toml`:

```toml
default_harness = "claude"

[harnesses.claude]
command = ["claude", "--dangerously-skip-permissions"]
```

`wrk go` replaces `{tree}` in any command arg with the tree name. An override replaces the whole built-in command, so keep `--name` in it if you want the session named:

```toml
[harnesses.claude]
command = ["claude", "--effort", "high", "--name", "{tree}"]
```

## herdr

`herdr-plugin/` lets [herdr](https://herdr.dev) open `wrk pick`'s selection in its own workspace and tab instead of the calling terminal. Link it with:

```sh
herdr plugin link <repo>/herdr-plugin
```

Then bind a key to it in herdr's config:

```toml
[[keys.command]]
key = "prefix+w"
type = "plugin_action"
command = "dev.wrk.pick"
```

See `herdr-plugin/open.sh` for the placement logic and `herdr-plugin/test.sh` for its offline tests.

## Rex

`rex-plugin/` adds a new-session composer to Rex. It holds `rw`, a script that opens a worktree as a Rex session, `rw.lua`, which binds its Rex actions, and `rw-launch`, the composer popup. Install all three with:

```sh
rex-plugin/install.sh
```

`install.sh` copies `rw` and `rw.lua` rather than linking them, because trees get deleted. It builds `rw-launch` with `cargo install`, then reloads Rex config.

A new tree's session opens on the agent tab, and the agent starts at once. A second `setup` tab follows the `on_create` log. It closes when the hook succeeds, and stays open as a shell if the hook fails.

Each Rex key is `alt+space` followed by one letter: `a` adds an agent or shell block to the session, `e` opens helix in its tree, and `w` closes the session and removes its tree. Pass `keys` to `setup` in `init.lua` to change them.

`alt+space` then `n` opens the composer. Type a ticket ID, a title, or a plain name. The first row always creates a tree with exactly the name you typed. Pick a ticket row to use Linear's branch name instead. The tree keeps the typed or ticket-derived name, and `rw` passes the branch to `wrk new --branch`, adding `--track` when `origin` already has that branch.

- `enter` starts the session.
- `↑` `↓` pick a ticket row.
- `^P` picks the repo and `^O` picks the agent (claude, codex or devin). Each opens a dropdown under its chip. In the repo dropdown, type to filter. The repo list comes from `wrk ls`, with `monorepo` first.
- `^B` edits the branch line, which is also the tree name. Once you edit it, it stops following the selection.
- You can also use the mouse: click a chip to open its dropdown, click an item to pick it, click a row to select it and click it again to start it, and click the branch line to edit it. The wheel scrolls.
- `esc` closes a menu, ends a branch edit, or closes the composer.

The composer remembers your last repo and agent, and caches your tickets so it draws at once. It hands each launch to `rw new`.
