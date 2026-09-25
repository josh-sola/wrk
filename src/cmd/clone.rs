use std::path::PathBuf;

use serde::Serialize;

use crate::git;
use crate::output::{self, WrkError};
use crate::paths::{self, Paths};
use crate::resolve::{Kind, validate_name};

#[derive(Debug, Serialize)]
pub struct Cloned {
    pub repo: String,
    pub path: PathBuf,
    pub url: String,
}

/// Derives the default repo name from a clone URL: the basename, minus a
/// trailing `.git`, handling trailing slashes and `host:path` scp URLs.
pub fn default_name_from_url(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    let after_slash = trimmed.rsplit('/').next().unwrap_or(trimmed);
    let base = if after_slash == trimmed && trimmed.contains(':') && !trimmed.contains("://") {
        trimmed.rsplit(':').next().unwrap_or(trimmed)
    } else {
        after_slash
    };
    base.strip_suffix(".git").unwrap_or(base).to_string()
}

pub fn clone(paths: &Paths, url: &str, name: Option<&str>) -> Result<Cloned, WrkError> {
    let name = match name {
        Some(n) => n.to_string(),
        None => default_name_from_url(url),
    };
    validate_name(Kind::Repo, &name)?;

    let dest = paths.repo(&name);
    if dest.exists() {
        return Err(WrkError::AlreadyExists(name));
    }
    paths::ensure_dir(&paths.repos_dir())?;

    match clone_and_detach(url, &dest) {
        Ok(()) => Ok(Cloned {
            repo: name,
            path: dest,
            url: url.to_string(),
        }),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dest);
            Err(e)
        }
    }
}

fn clone_and_detach(url: &str, dest: &std::path::Path) -> Result<(), WrkError> {
    git::clone_no_checkout(url, dest)?;
    let sha = git::rev_parse_head(dest)?;
    git::detach_head(dest, &sha)?;
    if !git::remote_head_exists(dest)? {
        git::set_remote_head_auto(dest)?;
    }
    Ok(())
}

pub fn run(paths: &Paths, url: &str, name: Option<&str>, json: bool) -> i32 {
    match clone(paths, url, name) {
        Ok(cloned) => {
            if json {
                output::print_json(&cloned);
            } else {
                println!("{}", cloned.path.display());
            }
            0
        }
        Err(e) => {
            output::emit_error(json, &e);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_name_strips_dot_git() {
        assert_eq!(
            default_name_from_url("https://example.com/org/repo.git"),
            "repo"
        );
    }

    #[test]
    fn default_name_handles_trailing_slash() {
        assert_eq!(
            default_name_from_url("https://example.com/org/repo/"),
            "repo"
        );
    }

    #[test]
    fn default_name_handles_scp_style_with_slash() {
        assert_eq!(default_name_from_url("git@host:group/repo.git"), "repo");
    }

    #[test]
    fn default_name_handles_scp_style_without_slash() {
        assert_eq!(default_name_from_url("git@host:repo.git"), "repo");
    }

    #[test]
    fn default_name_handles_no_dot_git_suffix() {
        assert_eq!(
            default_name_from_url("https://example.com/org/repo"),
            "repo"
        );
    }
}
