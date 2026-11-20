use crate::bridge::ffi as qt_bridge;
use crate::error::HostError;
use crate::resources::ResourceView;
use cxx_qt_lib::{QQmlApplicationEngine, QString, QStringList};
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

/// Per-catalog translation resolution and load status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogTranslationResult {
    pub catalog_id: String,
    pub selected_locale: Option<String>,
    pub loaded_path: Option<String>,
    pub loaded: bool,
    pub used_fallback: bool,
}

/// Summary returned after loading or switching translations.
///
/// Since `0.1.0-alpha.2`, [`Self::effective_locale`] tracks [`Self::active_locale`]
/// (the canonical application locale committed to Qt's default `QLocale`), decoupled from
/// whether RinUI's core catalog fell back to `en_US`. Inspect [`Self::rinui_locale`] /
/// [`Self::rinui_catalog`] for RinUI's loaded catalog locale and [`Self::app_catalogs`] for
/// per-catalog selections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationLoadResult {
    pub requested_locale: String,
    /// Active application locale (`== active_locale`).
    pub effective_locale: String,
    /// Canonical active application locale committed to `QLocale::setDefault()`.
    pub active_locale: String,
    /// Actual locale loaded by the RinUI core translation catalog (may fall back to `"en_US"`).
    pub rinui_locale: String,
    /// `true` when active locale, RinUI catalog, or any app catalog used a fallback.
    pub used_fallback: bool,
    /// `true` when RinUI core catalog fell back to `"en_US"`.
    pub rinui_used_fallback: bool,
    /// `true` when the requested locale tag was unrecognized and active locale fell back to `"en_US"`.
    pub active_locale_fallback: bool,
    pub rinui_loaded: bool,
    pub rinui_catalog: CatalogTranslationResult,
    pub app_catalogs: Vec<CatalogTranslationResult>,
    /// Legacy `(catalog_id, loaded)` summary matching `app_catalogs`.
    pub app_catalogs_loaded: Vec<(String, bool)>,
}

/// Manages Qt `QTranslator` instances for RinUI and consumer-registered catalogs.
pub struct TranslationManager {
    resources: ResourceView,
    app_catalogs: Vec<TranslationCatalog>,
    current_locale: String,
}

impl TranslationManager {
    pub(crate) fn new(resources: ResourceView) -> Self {
        let system_locale = Self::system_locale();
        let canonical = canonicalize_or_default(&system_locale);
        Self {
            resources,
            app_catalogs: Vec::new(),
            current_locale: canonical,
        }
    }

    /// Detect the operating system's current Qt locale name (e.g. `"en_US"`, `"zh_CN"`).
    pub fn system_locale() -> String {
        qt_bridge::rinui_system_locale_name().to_string()
    }

    /// Query Qt's current process-default `QLocale().name()`.
    pub fn default_qt_locale() -> String {
        qt_bridge::rinui_default_locale_name().to_string()
    }

    /// Currently active application locale string.
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

    /// Register/update `catalog` and immediately apply the current active locale,
    /// rolling back the catalog list if loading fails.
    pub(crate) fn register_and_apply(
        &mut self,
        catalog: TranslationCatalog,
        engine: Option<Pin<&mut QQmlApplicationEngine>>,
    ) -> Result<TranslationLoadResult, HostError> {
        let prev_catalogs = self.app_catalogs.clone();
        self.register_catalog(catalog);
        let locale = self.current_locale.clone();
        match self.load_locale(&locale, engine) {
            Ok(res) => Ok(res),
            Err(err) => {
                self.app_catalogs = prev_catalogs;
                Err(err)
            }
        }
    }

