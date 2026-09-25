use std::path::{Path, PathBuf};

/// Every filesystem location `wrk` touches, derived from one root.
///
/// The root is `$WRK_ROOT` when set, else `$HOME/Library/wrk`. Nothing else
/// in the crate should build a path by hand.
#[derive(Debug, Clone)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    pub fn from_env() -> Self {
        let root = std::env::var_os("WRK_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var_os("HOME").expect("HOME must be set");
                PathBuf::from(home).join("Library/wrk")
            });
        Paths { root }
    }

    pub fn new(root: PathBuf) -> Self {
        Paths { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn repos_dir(&self) -> PathBuf {
        self.root.join("repos")
    }

    pub fn repo(&self, repo: &str) -> PathBuf {
        self.repos_dir().join(repo)
    }

    pub fn trees_dir(&self) -> PathBuf {
        self.root.join("trees")
    }

    pub fn repo_trees_dir(&self, repo: &str) -> PathBuf {
        self.trees_dir().join(repo)
    }

    pub fn tree(&self, repo: &str, tree: &str) -> PathBuf {
        self.repo_trees_dir(repo).join(tree)
    }

    pub fn config(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn hooks_dir(&self) -> PathBuf {
        self.root.join("hooks")
    }

    pub fn repo_hooks_dir(&self, repo: &str) -> PathBuf {
        self.hooks_dir().join(repo)
    }

    pub fn hook(&self, repo: &str, hook: &str) -> PathBuf {
        self.repo_hooks_dir(repo).join(hook)
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join("state")
    }

    pub fn state_repos_dir(&self) -> PathBuf {
        self.state_dir().join("repos")
    }

    pub fn repo_lock(&self, repo: &str) -> PathBuf {
        self.state_repos_dir().join(format!("{repo}.lock"))
    }

    pub fn state_trees_dir(&self, repo: &str) -> PathBuf {
        self.state_dir().join("trees").join(repo)
    }

    pub fn tree_status(&self, repo: &str, tree: &str) -> PathBuf {
        self.state_trees_dir(repo).join(format!("{tree}.json"))
    }

    pub fn tree_lock(&self, repo: &str, tree: &str) -> PathBuf {
        self.state_trees_dir(repo).join(format!("{tree}.lock"))
    }

    pub fn tree_log(&self, repo: &str, tree: &str) -> PathBuf {
        self.state_trees_dir(repo).join(format!("{tree}.log"))
    }
}

pub fn ensure_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}
