use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const APP_CONFIG_PATH: &str = "./config/app-config.json";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct AppConfig {
    pub(crate) repo_scan: RepoScanConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            repo_scan: RepoScanConfig::default(),
        }
    }
}

impl AppConfig {
    pub(crate) fn load_or_create() -> Result<Self, AppConfigError> {
        Self::load_or_create_at(Path::new(APP_CONFIG_PATH))
    }

    pub(crate) fn load_or_create_at(path: &Path) -> Result<Self, AppConfigError> {
        let path = path.to_path_buf();
        if !path.exists() {
            let config = AppConfig::default();
            if let Err(error) = write_config(&path, &config) {
                eprintln!(
                    "Warning: failed to create application config at {}: {}. Continuing with defaults.",
                    path.display(),
                    error
                );
            }
            return Ok(config);
        }

        let content = fs::read_to_string(&path).map_err(|error| AppConfigError {
            path: path.clone(),
            kind: AppConfigErrorKind::Read(error),
        })?;

        let partial =
            serde_json::from_str::<PartialAppConfig>(&content).map_err(|error| AppConfigError {
                path: path.clone(),
                kind: AppConfigErrorKind::Parse(error.to_string()),
            })?;

        let needs_rewrite = partial.is_incomplete();
        let config = partial.into_complete();
        if needs_rewrite && let Err(error) = write_config(&path, &config) {
            eprintln!(
                "Warning: failed to update application config at {}: {}. Continuing with the in-memory defaults. You can delete the file and rerun the command to recreate it.",
                path.display(),
                error
            );
        }

        Ok(config)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct RepoScanConfig {
    pub(crate) ignore_filter: IgnoreFilterConfig,
}

impl Default for RepoScanConfig {
    fn default() -> Self {
        Self {
            ignore_filter: IgnoreFilterConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct IgnoreFilterConfig {
    pub(crate) enabled: bool,
    pub(crate) read_repo_gitignore: bool,
}

impl Default for IgnoreFilterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            read_repo_gitignore: true,
        }
    }
}

#[derive(Debug)]
pub(crate) struct AppConfigError {
    path: PathBuf,
    kind: AppConfigErrorKind,
}

#[derive(Debug)]
enum AppConfigErrorKind {
    Read(std::io::Error),
    Parse(String),
}

impl Display for AppConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            AppConfigErrorKind::Read(error) => {
                write!(
                    f,
                    "Failed to read application config {}: {}",
                    self.path.display(),
                    error
                )
            }
            AppConfigErrorKind::Parse(error) => {
                write!(
                    f,
                    "Invalid application config {}: {}. You can delete the file and rerun the command to recreate it.",
                    self.path.display(),
                    error
                )
            }
        }
    }
}

impl std::error::Error for AppConfigError {}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct PartialAppConfig {
    repo_scan: Option<PartialRepoScanConfig>,
}

impl PartialAppConfig {
    fn into_complete(self) -> AppConfig {
        AppConfig {
            repo_scan: self.repo_scan.unwrap_or_default().into_complete(),
        }
    }

    fn is_incomplete(&self) -> bool {
        self.repo_scan
            .as_ref()
            .is_none_or(PartialRepoScanConfig::is_incomplete)
    }
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct PartialRepoScanConfig {
    ignore_filter: Option<PartialIgnoreFilterConfig>,
}

impl PartialRepoScanConfig {
    fn into_complete(self) -> RepoScanConfig {
        RepoScanConfig {
            ignore_filter: self.ignore_filter.unwrap_or_default().into_complete(),
        }
    }

    fn is_incomplete(&self) -> bool {
        self.ignore_filter
            .as_ref()
            .is_none_or(PartialIgnoreFilterConfig::is_incomplete)
    }
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct PartialIgnoreFilterConfig {
    enabled: Option<bool>,
    read_repo_gitignore: Option<bool>,
}

impl PartialIgnoreFilterConfig {
    fn into_complete(self) -> IgnoreFilterConfig {
        IgnoreFilterConfig {
            enabled: self.enabled.unwrap_or(true),
            read_repo_gitignore: self.read_repo_gitignore.unwrap_or(true),
        }
    }

    fn is_incomplete(&self) -> bool {
        self.enabled.is_none() || self.read_repo_gitignore.is_none()
    }
}

fn write_config(path: &Path, config: &AppConfig) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(config).map_err(std::io::Error::other)?;
    fs::write(path, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "codeatlas2-{}-{}-{}",
            prefix,
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn test_load_or_create_creates_default_config() {
        let temp_dir = create_temp_dir("config-create");
        let config_path = temp_dir.join("config").join("app-config.json");

        let config = AppConfig::load_or_create_at(&config_path).expect("load config");

        assert_eq!(config, AppConfig::default());
        let stored = fs::read_to_string(&config_path).expect("read config file");
        let stored: AppConfig = serde_json::from_str(&stored).expect("parse stored config");
        assert_eq!(stored, AppConfig::default());
    }

    #[test]
    fn test_load_or_create_backfills_missing_fields() {
        let temp_dir = create_temp_dir("config-backfill");
        let config_path = temp_dir.join("config").join("app-config.json");
        fs::create_dir_all(config_path.parent().expect("config parent"))
            .expect("create config dir");
        fs::write(
            &config_path,
            r#"{
  "repo_scan": {
    "ignore_filter": {
      "enabled": false
    }
  }
}"#,
        )
        .expect("write partial config");

        let config = AppConfig::load_or_create_at(&config_path).expect("load config");

        assert_eq!(
            config,
            AppConfig {
                repo_scan: RepoScanConfig {
                    ignore_filter: IgnoreFilterConfig {
                        enabled: false,
                        read_repo_gitignore: true,
                    },
                },
            }
        );

        let stored = fs::read_to_string(&config_path).expect("read config file");
        let stored: AppConfig = serde_json::from_str(&stored).expect("parse stored config");
        assert_eq!(stored, config);
    }

    #[test]
    fn test_load_or_create_rejects_invalid_json() {
        let temp_dir = create_temp_dir("config-invalid");
        let config_path = temp_dir.join("config").join("app-config.json");
        fs::create_dir_all(config_path.parent().expect("config parent"))
            .expect("create config dir");
        fs::write(&config_path, "{ invalid json }").expect("write invalid config");

        let error =
            AppConfig::load_or_create_at(&config_path).expect_err("invalid config should fail");

        let message = error.to_string();
        assert!(message.contains("app-config.json"));
        assert!(message.contains("delete the file"));
    }
}
