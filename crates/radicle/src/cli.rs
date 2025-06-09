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
}

impl Default for Config {
    fn default() -> Self {
        Self { hints: true }
    }
}
