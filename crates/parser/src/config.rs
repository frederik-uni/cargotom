use info_provider::api::ViewMode;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_per_page")]
    pub per_page: usize,

    /// All, UnusedOpt, Features,
    #[serde(default = "default_feature_display_mode")]
    pub feature_display_mode: ViewMode,

    #[serde(default)]
    pub hide_docs_info_message: bool,

    #[serde(default)]
    pub sort_format: bool,

    #[serde(default = "default_true")]
    pub offline: bool,

    #[serde(default = "default_true")]
    pub stable_version: bool,
}

#[derive(Deserialize, Serialize)]
pub struct ConfigV2 {
    pub online: OnlineConfig,
    pub offline: OfflineConfig,
    pub display: DisplayConfig,
    pub general: GeneralConfig,
}

#[derive(Deserialize, Serialize)]
pub struct GeneralConfig {
    #[serde(default = "default_true")]
    pub stable_version: bool,
    #[serde(default)]
    pub sort_format: bool,
}

#[derive(Deserialize, Serialize)]
pub struct OnlineConfig {
    #[serde(default = "default_per_page")]
    pub per_page: usize,
}

#[derive(Deserialize, Serialize)]
pub struct OfflineConfig {
    #[serde(default = "default_true")]
    pub offline: bool,
}

#[derive(Deserialize, Serialize)]
pub struct DisplayConfig {
    /// All, UnusedOpt, Features,
    #[serde(default = "default_feature_display_mode")]
    pub feature_display_mode: ViewMode,
    #[serde(default)]
    pub hide_docs_info_message: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            per_page: default_per_page(),
            feature_display_mode: default_feature_display_mode(),
            hide_docs_info_message: false,
            sort_format: false,
            stable_version: true,
            offline: true,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_per_page() -> usize {
    25
}

fn default_feature_display_mode() -> ViewMode {
    ViewMode::UnusedOpt
}