    /// Load RinUI + registered application catalogs for `locale` and optionally retranslate
    /// a live `QQmlApplicationEngine`.
    ///
    /// RinUI's `en_US` catalog fallback does not override a valid requested application locale.
    pub(crate) fn load_locale(
        &mut self,
        locale: &str,
        engine: Option<Pin<&mut QQmlApplicationEngine>>,
    ) -> Result<TranslationLoadResult, HostError> {
        let trimmed = locale.trim();
        let raw_input = if trimmed.is_empty() {
            Self::system_locale()
        } else {
            trimmed.to_owned()
        };
        let requested = normalize_locale_tag(&raw_input);
        let canonical =
            qt_bridge::rinui_canonicalize_locale_name(&QString::from(raw_input.as_str()))
                .to_string();

        let (active_locale, active_locale_fallback) = if !canonical.is_empty() {
            (canonical.clone(), false)
        } else {
            warn!(
                requested = %requested,
                "Unrecognized locale tag; falling back active locale to en_US"
            );
            ("en_US".to_owned(), true)
        };

        let candidates = if active_locale_fallback {
            vec!["en_US".to_owned()]
        } else {
            locale_candidates(&requested, &active_locale)
        };

        let mut rinui_selected: Option<(String, String)> = None;
        for cand in &candidates {
            let path = self.resources.rinui_qm_path(cand);
            if qt_bridge::rinui_qt_file_exists(&QString::from(path.as_str())) {
                rinui_selected = Some((cand.clone(), path));
                break;
            }
        }

        let (rinui_locale, rinui_qm, rinui_used_fallback) =
            if let Some((cand, path)) = rinui_selected {
                (cand, path, false)
            } else {
                let fallback_qm = self.resources.rinui_qm_path("en_US");
                warn!(
                    requested = %requested,
                    active_locale = %active_locale,
                    fallback = %fallback_qm,
                    "RinUI translation file missing; falling back RinUI catalog to en_US"
                );
                ("en_US".to_owned(), fallback_qm, true)
            };

        let mut pending_apps = Vec::with_capacity(self.app_catalogs.len());
        let mut app_ids = QStringList::default();
        let mut app_paths = QStringList::default();

        for catalog in &self.app_catalogs {
            let mut resolved_cand: Option<(String, String, bool)> = None;
            for cand in &candidates {
                let candidate_qm = catalog.resolve_qm_path(cand);
                if qt_bridge::rinui_qt_file_exists(&QString::from(candidate_qm.as_str())) {
                    resolved_cand = Some((cand.clone(), candidate_qm, false));
                    break;
                }
            }
            if resolved_cand.is_none() && !candidates.iter().any(|c| c == "en_US") {
                let fallback_app_qm = catalog.resolve_qm_path("en_US");
                if qt_bridge::rinui_qt_file_exists(&QString::from(fallback_app_qm.as_str())) {
                    resolved_cand = Some(("en_US".to_owned(), fallback_app_qm, true));
                }
            }

            app_ids.append(QString::from(catalog.id()));
            match resolved_cand {
                Some((selected_loc, qm_path, used_fb)) => {
                    app_paths.append(QString::from(qm_path.as_str()));
                    pending_apps.push(CatalogTranslationResult {
                        catalog_id: catalog.id().to_owned(),
                        selected_locale: Some(selected_loc),
                        loaded_path: Some(qm_path),
                        loaded: true,
                        used_fallback: used_fb,
                    });
                }
                None => {
                    debug!(
                        catalog = %catalog.id(),
                        active_locale = %active_locale,
                        "Optional app translation catalog has no matching or en_US .qm; unloading"
                    );
                    app_paths.append(QString::default());
                    pending_apps.push(CatalogTranslationResult {
                        catalog_id: catalog.id().to_owned(),
                        selected_locale: None,
                        loaded_path: None,
                        loaded: false,
                        used_fallback: false,
                    });
                }
            }
        }

        let mut failed_path = QString::default();
        let ok = qt_bridge::rinui_translator_apply_transaction(
            &QString::from(active_locale.as_str()),
            &QString::from(rinui_qm.as_str()),
            &app_ids,
            &app_paths,
            &mut failed_path,
        );
        if !ok {
            let path_str = failed_path.to_string();
            return Err(HostError::TranslationLoadFailed {
                path: if path_str.is_empty() {
                    rinui_qm
                } else {
                    path_str
                },
            });
        }

        self.current_locale = active_locale.clone();
        if let Some(eng) = engine {
            qt_bridge::rinui_engine_retranslate(eng);
        }

        let app_catalogs_loaded: Vec<(String, bool)> = pending_apps
            .iter()
            .map(|c| (c.catalog_id.clone(), c.loaded))
            .collect();
        let any_app_fallback = pending_apps.iter().any(|c| c.used_fallback);
        let used_fallback = rinui_used_fallback || active_locale_fallback || any_app_fallback;

        let rinui_catalog = CatalogTranslationResult {
            catalog_id: "rinui".to_owned(),
            selected_locale: Some(rinui_locale.clone()),
            loaded_path: Some(rinui_qm),
            loaded: true,
            used_fallback: rinui_used_fallback,
        };

        Ok(TranslationLoadResult {
            requested_locale: requested,
            effective_locale: active_locale.clone(),
            active_locale,
            rinui_locale,
            used_fallback,
            rinui_used_fallback,
            active_locale_fallback,
            rinui_loaded: true,
            rinui_catalog,
            app_catalogs: pending_apps,
            app_catalogs_loaded,
        })
    }
}

fn canonicalize_or_default(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "en_US".to_owned();
    }
    let canonical = qt_bridge::rinui_canonicalize_locale_name(&QString::from(trimmed)).to_string();
    if canonical.is_empty() {
        "en_US".to_owned()
    } else {
        canonical
    }
}

fn normalize_locale_tag(raw: &str) -> String {
    let before_dot = raw.split('.').next().unwrap_or(raw);
    let before_at = before_dot.split('@').next().unwrap_or(before_dot);
    before_at.trim().replace('-', "_")
}

fn locale_candidates(normalized: &str, canonical: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut push_unique = |s: &str| {
        if !s.is_empty() && !out.iter().any(|existing| existing == s) {
            out.push(s.to_owned());
        }
    };

    push_unique(normalized);
    push_unique(canonical);

    let parts: Vec<&str> = normalized.split('_').filter(|s| !s.is_empty()).collect();
    if parts.len() >= 3 {
        let lang_region = format!("{}_{}", parts[0], parts[parts.len() - 1]);
        push_unique(&lang_region);
    }
    if parts.len() == 1 {
        let expanded = match parts[0] {
            "zh" => Some("zh_CN"),
            "en" => Some("en_US"),
            _ => None,
        };
        if let Some(tag) = expanded {
            push_unique(tag);
        }
    }

    let canon_parts: Vec<&str> = canonical.split('_').filter(|s| !s.is_empty()).collect();
    if canon_parts.len() >= 2 {
        push_unique(canon_parts[0]);
    }
    if parts.len() >= 2 {
        push_unique(parts[0]);
    }

    out
}
