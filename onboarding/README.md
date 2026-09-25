# Set up wrk for the Sola repos

This folder sets up `wrk` for `monorepo` and `helm`. It is written so you can point your coding agent at it and say "follow this". Each step says how to check it worked.

When you are done you will have:

- `wrk` installed.
- `monorepo` and `helm` cloned into `wrk`.
- Hooks that set up each new tree in the background: install, build, Python envs, and `.env` files for `monorepo`.

## Before you start

You need these on your machine:

- **GitHub SSH access** to `Sola-Solutions`. Check with `ssh -T git@github.com`.
- **The monorepo toolchain**: `pnpm`, `uv`, and the AWS CLI (`aws`). You already have these if you work in `monorepo` today.
- **An AWS SSO profile for dev with read access.** The hook assumes it is called `admin-dev-readonly`. Check with `aws configure list-profiles`. If yours has another name, change it in step 3.

## 1. Install wrk

Download the latest release into `~/.local/bin`:

```sh
mkdir -p ~/.local/bin
curl -fsSL https://github.com/josh-sola/wrk/releases/latest/download/wrk-macos.tar.gz | tar -xz -C ~/.local/bin
```

The binary runs on both Apple Silicon and Intel Macs. If `~/.local/bin` is not on your `PATH`, add `export PATH="$HOME/.local/bin:$PATH"` to `~/.zshrc` and open a new shell.

Check: `wrk --help` prints the command list.

Download with `curl`, not a browser. macOS blocks unsigned binaries that a browser downloads. If you already did, run `xattr -d com.apple.quarantine ~/.local/bin/wrk`.

To build from source instead, run `cargo install --path .` from the root of this repo.

## 2. Copy in the hooks

`wrk` keeps everything under `~/Library/wrk` (or `$WRK_ROOT` if you set it). Copy the hooks from this folder into it, keeping them executable:

```sh
WRK_ROOT="${WRK_ROOT:-$HOME/Library/wrk}"
mkdir -p "$WRK_ROOT/hooks"
cp -Rp onboarding/hooks/. "$WRK_ROOT/hooks/"
```

Check: `ls -l "$WRK_ROOT/hooks/monorepo/on_create"` shows `-rwxr-xr-x`. If a hook exists but is not executable, `wrk` still creates the tree but marks its hook as failed.

The folder name under `hooks/` must match the repo name `wrk` uses. Step 4 clones the repos under the names `monorepo` and `helm`, which match.

These are copies, so you can edit them freely. To pick up later changes from this folder, copy them again.

## 3. Check the hooks fit your machine

Read `hooks/monorepo/on_create`. Change these if they don't match your setup:

- **AWS profile.** The last lines log in to `admin-dev-readonly` if needed, then run `pnpm generate:env`. Change `AWS_PROFILE` if your dev profile has another name.
- **Turbo remote cache.** `TURBO_TEAM` and `TURBO_CACHE` read CI's cache so builds start warm. That needs `turbo login` once. Without it, turbo warns and builds from scratch.

`hooks/lib.sh` holds the shared helpers. Its `link_plans` gives every tree of a repo a shared `plans/` folder that survives `wrk rm`, and hides it from git. Remove the `link_plans` line from a hook if you don't want that.

## 4. Clone the repos

```sh
wrk clone git@github.com:Sola-Solutions/monorepo.git
wrk clone git@github.com:Sola-Solutions/helm.git
```

`monorepo` is large, so its clone takes a while.

Check: `wrk ls` lists both repos.

## 5. Try it

Create a tree and wait for its hook:

```sh
wrk new monorepo onboarding-test --wait
```

The first run installs everything, so expect several minutes. If your AWS session has expired, a browser page opens near the end. Approve it and the hook goes on.

Check: the command exits 0 and `wrk ls monorepo` shows the tree. If the hook failed, `wrk logs monorepo onboarding-test` shows why.

Then remove the test tree:

```sh
wrk rm monorepo onboarding-test
```

## 6. Install the wrk skill

`skills/wrk/SKILL.md` teaches an agent to use `wrk`: create trees, wait for hooks, and clean up. Codex and Pi read skills from `~/.agents/skills`. Claude reads `~/.claude/skills`, so link it there too:

```sh
mkdir -p ~/.agents/skills ~/.claude/skills
cp -R skills/wrk ~/.agents/skills/
ln -s ../../.agents/skills/wrk ~/.claude/skills/wrk
```

Check: a new Claude, Codex, or Pi session lists `wrk` among its skills.

## Daily use

```sh
wrk go                         # pick a tree and open your agent in it
wrk go monorepo my-branch      # open a tree, creating it if needed
wrk ls                         # list repos and trees
```

See the repo's main `README.md` for every command and for harness settings.

## What the hooks do

`monorepo/on_create` runs in the new tree, in order:

1. Links the shared `plans/` folder.
2. `pnpm install --frozen-lockfile`, then `pnpm build:packages`.
3. `uv sync` for the Python projects.
4. Logs in to AWS dev if needed, then `pnpm generate:env` to write each service's `.env` file from Parameter Store.

The AWS step is last, so an unfinished login fails only that step. To retry it, run `pnpm generate:env` in the tree once you are logged in.

`helm/on_create` only links the shared `plans/` folder.
