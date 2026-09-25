#![allow(dead_code)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use assert_cmd::cargo::CommandCargoExt;
use tempfile::TempDir;

/// A hermetic `wrk` environment: its own `WRK_ROOT`, its own `HOME`, and no
/// network access to git config or a real origin.
pub struct Env {
    pub root: TempDir,
    pub home: TempDir,
    pub work: TempDir,
}

impl Env {
    pub fn new() -> Env {
        Env {
            root: TempDir::new().expect("tempdir"),
            home: TempDir::new().expect("tempdir"),
            work: TempDir::new().expect("tempdir"),
        }
    }

    pub fn root_path(&self) -> &Path {
        self.root.path()
    }

    /// A ready-to-run `wrk` command with a hermetic environment.
    pub fn cmd(&self, args: &[&str]) -> Command {
        let mut cmd = Command::cargo_bin("wrk").expect("wrk binary");
        cmd.args(args);
        self.apply_env(&mut cmd);
        cmd
    }

    pub fn apply_env(&self, cmd: &mut Command) {
        cmd.env("WRK_ROOT", self.root.path())
            .env("HOME", self.home.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com");
    }

    /// Creates a bare `origin.git` under this env's work dir, seeded with a
    /// commit on `main` and, optionally, extra branches that never get
    /// checked out (so a clone only tracks them remotely).
    pub fn origin(&self, name: &str, extra_branches: &[&str]) -> PathBuf {
        let src = self.work.path().join(format!("{name}-src"));
        std::fs::create_dir_all(&src).unwrap();
        self.git(&src, &["init", "-q", "-b", "main"]);
        std::fs::write(src.join("README.md"), "seed\n").unwrap();
        self.git(&src, &["add", "README.md"]);
        self.git(&src, &["commit", "-q", "-m", "seed"]);
        for branch in extra_branches {
            self.git(&src, &["branch", branch]);
        }

        let bare = self.work.path().join(format!("{name}.git"));
        self.git(
            self.work.path(),
            &[
                "clone",
                "-q",
                "--bare",
                src.to_str().unwrap(),
                bare.to_str().unwrap(),
            ],
        );
        bare
    }

    pub fn origin_url(&self, bare_path: &Path) -> String {
        format!("file://{}", bare_path.display())
    }

    fn git(&self, dir: &Path, args: &[&str]) {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(dir);
        self.apply_env(&mut cmd);
        let status = cmd.status().expect("git must run");
        assert!(status.success(), "git {args:?} failed in {dir:?}");
    }

    /// Writes an executable hook script for `repo`.
    pub fn write_hook(&self, repo: &str, hook: &str, script: &str) -> PathBuf {
        let dir = self.root.path().join("hooks").join(repo);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(hook);
        std::fs::write(&path, script).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    /// Writes a hook file without the executable bit set.
    pub fn write_non_executable_hook(&self, repo: &str, hook: &str, script: &str) -> PathBuf {
        let dir = self.root.path().join("hooks").join(repo);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(hook);
        std::fs::write(&path, script).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    pub fn tree_path(&self, repo: &str, tree: &str) -> PathBuf {
        self.root.path().join("trees").join(repo).join(tree)
    }

    pub fn status_path(&self, repo: &str, tree: &str) -> PathBuf {
        self.root
            .path()
            .join("state/trees")
            .join(repo)
            .join(format!("{tree}.json"))
    }

    pub fn log_path(&self, repo: &str, tree: &str) -> PathBuf {
        self.root
            .path()
            .join("state/trees")
            .join(repo)
            .join(format!("{tree}.log"))
    }

    pub fn read_status(&self, repo: &str, tree: &str) -> Option<serde_json::Value> {
        let bytes = std::fs::read(self.status_path(repo, tree)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Writes a status file by hand, bypassing `wrk` entirely, to simulate a
    /// status a real hook run left behind (or never got to update).
    pub fn write_status(&self, repo: &str, tree: &str, value: &serde_json::Value) {
        let path = self.status_path(repo, tree);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    }
}

impl Default for Env {
    fn default() -> Self {
        Env::new()
    }
}

/// Polls `check` every 20ms until it returns `Some`, or panics once
/// `timeout` has elapsed. Keeps tests free of fixed sleeps.
pub fn poll_until<T>(timeout: Duration, mut check: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = check() {
            return value;
        }
        if Instant::now() >= deadline {
            panic!("condition was not met within {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn wait_for_status(env: &Env, repo: &str, tree: &str, status: &str, timeout: Duration) {
    poll_until(timeout, || {
        let value = env.read_status(repo, tree)?;
        if value.get("status")?.as_str()? == status {
            Some(())
        } else {
            None
        }
    });
}
