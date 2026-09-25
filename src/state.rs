use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustix::fs::{FlockOperation, flock};
use serde::{Deserialize, Serialize};

use crate::output::WrkError;
use crate::paths::{Paths, ensure_dir};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    None,
}

impl HookStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            HookStatus::Pending => "pending",
            HookStatus::Running => "running",
            HookStatus::Succeeded => "succeeded",
            HookStatus::Failed => "failed",
            HookStatus::None => "none",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeState {
    pub hook: String,
    pub status: HookStatus,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub exit_code: Option<i32>,
    /// Set when `status` is written as `pending`, so a later reader can
    /// tell a fresh pending state from one whose runner never showed up.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub created_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub finished_at: Option<u64>,
}

impl TreeState {
    pub fn none(hook: &str) -> Self {
        TreeState {
            hook: hook.to_string(),
            status: HookStatus::None,
            pid: None,
            exit_code: None,
            created_at: None,
            started_at: None,
            finished_at: None,
        }
    }
}

/// How long a `pending` status is given to turn into `running` before it's
/// treated as abandoned.
pub const PENDING_GRACE: Duration = Duration::from_secs(10);

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs()
}

/// Reads a tree's status file. A missing file means the tree has never run
/// a hook, which is reported as `none`.
pub fn read_status(paths: &Paths, repo: &str, tree: &str) -> Result<TreeState, WrkError> {
    let path = paths.tree_status(repo, tree);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let state: TreeState = serde_json::from_slice(&bytes)
                .map_err(|e| WrkError::Io(format!("corrupt status file {path:?}: {e}")))?;
            Ok(state)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(TreeState::none("")),
        Err(e) => Err(e.into()),
    }
}

pub fn write_status_atomic(
    paths: &Paths,
    repo: &str,
    tree: &str,
    state: &TreeState,
) -> Result<(), WrkError> {
    let dir = paths.state_trees_dir(repo);
    ensure_dir(&dir)?;
    let target = paths.tree_status(repo, tree);
    let tmp = dir.join(format!(".{tree}.json.tmp"));
    let body = serde_json::to_vec_pretty(state).expect("state must serialize");
    {
        let mut f = File::create(&tmp)?;
        f.write_all(&body)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &target)?;
    Ok(())
}

/// An open, locked lock file. Dropping it releases the flock.
pub struct Lock {
    _file: File,
}

fn open_lock_file(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
}

fn would_block(errno: rustix::io::Errno) -> bool {
    errno == rustix::io::Errno::WOULDBLOCK || errno == rustix::io::Errno::AGAIN
}

impl Lock {
    pub fn acquire_exclusive(path: &Path) -> io::Result<Lock> {
        let file = open_lock_file(path)?;
        flock(&file, FlockOperation::LockExclusive)?;
        Ok(Lock { _file: file })
    }

    pub fn try_acquire_exclusive(path: &Path) -> io::Result<Option<Lock>> {
        let file = open_lock_file(path)?;
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => Ok(Some(Lock { _file: file })),
            Err(errno) if would_block(errno) => Ok(None),
            Err(errno) => Err(errno.into()),
        }
    }

    pub fn acquire_shared(path: &Path) -> io::Result<Lock> {
        let file = open_lock_file(path)?;
        flock(&file, FlockOperation::LockShared)?;
        Ok(Lock { _file: file })
    }

    pub fn try_acquire_shared(path: &Path) -> io::Result<Option<Lock>> {
        let file = open_lock_file(path)?;
        match flock(&file, FlockOperation::NonBlockingLockShared) {
            Ok(()) => Ok(Some(Lock { _file: file })),
            Err(errno) if would_block(errno) => Ok(None),
            Err(errno) => Err(errno.into()),
        }
    }
}

pub fn try_acquire_shared_timeout(path: &Path, remaining: Duration) -> io::Result<Option<Lock>> {
    let deadline = Instant::now() + remaining;
    loop {
        if let Some(lock) = Lock::try_acquire_shared(path)? {
            return Ok(Some(lock));
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(100).min(deadline - now));
    }
}

pub fn append_log_handle(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

pub fn create_log(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    File::create(path)
}

pub fn read_log(path: &Path) -> io::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

pub fn read_log_from(path: &Path, offset: u64) -> io::Result<(String, u64)> {
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((String::new(), offset)),
        Err(e) => return Err(e),
    };
    use std::io::Seek;
    let len = f.metadata()?.len();
    if len <= offset {
        return Ok((String::new(), offset));
    }
    f.seek(io::SeekFrom::Start(offset))?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)?;
    Ok((buf, len))
}
