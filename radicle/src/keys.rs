#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub secret: std::path::PathBuf,
    pub public: std::path::PathBuf,
}
