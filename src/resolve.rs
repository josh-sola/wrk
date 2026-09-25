use crate::output::WrkError;
use crate::paths::Paths;

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Repo,
    Tree,
    Harness,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Repo => "repo",
            Kind::Tree => "tree",
            Kind::Harness => "harness",
        }
    }
}

/// Names read from a directory listing, sorted for stable output.
fn list_dir_names(dir: &std::path::Path) -> Result<Vec<String>, WrkError> {
    let mut names = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(names),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && let Some(name) = entry.file_name().to_str()
        {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}

pub fn repos(paths: &Paths) -> Result<Vec<String>, WrkError> {
    list_dir_names(&paths.repos_dir())
}

pub fn trees(paths: &Paths, repo: &str) -> Result<Vec<String>, WrkError> {
    list_dir_names(&paths.repo_trees_dir(repo))
}

/// Exact match, then unique prefix, then error. Ambiguous prefixes report
/// every matching candidate, sorted.
pub fn resolve(kind: Kind, input: &str, candidates: &[String]) -> Result<String, WrkError> {
    if candidates.iter().any(|c| c == input) {
        return Ok(input.to_string());
    }
    let mut matches: Vec<&String> = candidates.iter().filter(|c| c.starts_with(input)).collect();
    match matches.len() {
        0 => Err(WrkError::NotFound(format!(
            "no {} matches {input:?}",
            kind.label()
        ))),
        1 => Ok(matches.remove(0).clone()),
        _ => {
            let mut candidates: Vec<String> = matches.into_iter().cloned().collect();
            candidates.sort();
            Err(WrkError::AmbiguousPrefix {
                prefix: input.to_string(),
                candidates,
            })
        }
    }
}

pub fn resolve_repo(paths: &Paths, input: &str) -> Result<String, WrkError> {
    let candidates = repos(paths)?;
    resolve(Kind::Repo, input, &candidates)
}

pub fn resolve_tree(paths: &Paths, repo: &str, input: &str) -> Result<String, WrkError> {
    let candidates = trees(paths, repo)?;
    resolve(Kind::Tree, input, &candidates)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeMatch {
    Existing(String),
    Create(String),
}

fn match_tree_or_new(input: &str, candidates: &[String]) -> Result<TreeMatch, WrkError> {
    if candidates.iter().any(|c| c == input) {
        return Ok(TreeMatch::Existing(input.to_string()));
    }
    let matches: Vec<&String> = candidates.iter().filter(|c| c.starts_with(input)).collect();
    match matches.len() {
        0 => Ok(TreeMatch::Create(input.to_string())),
        1 => Ok(TreeMatch::Existing(matches[0].clone())),
        _ => {
            let mut candidates: Vec<String> = matches.into_iter().cloned().collect();
            candidates.sort();
            Err(WrkError::AmbiguousPrefix {
                prefix: input.to_string(),
                candidates,
            })
        }
    }
}

/// Resolves a tree name for `go`: exact match, then unique prefix, then
/// (when nothing matches) the literal name to create. `force_new` skips
/// matching altogether and always creates the literal name.
pub fn resolve_or_new(
    paths: &Paths,
    repo: &str,
    input: &str,
    force_new: bool,
) -> Result<TreeMatch, WrkError> {
    if force_new {
        return Ok(TreeMatch::Create(input.to_string()));
    }
    let candidates = trees(paths, repo)?;
    match_tree_or_new(input, &candidates)
}

/// Structural check shared by repo and tree names: non-empty, no `/`, and
/// must not start with `.`. Tree names get an additional git ref-format
/// check where they are created.
pub fn validate_name(kind: Kind, name: &str) -> Result<(), WrkError> {
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        return Err(WrkError::InvalidName(format!(
            "invalid {} name {name:?}",
            kind.label()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_exact_match_wins_even_with_prefix_collision() {
        let candidates = vec!["foo".to_string(), "foobar".to_string()];
        assert_eq!(resolve(Kind::Repo, "foo", &candidates).unwrap(), "foo");
    }

    #[test]
    fn resolve_unique_prefix() {
        let candidates = vec!["alpha".to_string(), "beta".to_string()];
        assert_eq!(resolve(Kind::Repo, "al", &candidates).unwrap(), "alpha");
    }

    #[test]
    fn resolve_ambiguous_prefix_lists_sorted_candidates() {
        let candidates = vec!["bravo".to_string(), "alpha".to_string(), "beta".to_string()];
        let err = resolve(Kind::Repo, "b", &candidates).unwrap_err();
        assert_eq!(err.code(), "ambiguous_prefix");
        assert_eq!(
            err.candidates().unwrap(),
            &["beta".to_string(), "bravo".to_string()]
        );
    }

    #[test]
    fn resolve_no_match() {
        let candidates = vec!["alpha".to_string()];
        let err = resolve(Kind::Repo, "z", &candidates).unwrap_err();
        assert_eq!(err.code(), "not_found");
    }

    #[test]
    fn validate_name_rejects_slash_dot_and_empty() {
        assert!(validate_name(Kind::Tree, "").is_err());
        assert!(validate_name(Kind::Tree, "a/b").is_err());
        assert!(validate_name(Kind::Tree, ".hidden").is_err());
        assert!(validate_name(Kind::Tree, "fine").is_ok());
    }

    #[test]
    fn match_tree_or_new_exact_match_wins() {
        let candidates = vec!["foo".to_string(), "foobar".to_string()];
        assert_eq!(
            match_tree_or_new("foo", &candidates).unwrap(),
            TreeMatch::Existing("foo".to_string())
        );
    }

    #[test]
    fn match_tree_or_new_unique_prefix() {
        let candidates = vec!["alpha".to_string(), "beta".to_string()];
        assert_eq!(
            match_tree_or_new("al", &candidates).unwrap(),
            TreeMatch::Existing("alpha".to_string())
        );
    }

    #[test]
    fn match_tree_or_new_ambiguous_prefix_errors() {
        let candidates = vec!["alpha".to_string(), "alphabet".to_string()];
        let err = match_tree_or_new("alph", &candidates).unwrap_err();
        assert_eq!(err.code(), "ambiguous_prefix");
    }

    #[test]
    fn match_tree_or_new_creates_when_nothing_matches() {
        let candidates = vec!["alpha".to_string()];
        assert_eq!(
            match_tree_or_new("zzz", &candidates).unwrap(),
            TreeMatch::Create("zzz".to_string())
        );
    }
}
