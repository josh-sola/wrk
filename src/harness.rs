use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::config::Config;
use crate::output::WrkError;
use crate::resolve::{self, Kind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Harness {
    pub name: String,
    pub command: Vec<String>,
}

pub fn resolve(config: &Config, prefix: &str) -> Result<Harness, WrkError> {
    let mut names: Vec<String> = config.harnesses.keys().cloned().collect();
    names.sort();
    let name = resolve::resolve(Kind::Harness, prefix, &names)?;
    let command = config.harnesses[&name].clone();
    Ok(Harness { name, command })
}

/// Execs `harness` in `tree_path`, replacing the current process. Only
/// returns when the exec itself fails.
pub fn exec_in(
    harness: &Harness,
    tree_path: &Path,
    env: Vec<(&'static str, String)>,
    extra_args: &[String],
) -> WrkError {
    let program = &harness.command[0];
    let mut cmd = Command::new(program);
    cmd.args(&harness.command[1..]);
    cmd.args(extra_args);
    cmd.current_dir(tree_path);
    cmd.envs(env);

    let err = cmd.exec();
    if err.kind() == std::io::ErrorKind::NotFound {
        WrkError::HarnessNotFound(format!("{program}: command not found on PATH"))
    } else {
        WrkError::Io(err.to_string())
    }
}
