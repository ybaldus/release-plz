use anyhow::Context;
use release_plz_core::{ReleaseRequest, UpdateRequest};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, time::Duration};
use url::Url;

/// You can find the documentation of the configuration file
/// [here](https://release-plz.ieni.dev/docs/config).
#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// # Workspace
    /// Global configuration. Applied to all packages by default.
    #[serde(default)]
    pub workspace: Workspace,
    /// # Package
    /// Package-specific configuration. This overrides `workspace`.
    /// Not all settings of `workspace` can be overridden.
    #[serde(default)]
    package: Vec<PackageSpecificConfigWithName>,
}

impl Config {
    /// Package-specific configurations.
    /// Returns `<package name, package config>`.
    fn packages(&self) -> HashMap<&str, &PackageSpecificConfig> {
        self.package
            .iter()
            .map(|p| (p.name.as_str(), &p.config))
            .collect()
    }

    pub fn fill_update_config(
        &self,
        is_changelog_update_disabled: bool,
        update_request: UpdateRequest,
    ) -> UpdateRequest {
        let mut default_update_config = self.workspace.packages_defaults.clone();
        if is_changelog_update_disabled {
            default_update_config.changelog_update = false.into();
        }
        let mut update_request =
            update_request.with_default_package_config(default_update_config.into());
        for (package, config) in self.packages() {
            let mut update_config = config.clone();
            update_config = update_config.merge(self.workspace.packages_defaults.clone());
            if is_changelog_update_disabled {
                update_config.common.changelog_update = false.into();
            }
            update_request = update_request.with_package_config(package, update_config.into());
        }
        update_request
    }

    pub fn fill_release_config(
        &self,
        allow_dirty: bool,
        no_verify: bool,
        release_request: ReleaseRequest,
    ) -> ReleaseRequest {
        let mut default_config = self.workspace.packages_defaults.clone();
        if no_verify {
            default_config.publish_no_verify = Some(true);
        }
        if allow_dirty {
            default_config.publish_allow_dirty = Some(true);
        }
        let mut release_request =
            release_request.with_default_package_config(default_config.into());

        for (package, config) in self.packages() {
            let mut release_config = config.clone();
            release_config = release_config.merge(self.workspace.packages_defaults.clone());

            if no_verify {
                release_config.common.publish_no_verify = Some(true);
            }
            if allow_dirty {
                release_config.common.publish_allow_dirty = Some(true);
            }
            release_request = release_request.with_package_config(package, release_config.into());
        }
        release_request
    }
}

/// Config at the `[workspace]` level.
#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, JsonSchema)]
pub struct Workspace {
    /// Configuration applied at the `[[package]]` level, too.
    #[serde(flatten)]
    pub packages_defaults: PackageConfig,
    /// # Allow Dirty
    /// - If `true`, allow dirty working directories to be updated. The uncommitted changes will be part of the update.
    /// - If `false` or [`Option::None`], the command will fail if the working directory is dirty.
    pub allow_dirty: Option<bool>,
    /// # Changelog Config
    /// Path to the git cliff configuration file. Defaults to the `keep a changelog` configuration.
    pub changelog_config: Option<PathBuf>,
    /// # Dependencies Update
    /// - If `true`, update all the dependencies in the Cargo.lock file by running `cargo update`.
    /// - If `false` or [`Option::None`], only update the workspace packages by running `cargo update --workspace`.
    pub dependencies_update: Option<bool>,
    /// # PR Draft
    /// If `true`, the created release PR will be marked as a draft.
    #[serde(default)]
    pub pr_draft: bool,
    /// # PR Labels
    /// Labels to add to the release PR.
    #[serde(default)]
    pub pr_labels: Vec<String>,
    /// # Publish Timeout
    /// Timeout for the publishing process
    pub publish_timeout: Option<String>,
    /// # Repo URL
    /// GitHub/Gitea repository url where your project is hosted.
    /// It is used to generate the changelog release link.
    /// It defaults to the url of the default remote.
    pub repo_url: Option<Url>,
}

