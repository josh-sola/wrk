use crate::hooks;
use crate::paths::Paths;

/// `wrk internal run-hook <repo> <tree> <hook>`: the detached process `new`
/// spawns to run `on_create`. Never invoked directly by a user.
pub fn run_hook(paths: &Paths, repo: &str, tree: &str, hook: &str) -> i32 {
    match hooks::run_in_foreground(paths, repo, tree, hook) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}
