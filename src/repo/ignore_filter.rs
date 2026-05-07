use crate::config::app_config::IgnoreFilterConfig;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::Path;

pub(crate) struct RepoScanFilter {
    matcher: Gitignore,
}

impl RepoScanFilter {
    pub(crate) fn load(repo_root: &Path, config: &IgnoreFilterConfig) -> Option<Self> {
        if !config.enabled || !config.read_repo_gitignore {
            return None;
        }

        let gitignore_path = repo_root.join(".gitignore");
        if !gitignore_path.exists() {
            return None;
        }

        let mut builder = GitignoreBuilder::new(repo_root);
        if let Some(error) = builder.add(&gitignore_path) {
            eprintln!(
                "Warning: failed to parse repo ignore rules from {}: {}. Continuing repo scan without additional filtering.",
                gitignore_path.display(),
                error
            );
            return None;
        }

        let matcher = match builder.build() {
            Ok(matcher) => matcher,
            Err(error) => {
                eprintln!(
                    "Warning: failed to build repo ignore matcher from {}: {}. Continuing repo scan without additional filtering.",
                    gitignore_path.display(),
                    error
                );
                return None;
            }
        };

        if matcher.is_empty() {
            None
        } else {
            Some(Self { matcher })
        }
    }

    pub(crate) fn is_ignored(&self, relative_path: &Path, is_dir: bool) -> bool {
        self.matcher
            .matched_path_or_any_parents(relative_path, is_dir)
            .is_ignore()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
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

    fn enabled_config() -> IgnoreFilterConfig {
        IgnoreFilterConfig {
            enabled: true,
            read_repo_gitignore: true,
        }
    }

    #[test]
    fn test_root_anchored_rule_matches_only_repo_root_path() {
        let repo_root = create_temp_dir("ignore-root-anchored");
        fs::write(repo_root.join(".gitignore"), "/src/lib.rs\n").expect("write gitignore");

        let filter = RepoScanFilter::load(&repo_root, &enabled_config()).expect("load filter");

        assert!(filter.is_ignored(Path::new("src/lib.rs"), false));
        assert!(!filter.is_ignored(Path::new("nested/src/lib.rs"), false));
    }

    #[test]
    fn test_negation_rule_reincludes_specific_file() {
        let repo_root = create_temp_dir("ignore-negation");
        fs::write(
            repo_root.join(".gitignore"),
            "src/generated/*.rs\n!src/generated/keep.rs\n",
        )
        .expect("write gitignore");

        let filter = RepoScanFilter::load(&repo_root, &enabled_config()).expect("load filter");

        assert!(filter.is_ignored(Path::new("src/generated/drop.rs"), false));
        assert!(!filter.is_ignored(Path::new("src/generated/keep.rs"), false));
    }

    #[test]
    fn test_directory_rule_matches_directory_and_children() {
        let repo_root = create_temp_dir("ignore-directory-rule");
        fs::write(repo_root.join(".gitignore"), "build/\n").expect("write gitignore");

        let filter = RepoScanFilter::load(&repo_root, &enabled_config()).expect("load filter");

        assert!(filter.is_ignored(Path::new("build"), true));
        assert!(filter.is_ignored(Path::new("build/generated.rs"), false));
        assert!(!filter.is_ignored(Path::new("src/build.rs"), false));
    }

    #[test]
    fn test_load_only_uses_repo_root_gitignore() {
        let repo_root = create_temp_dir("ignore-root-only");
        fs::create_dir_all(repo_root.join("src")).expect("create src dir");
        fs::write(repo_root.join("src/.gitignore"), "nested.rs\n").expect("write nested gitignore");

        let filter = RepoScanFilter::load(&repo_root, &enabled_config());

        assert!(filter.is_none());
    }
}
