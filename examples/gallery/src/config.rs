use rinui_host::{
    AppIdentity, AppJsonStore, ConfigError, ConfigLoadReport, ConfigLocation, TranslationManager,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GalleryConfig {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default, rename = "recentlyViewed")]
    pub recently_viewed: Vec<String>,
}

fn default_language() -> String {
    let sys = TranslationManager::system_locale();
    if sys.is_empty() {
        "en_US".to_owned()
    } else {
        sys
    }
}

impl Default for GalleryConfig {
    fn default() -> Self {
        Self {
            language: default_language(),
            favorites: Vec::new(),
            recently_viewed: Vec::new(),
        }
    }
}

pub fn load_gallery_config(
    location: &ConfigLocation,
    identity: &AppIdentity,
    qt_local_data: Option<&Path>,
) -> Result<(AppJsonStore<GalleryConfig>, ConfigLoadReport), ConfigError> {
    AppJsonStore::load_or_default(
        location,
        identity,
        qt_local_data,
        "config.json",
        GalleryConfig::default(),
    )
}

pub fn format_config_path(path: Option<&Path>) -> PathBuf {
    path.map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("<in-memory>"))
}
