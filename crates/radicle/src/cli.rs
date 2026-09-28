/// CLI configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "schemars",
    derive(schemars::JsonSchema),
    schemars(rename = "CliConfig")
)]
pub struct Config {
    /// Whether to show hints or not in the CLI.
    #[serde(default)]
    pub hints: bool,
    /// Issue import/export configuration.
    #[serde(default)]
    pub issues: Issues,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hints: true,
            issues: Issues::default(),
        }
    }
}

/// Issue import/export CLI configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Issues {
    /// Directory under the repository root where issue markdown files are stored.
    #[serde(default = "Issues::default_directory")]
    pub directory: String,
}

impl Issues {
    fn default_directory() -> String {
        "issues".to_owned()
    }
}

impl Default for Issues {
    fn default() -> Self {
        Self {
            directory: Self::default_directory(),
        }
    }
}
