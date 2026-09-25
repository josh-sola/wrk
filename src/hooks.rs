use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};

use crate::output::WrkError;
use crate::paths::Paths;
use crate::state::{self, HookStatus, TreeState};

fn is_executable(path: &Path) -> std::io::Result<bool> {
    let mode = std::fs::metadata(path)?.permissions().mode();
    Ok(mode & 0o111 != 0)
}

enum Presence {
    Absent,
    NotExecutable(PathBuf),
    Present(PathBuf),
}

fn hook_presence(paths: &Paths, repo: &str, hook: &str) -> Result<Presence, WrkError> {
    let path = paths.hook(repo, hook);
    if !path.exists() {
        return Ok(Presence::Absent);
    }
    if is_executable(&path)? {
        Ok(Presence::Present(path))
    } else {
        Ok(Presence::NotExecutable(path))
    }
}

pub(crate) fn hook_env(paths: &Paths, repo: &str, tree: &str) -> Vec<(&'static str, String)> {
    let repo_path = paths.repo(repo);
    let tree_path = paths.tree(repo, tree);
    vec![
        ("WRK_ROOT", paths.root().display().to_string()),
        ("WRK_REPO", repo.to_string()),
        ("WRK_TREE", tree.to_string()),
        ("WRK_TREE_PATH", tree_path.display().to_string()),
        ("WRK_REPO_PATH", repo_path.display().to_string()),
        ("WRK_BRANCH", tree.to_string()),
    ]
}

/// Starts `on_create` for a freshly created tree. Writes `none` and returns
/// without spawning when no hook file exists. A present-but-not-executable
/// hook is recorded as `failed` and returned as an error: the tree stays,
/// only the hook is considered to have failed.
pub fn start_on_create(paths: &Paths, repo: &str, tree: &str) -> Result<HookStatus, WrkError> {
    match hook_presence(paths, repo, "on_create")? {
        Presence::Absent => {
            state::write_status_atomic(paths, repo, tree, &TreeState::none("on_create"))?;
            Ok(HookStatus::None)
        }
        Presence::NotExecutable(path) => {
            state::write_status_atomic(
                paths,
                repo,
                tree,
                &TreeState {
                    hook: "on_create".to_string(),
                    status: HookStatus::Failed,
                    pid: None,
                    exit_code: None,
                    created_at: None,
                    started_at: Some(state::now_unix()),
                    finished_at: Some(state::now_unix()),
                },
            )?;
            Err(WrkError::HookNotExecutable(path.display().to_string()))
        }
        Presence::Present(_) => {
            state::write_status_atomic(
                paths,
                repo,
                tree,
                &TreeState {
                    hook: "on_create".to_string(),
                    status: HookStatus::Pending,
                    pid: None,
                    exit_code: None,
                    created_at: Some(state::now_unix()),
                    started_at: None,
                    finished_at: None,
                },
            )?;
            spawn_runner(paths, repo, tree, "on_create")?;
            Ok(HookStatus::Pending)
        }
    }
}

/// Double-forks so the runner is never our child: `go` execs a harness in
/// our place, which would never reap it. The runner leads its own session,
/// so `rm --force` can `killpg` it and everything it spawned.
fn spawn_runner(paths: &Paths, repo: &str, tree: &str, hook: &str) -> Result<(), WrkError> {
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.args(["internal", "run-hook", repo, tree, hook])
        .env("WRK_ROOT", paths.root())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| match libc::fork() {
            -1 => Err(std::io::Error::last_os_error()),
            0 => {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            }
            _ => libc::_exit(0),
        });
    }
    let mut intermediate = cmd.spawn()?;
    intermediate.wait()?;
    Ok(())
}