impl Workspace {
    /// Get the publish timeout. Defaults to 30 minutes.
    pub fn publish_timeout(&self) -> anyhow::Result<Duration> {
        let publish_timeout = self.publish_timeout.as_deref().unwrap_or("30m");
        duration_str::parse(publish_timeout)
            .with_context(|| format!("invalid publish_timeout {}", publish_timeout))
    }
}

/// Config at the `[[package]]` level.
#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, JsonSchema)]
pub struct PackageSpecificConfig {
    /// Configuration that can be specified at the `[workspace]` level, too.
    #[serde(flatten)]
    common: PackageConfig,
    /// # Changelog Path
    /// Normally the changelog is placed in the same directory of the Cargo.toml file.
    /// The user can provide a custom path here.
    /// This changelog_path needs to be propagated to all the commands:
    /// `update`, `release-pr` and `release`.
    changelog_path: Option<PathBuf>,
    /// # Changelog Include
    /// List of package names.
    /// Include the changelogs of these packages in the changelog of the current package.
    changelog_include: Option<Vec<String>>,
}

impl PackageSpecificConfig {
    /// Merge the package-specific configuration with the global configuration.
    pub fn merge(self, default: PackageConfig) -> PackageSpecificConfig {
        PackageSpecificConfig {
            common: self.common.merge(default),
            changelog_path: self.changelog_path,
            changelog_include: self.changelog_include,
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, JsonSchema)]
pub struct PackageSpecificConfigWithName {
    pub name: String,
    #[serde(flatten)]
    pub config: PackageSpecificConfig,
}

impl From<PackageSpecificConfig> for release_plz_core::PackageReleaseConfig {
    fn from(config: PackageSpecificConfig) -> Self {
        let generic = config.common.into();

        Self {
            generic,
            changelog_path: config.changelog_path,
        }
    }
}

impl From<PackageConfig> for release_plz_core::ReleaseConfig {
    fn from(value: PackageConfig) -> Self {
        let is_publish_enabled = value.publish != Some(false);
        let is_git_release_enabled = value.git_release_enable != Some(false);
        let is_git_release_draft = value.git_release_draft == Some(true);
        let is_git_tag_enabled = value.git_tag_enable != Some(false);
        let release = value.release != Some(false);
        let mut cfg = Self::default()
            .with_publish(release_plz_core::PublishConfig::enabled(is_publish_enabled))
            .with_git_release(
                release_plz_core::GitReleaseConfig::enabled(is_git_release_enabled)
                    .set_draft(is_git_release_draft),
            )
            .with_git_tag(release_plz_core::GitTagConfig::enabled(is_git_tag_enabled))
            .with_release(release);

        if let Some(no_verify) = value.publish_no_verify {
            cfg = cfg.with_no_verify(no_verify);
        }
        if let Some(allow_dirty) = value.publish_allow_dirty {
            cfg = cfg.with_allow_dirty(allow_dirty);
        }
        cfg
    }
}

/// Configuration that can be specified both at the `[workspace]` and at the `[[package]]` level.
#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Default, Clone, JsonSchema)]
pub struct PackageConfig {
    /// # Changelog Update
    /// Whether to create/update changelog or not.
    /// If unspecified, the changelog is updated.
    pub changelog_update: Option<bool>,
    /// # Git Release Enable
    /// Publish the GitHub/Gitea release for the created git tag.
    /// Enabled by default.
    pub git_release_enable: Option<bool>,
    /// # Git Release Type
    /// Whether to mark the created release as not ready for production.
    pub git_release_type: Option<ReleaseType>,
    /// # Git Release Draft
    /// If true, will not auto-publish the release.
    pub git_release_draft: Option<bool>,
    /// # Git Tag Enable
    /// Publish the git tag for the new package version.
    /// Enabled by default.
    pub git_tag_enable: Option<bool>,
    /// # Publish
    /// If `Some(false)`, don't run `cargo publish`.
    pub publish: Option<bool>,
    /// # Publish Allow Dirty
    /// If `Some(true)`, add the `--allow-dirty` flag to the `cargo publish` command.
    pub publish_allow_dirty: Option<bool>,
    /// # Publish No Verify
    /// If `Some(true)`, add the `--no-verify` flag to the `cargo publish` command.
    pub publish_no_verify: Option<bool>,
    /// # Semver Check
    /// Controls when to run cargo-semver-checks.
    /// If unspecified, run cargo-semver-checks if the package is a library.
    pub semver_check: Option<bool>,
    /// # Release
    /// Used to toggle off the update/release process for a workspace or package.
    pub release: Option<bool>,
}

impl From<PackageConfig> for release_plz_core::UpdateConfig {
    fn from(config: PackageConfig) -> Self {
        Self {
            semver_check: config.semver_check != Some(false),
            changelog_update: config.changelog_update != Some(false),
            release: config.release != Some(false),
        }
    }
}

impl From<PackageSpecificConfig> for release_plz_core::PackageUpdateConfig {
    fn from(config: PackageSpecificConfig) -> Self {
        Self {
            generic: config.common.into(),
            changelog_path: config.changelog_path,
            changelog_include: config.changelog_include.unwrap_or_default(),
        }
    }
}

impl PackageConfig {
    /// Merge the package-specific configuration with the global configuration.
    pub fn merge(self, default: Self) -> Self {
        Self {
            semver_check: self.semver_check.or(default.semver_check),
            changelog_update: self.changelog_update.or(default.changelog_update),
            git_release_enable: self.git_release_enable.or(default.git_release_enable),
            git_release_type: self.git_release_type.or(default.git_release_type),
            git_release_draft: self.git_release_draft.or(default.git_release_draft),

            publish: self.publish.or(default.publish),
            publish_allow_dirty: self.publish_allow_dirty.or(default.publish_allow_dirty),
            publish_no_verify: self.publish_no_verify.or(default.publish_no_verify),
            git_tag_enable: self.git_tag_enable.or(default.git_tag_enable),
            release: self.release.or(default.release),
        }
    }
}

/// Whether to run cargo-semver-checks or not.
/// Note: you can only run cargo-semver-checks on a library.
#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum SemverCheck {
    /// Run cargo-semver-checks.
    #[default]
    Yes,
    /// Don't run cargo-semver-checks.
    No,
}

