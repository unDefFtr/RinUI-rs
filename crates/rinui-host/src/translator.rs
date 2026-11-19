use crate::bridge::ffi as qt_bridge;
use crate::error::HostError;
use crate::resources::ResolvedResources;
use cxx_qt_lib::{QQmlApplicationEngine, QString};
use std::path::PathBuf;
use std::pin::Pin;
use tracing::{debug, warn};

/// Application-owned Qt translation catalog (`.qm` files).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationCatalog {
    /// Load `<prefix>/<locale>.qm` from compiled Qt resources (e.g. `":/gallery/languages"`).
    QrcPrefix { id: String, prefix: String },
    /// Load `<dir>/<locale>.qm` from a directory on disk.
    Directory { id: String, dir: PathBuf },
}

impl TranslationCatalog {
    pub fn id(&self) -> &str {
        match self {
            Self::QrcPrefix { id, .. } | Self::Directory { id, .. } => id,
        }
    }

    pub fn resolve_qm_path(&self, locale: &str) -> String {
        match self {
            Self::QrcPrefix { prefix, .. } => {
                let trimmed = prefix.trim_end_matches('/');
                format!("{trimmed}/{locale}.qm")
            }
            Self::Directory { dir, .. } => dir
                .join(format!("{locale}.qm"))
                .to_string_lossy()
                .into_owned(),
        }
    }
}

/// Summary returned after loading or switching translations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationLoadResult {
    pub requested_locale: String,
    pub effective_locale: String,
    pub used_fallback: bool,
    pub rinui_loaded: bool,
    pub app_catalogs_loaded: Vec<(String, bool)>,
}

/// Manages Qt `QTranslator` instances for RinUI and consumer-registered catalogs.
pub struct TranslationManager {
    resources: ResolvedResources,
    app_catalogs: Vec<TranslationCatalog>,
    current_locale: String,
}

impl TranslationManager {
    pub(crate) fn new(resources: ResolvedResources) -> Self {
        let system_locale = qt_bridge::rinui_system_locale_name().to_string();
        Self {
            resources,
            app_catalogs: Vec::new(),
            current_locale: if system_locale.is_empty() {
                "en_US".to_owned()
            } else {
                system_locale
            },
        }
    }

    /// Detect the operating system's current Qt locale name (e.g. `"en_US"`, `"zh_CN"`).
    pub fn system_locale() -> String {
        qt_bridge::rinui_system_locale_name().to_string()
    }

    /// Currently active locale string.
    pub fn current_locale(&self) -> &str {
        &self.current_locale
    }

    /// Register or update a consumer application translation catalog.
    pub fn register_catalog(&mut self, catalog: TranslationCatalog) {
        if let Some(existing) = self
            .app_catalogs
            .iter_mut()
            .find(|c| c.id() == catalog.id())
        {
            *existing = catalog;
        } else {
            self.app_catalogs.push(catalog);
        }
    }

    /// Load RinUI + registered application catalogs for `locale` (with `en_US` fallback)
    /// and optionally retranslate a live `QQmlApplicationEngine`.
    pub(crate) fn load_locale(
        &mut self,
        locale: &str,
        engine: Option<Pin<&mut QQmlApplicationEngine>>,
    ) -> Result<TranslationLoadResult, HostError> {
        let trimmed = locale.trim();
        let requested = if trimmed.is_empty() {
            normalize_locale_tag(&Self::system_locale())
        } else {
            normalize_locale_tag(trimmed)
        };

        let candidates = locale_candidates(&requested);
        let mut selected: Option<(String, String)> = None;
        for cand in &candidates {
            let path = self.resources.rinui_qm_path(cand);
            if qt_bridge::rinui_qt_file_exists(&QString::from(path.as_str())) {
                selected = Some((cand.clone(), path));
                break;
            }
        }

        let (effective, rinui_qm, used_fallback) = if let Some((cand, path)) = selected {
            (cand, path, false)
        } else {
            let fallback_qm = self.resources.rinui_qm_path("en_US");
            warn!(
                requested = %requested,
                fallback = %fallback_qm,
                "RinUI translation file missing; falling back to en_US"
            );
            ("en_US".to_owned(), fallback_qm, true)
        };

        let rinui_loaded = qt_bridge::rinui_translator_load_rinui(
            &QString::from(rinui_qm.as_str()),
            &QString::from(effective.as_str()),
        );
        if !rinui_loaded {
            return Err(HostError::TranslationLoadFailed { path: rinui_qm });
        }

        let mut app_catalogs_loaded = Vec::with_capacity(self.app_catalogs.len());
        for catalog in &self.app_catalogs {
            let mut resolved_app_qm = None;
            for cand in &candidates {
                let candidate_qm = catalog.resolve_qm_path(cand);
                if qt_bridge::rinui_qt_file_exists(&QString::from(candidate_qm.as_str())) {
                    resolved_app_qm = Some(candidate_qm);
                    break;
                }
            }
            if resolved_app_qm.is_none() {
                let fallback_app_qm = catalog.resolve_qm_path("en_US");
                if qt_bridge::rinui_qt_file_exists(&QString::from(fallback_app_qm.as_str())) {
                    resolved_app_qm = Some(fallback_app_qm);
                }
            }

            let ok = if let Some(app_qm) = resolved_app_qm {
                let loaded = qt_bridge::rinui_translator_load_app(
                    &QString::from(catalog.id()),
                    &QString::from(app_qm.as_str()),
                );
                if !loaded {
                    debug!(
                        catalog = %catalog.id(),
                        path = %app_qm,
                        "Optional app translation catalog failed to load"
                    );
                }
                loaded
            } else {
                qt_bridge::rinui_translator_unload_app(&QString::from(catalog.id()));
                false
            };
            app_catalogs_loaded.push((catalog.id().to_owned(), ok));
        }

        self.current_locale = effective.clone();
        if let Some(eng) = engine {
            qt_bridge::rinui_engine_retranslate(eng);
        }

        Ok(TranslationLoadResult {
            requested_locale: requested,
            effective_locale: effective,
            used_fallback,
            rinui_loaded,
            app_catalogs_loaded,
        })
    }
}

fn normalize_locale_tag(raw: &str) -> String {
    let base = raw.split('.').next().unwrap_or(raw).trim();
    base.replace('-', "_")
}

fn locale_candidates(normalized: &str) -> Vec<String> {
    let mut out = Vec::new();
    if !normalized.is_empty() {
        out.push(normalized.to_owned());
    }
    let parts: Vec<&str> = normalized.split('_').filter(|s| !s.is_empty()).collect();
    if parts.len() >= 3 {
        let lang_region = format!("{}_{}", parts[0], parts[parts.len() - 1]);
        if !out.contains(&lang_region) {
            out.push(lang_region);
        }
    }
    if parts.len() == 1 {
        let expanded = match parts[0] {
            "zh" => Some("zh_CN"),
            "en" => Some("en_US"),
            _ => None,
        };
        if let Some(tag) = expanded {
            if !out.iter().any(|item| item == tag) {
                out.push(tag.to_owned());
            }
        }
    }
    out
}
