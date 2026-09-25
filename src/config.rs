use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::output::WrkError;
use crate::paths::Paths;

const DEFAULT_HARNESS: &str = "claude";
const BUILTIN_HARNESSES: &[&str] = &["claude", "codex", "pi", "devin"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HarnessSpec {
    command: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    default_harness: Option<String>,
    #[serde(default)]
    harnesses: BTreeMap<String, HarnessSpec>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub default_harness: String,
    pub harnesses: BTreeMap<String, Vec<String>>,
}

fn builtin_harnesses() -> BTreeMap<String, Vec<String>> {
    BUILTIN_HARNESSES
        .iter()
        .map(|name| ((*name).to_string(), vec![(*name).to_string()]))
        .collect()
}

fn invalid(path: &Path, message: impl Into<String>) -> WrkError {
    WrkError::ConfigInvalid {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

fn merge(path: &Path, raw: RawConfig) -> Result<Config, WrkError> {
    let mut harnesses = builtin_harnesses();
    for (name, spec) in raw.harnesses {
        if spec.command.is_empty() {
            return Err(invalid(
                path,
                format!("harness {name:?} has an empty command"),
            ));
        }
        harnesses.insert(name, spec.command);
    }

    let default_harness = raw
        .default_harness
        .unwrap_or_else(|| DEFAULT_HARNESS.to_string());
    if !harnesses.contains_key(&default_harness) {
        return Err(invalid(
            path,
            format!("default_harness {default_harness:?} matches no configured harness"),
        ));
    }

    Ok(Config {
        default_harness,
        harnesses,
    })
}

/// Loads `$WRK_ROOT/config.toml` merged over the built-in harnesses. A
/// missing file is not an error: the built-ins apply on their own.
pub fn load(paths: &Paths) -> Result<Config, WrkError> {
    let path = paths.config();
    let raw = match std::fs::read_to_string(&path) {
        Ok(contents) => toml::from_str(&contents).map_err(|e| invalid(&path, e.to_string()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => RawConfig::default(),
        Err(e) => return Err(e.into()),
    };
    merge(&path, raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn paths_with_config(contents: Option<&str>) -> (TempDir, Paths) {
        let dir = TempDir::new().expect("tempdir");
        let paths = Paths::new(dir.path().to_path_buf());
        if let Some(contents) = contents {
            std::fs::write(dir.path().join("config.toml"), contents).unwrap();
        }
        (dir, paths)
    }

    #[test]
    fn missing_file_falls_back_to_builtin_defaults() {
        let (_dir, paths) = paths_with_config(None);
        let config = load(&paths).unwrap();
        assert_eq!(config.default_harness, "claude");
        assert_eq!(config.harnesses.len(), 4);
        assert_eq!(config.harnesses["claude"], vec!["claude".to_string()]);
    }

    #[test]
    fn user_entry_overrides_a_builtin() {
        let (_dir, paths) = paths_with_config(Some(
            "[harnesses.claude]\ncommand = [\"claude\", \"--danger\"]\n",
        ));
        let config = load(&paths).unwrap();
        assert_eq!(
            config.harnesses["claude"],
            vec!["claude".to_string(), "--danger".to_string()]
        );
    }

    #[test]
    fn a_user_harness_not_overriding_a_builtin_is_added() {
        let (_dir, paths) =
            paths_with_config(Some("[harnesses.custom]\ncommand = [\"custom-cli\"]\n"));
        let config = load(&paths).unwrap();
        assert_eq!(config.harnesses.len(), 5);
        assert_eq!(config.harnesses["custom"], vec!["custom-cli".to_string()]);
    }

    #[test]
    fn invalid_toml_is_config_invalid() {
        let (_dir, paths) = paths_with_config(Some("not valid toml === ["));
        let err = load(&paths).unwrap_err();
        assert_eq!(err.code(), "config_invalid");
    }

    #[test]
    fn unknown_field_is_rejected() {
        let (_dir, paths) = paths_with_config(Some("bogus = true\n"));
        let err = load(&paths).unwrap_err();
        assert_eq!(err.code(), "config_invalid");
    }

    #[test]
    fn bad_default_harness_is_rejected() {
        let (_dir, paths) = paths_with_config(Some("default_harness = \"nope\"\n"));
        let err = load(&paths).unwrap_err();
        assert_eq!(err.code(), "config_invalid");
    }

    #[test]
    fn empty_command_is_rejected() {
        let (_dir, paths) = paths_with_config(Some("[harnesses.claude]\ncommand = []\n"));
        let err = load(&paths).unwrap_err();
        assert_eq!(err.code(), "config_invalid");
    }
}