#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, Clone, Copy, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseType {
    /// # Prod
    /// Will mark the release as ready for production.
    #[default]
    Prod,
    /// # Pre
    /// Will mark the release as not ready for production.
    /// I.e. as pre-release.
    Pre,
    /// # Auto
    /// Will mark the release as not ready for production
    /// in case there is a semver pre-release in the tag e.g. v1.0.0-rc1.
    /// Otherwise, will mark the release as ready for production.
    Auto,
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE_WORKSPACE_CONFIG: &str = r#"
        [workspace]
        dependencies_update = false
        allow_dirty = false
        changelog_config = "../git-cliff.toml"
        repo_url = "https://github.com/MarcoIeni/release-plz"
        git_release_enable = true
        git_release_type = "prod"
        git_release_draft = false
        publish_timeout = "10m"
    "#;

    const BASE_PACKAGE_CONFIG: &str = r#"
        [[package]]
        name = "crate1"
    "#;

    fn create_base_workspace_config() -> Config {
        Config {
            workspace: Workspace {
                dependencies_update: Some(false),
                changelog_config: Some("../git-cliff.toml".into()),
                allow_dirty: Some(false),
                repo_url: Some("https://github.com/MarcoIeni/release-plz".parse().unwrap()),
                packages_defaults: PackageConfig {
                    semver_check: None,
                    changelog_update: None,
                    git_release_enable: Some(true),
                    git_release_type: Some(ReleaseType::Prod),
                    git_release_draft: Some(false),
                    ..Default::default()
                },
                pr_draft: false,
                pr_labels: vec![],
                publish_timeout: Some("10m".to_string()),
            },
            package: [].into(),
        }
    }

    fn create_base_package_config() -> PackageSpecificConfigWithName {
        PackageSpecificConfigWithName {
            name: "crate1".to_string(),
            config: PackageSpecificConfig {
                common: PackageConfig {
                    semver_check: None,
                    changelog_update: None,
                    git_release_enable: None,
                    git_release_type: None,
                    git_release_draft: None,
                    ..Default::default()
                },
                changelog_path: None,
                changelog_include: None,
            },
        }
    }

    #[test]
    fn config_without_update_config_is_deserialized() {
        let expected_config = create_base_workspace_config();

        let config: Config = toml::from_str(BASE_WORKSPACE_CONFIG).unwrap();
        assert_eq!(config, expected_config)
    }

    #[test]
    fn config_is_deserialized() {
        let config = &format!(
            "{}\
            changelog_update = true",
            BASE_WORKSPACE_CONFIG
        );

        let mut expected_config = create_base_workspace_config();
        expected_config.workspace.packages_defaults.changelog_update = true.into();

        let config: Config = toml::from_str(config).unwrap();
        assert_eq!(config, expected_config)
    }

    fn config_package_release_is_deserialized(config_flag: &str, expected_value: bool) {
        let config = &format!(
            "{}\n{}\
            release = {}",
            BASE_WORKSPACE_CONFIG, BASE_PACKAGE_CONFIG, config_flag
        );

        let mut expected_config = create_base_workspace_config();
        let mut package_config = create_base_package_config();
        package_config.config.common.release = expected_value.into();
        expected_config.package = [package_config].into();

        let config: Config = toml::from_str(config).unwrap();
        assert_eq!(config, expected_config)
    }

    #[test]
    fn config_package_release_is_deserialized_true() {
        config_package_release_is_deserialized("true", true);
    }

    #[test]
    fn config_package_release_is_deserialized_false() {
        config_package_release_is_deserialized("false", false);
    }

    fn config_workspace_release_is_deserialized(config_flag: &str, expected_value: bool) {
        let config = &format!(
            "{}\
            release = {}",
            BASE_WORKSPACE_CONFIG, config_flag
        );

        let mut expected_config = create_base_workspace_config();
        expected_config.workspace.packages_defaults.release = expected_value.into();

        let config: Config = toml::from_str(config).unwrap();
        assert_eq!(config, expected_config)
    }

    #[test]
    fn config_workspace_release_is_deserialized_true() {
        config_workspace_release_is_deserialized("true", true);
    }

    #[test]
    fn config_workspace_release_is_deserialized_false() {
        config_workspace_release_is_deserialized("false", false);
    }

    #[test]
    fn config_is_serialized() {
        let config = Config {
            workspace: Workspace {
                dependencies_update: None,
                changelog_config: Some("../git-cliff.toml".into()),
                allow_dirty: None,
                repo_url: Some("https://github.com/MarcoIeni/release-plz".parse().unwrap()),
                pr_draft: false,
                pr_labels: vec!["label1".to_string()],
                packages_defaults: PackageConfig {
                    semver_check: None,
                    changelog_update: true.into(),
                    git_release_enable: true.into(),
                    git_release_type: Some(ReleaseType::Prod),
                    git_release_draft: Some(false),
                    release: Some(true),
                    ..Default::default()
                },
                publish_timeout: Some("10m".to_string()),
            },
            package: [PackageSpecificConfigWithName {
                name: "crate1".to_string(),
                config: PackageSpecificConfig {
                    common: PackageConfig {
                        semver_check: Some(false),
                        changelog_update: true.into(),
                        git_release_enable: true.into(),
                        git_release_type: Some(ReleaseType::Prod),
                        git_release_draft: Some(false),
                        release: Some(false),
                        ..Default::default()
                    },
                    changelog_path: Some("./CHANGELOG.md".into()),
                    changelog_include: Some(vec!["pkg1".to_string()]),
                },
            }]
            .into(),
        };

        expect_test::expect![[r#"
            [workspace]
            changelog_update = true
            git_release_enable = true
            git_release_type = "prod"
            git_release_draft = false
            release = true
            changelog_config = "../git-cliff.toml"
            pr_draft = false
            pr_labels = ["label1"]
            publish_timeout = "10m"
            repo_url = "https://github.com/MarcoIeni/release-plz"

            [[package]]
            name = "crate1"
            changelog_update = true
            git_release_enable = true
            git_release_type = "prod"
            git_release_draft = false
            semver_check = false
            release = false
            changelog_path = "./CHANGELOG.md"
            changelog_include = ["pkg1"]
        "#]]
        .assert_eq(&toml::to_string(&config).unwrap());
    }
}
