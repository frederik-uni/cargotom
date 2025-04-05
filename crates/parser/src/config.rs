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
    pub stable_version: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            per_page: default_per_page(),
            feature_display_mode: default_feature_display_mode(),
            hide_docs_info_message: false,
            sort_format: false,
            stable_version: true,
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