/// Runs a hook in the foreground for the calling process: holds the tree
/// lock for its whole life, so releasing it is the liveness signal a crashed
/// run leaves behind.
pub fn run_in_foreground(
    paths: &Paths,
    repo: &str,
    tree: &str,
    hook: &str,
) -> Result<(), WrkError> {
    let lock_path = paths.tree_lock(repo, tree);
    let _lock = state::Lock::acquire_exclusive(&lock_path)?;

    // A tree removed while this runner was still pending, or a status that
    // moved on without us, means we lost the race with `rm`. Do nothing.
    let current = state::read_status(paths, repo, tree)?;
    if current.hook != hook
        || current.status != HookStatus::Pending
        || !paths.tree(repo, tree).exists()
    {
        return Ok(());
    }

    let pid = std::process::id();
    let started_at = state::now_unix();
    state::write_status_atomic(
        paths,
        repo,
        tree,
        &TreeState {
            hook: hook.to_string(),
            status: HookStatus::Running,
            pid: Some(pid),
            exit_code: None,
            created_at: current.created_at,
            started_at: Some(started_at),
            finished_at: None,
        },
    )?;

    let log_path = paths.tree_log(repo, tree);
    let mut log_file = state::create_log(&log_path)?;
    writeln!(log_file, "==> {hook} started at {started_at} (pid {pid})")?;

    let hook_path = paths.hook(repo, hook);
    let tree_path = paths.tree(repo, tree);
    let mut command = Command::new(&hook_path);
    command
        .current_dir(&tree_path)
        .stdin(Stdio::null())
        .stdout(log_file.try_clone()?)
        .stderr(log_file.try_clone()?);
    for (key, value) in hook_env(paths, repo, tree) {
        command.env(key, value);
    }

    let result = command.status();
    let finished_at = state::now_unix();
    match result {
        Ok(status) => {
            state::write_status_atomic(
                paths,
                repo,
                tree,
                &TreeState {
                    hook: hook.to_string(),
                    status: if status.success() {
                        HookStatus::Succeeded
                    } else {
                        HookStatus::Failed
                    },
                    pid: Some(pid),
                    exit_code: status.code(),
                    created_at: None,
                    started_at: Some(started_at),
                    finished_at: Some(finished_at),
                },
            )?;
            Ok(())
        }
        Err(e) => {
            state::write_status_atomic(
                paths,
                repo,
                tree,
                &TreeState {
                    hook: hook.to_string(),
                    status: HookStatus::Failed,
                    pid: Some(pid),
                    exit_code: None,
                    created_at: None,
                    started_at: Some(started_at),
                    finished_at: Some(finished_at),
                },
            )?;
            Err(e.into())
        }
    }
}

/// Runs `on_destroy` synchronously in the caller's process, mirroring its
/// output to stderr and appending it to the tree's log. Returns `None` when
/// no hook file exists.
pub fn run_on_destroy(
    paths: &Paths,
    repo: &str,
    tree: &str,
) -> Result<Option<ExitStatus>, WrkError> {
    let path = match hook_presence(paths, repo, "on_destroy")? {
        Presence::Absent => return Ok(None),
        Presence::NotExecutable(path) => {
            return Err(WrkError::HookNotExecutable(path.display().to_string()));
        }
        Presence::Present(path) => path,
    };

    let tree_path = paths.tree(repo, tree);
    let log_path = paths.tree_log(repo, tree);
    let log_file = state::append_log_handle(&log_path)?;
    let log_file = Arc::new(Mutex::new(log_file));

    let mut command = Command::new(&path);
    command
        .current_dir(&tree_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in hook_env(paths, repo, tree) {
        command.env(key, value);
    }

    let mut child = command.spawn()?;
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    let out_log = log_file.clone();
    let out_thread = std::thread::spawn(move || tee_to_stderr_and_log(stdout, out_log));
    let err_log = log_file.clone();
    let err_thread = std::thread::spawn(move || tee_to_stderr_and_log(stderr, err_log));

    let status = child.wait()?;
    let _ = out_thread.join();
    let _ = err_thread.join();

    Ok(Some(status))
}

fn tee_to_stderr_and_log<R: Read>(mut reader: R, log: Arc<Mutex<File>>) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let mut stderr = std::io::stderr();
                let _ = stderr.write_all(&buf[..n]);
                let _ = stderr.flush();
                if let Ok(mut f) = log.lock() {
                    let _ = f.write_all(&buf[..n]);
                }
            }
            Err(_) => break,
        }
    }
}
