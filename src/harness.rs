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

/// Replaces every `{tree}` occurrence in each arg with `tree`.
fn substitute(command: &[String], tree: &str) -> Vec<String> {
    command
        .iter()
        .map(|arg| arg.replace("{tree}", tree))
        .collect()
}

/// Execs `harness` in `tree_path`, replacing the current process. Only
/// returns when the exec itself fails.
///
/// Each command arg containing `{tree}` has every occurrence replaced with
/// `tree`; `extra_args` passes through untouched.
pub fn exec_in(
    harness: &Harness,
    tree_path: &Path,
    tree: &str,
    env: Vec<(&'static str, String)>,
    extra_args: &[String],
) -> WrkError {
    let command = substitute(&harness.command, tree);
    let program = &command[0];
    let mut cmd = Command::new(program);
    cmd.args(&command[1..]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_replaces_the_tree_placeholder() {
        let command = vec![
            "claude".to_string(),
            "--name".to_string(),
            "{tree}".to_string(),
        ];
        assert_eq!(
            substitute(&command, "my-tree"),
            vec![
                "claude".to_string(),
                "--name".to_string(),
                "my-tree".to_string()
            ]
        );
    }

    #[test]
    fn substitute_replaces_a_placeholder_embedded_in_a_larger_arg() {
        let command = vec!["pi".to_string(), "--name={tree}".to_string()];
        assert_eq!(
            substitute(&command, "my-tree"),
            vec!["pi".to_string(), "--name=my-tree".to_string()]
        );
    }

    #[test]
    fn substitute_leaves_args_without_the_placeholder_untouched() {
        let command = vec!["codex".to_string()];
        assert_eq!(substitute(&command, "my-tree"), vec!["codex".to_string()]);
    }
}
