use crate::error::ConfigError;
use crate::platform::{BackdropEffect, OsKind, PlatformCapabilities, ThemeMode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

/// Current `RinConfig` schema version written by `rinui-rs`.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Application identity used to isolate Qt `QStandardPaths` and configuration directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIdentity {
    pub organization_name: String,
    pub organization_domain: String,
    pub application_name: String,
    pub application_version: String,
}

impl AppIdentity {
    /// Create an application identity with explicit organization and application names.
    pub fn new(organization_name: impl Into<String>, application_name: impl Into<String>) -> Self {
        Self {
            organization_name: organization_name.into(),
            organization_domain: String::new(),
            application_name: application_name.into(),
            application_version: "0.1.0".to_owned(),
        }
    }

    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.organization_domain = domain.into();
        self
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.application_version = version.into();
        self
    }

    fn sanitized_org(&self) -> String {
        sanitize_path_segment(&self.organization_name, "RinUIHost")
    }

    fn sanitized_app(&self) -> String {
        sanitize_path_segment(&self.application_name, "App")
    }
}

/// Configuration storage policy for a host or consumer application.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConfigLocation {
    /// Platform standard per-application local data directory.
    #[default]
    Standard,
    /// Explicit base directory override (isolated per application or test run).
    CustomDir(PathBuf),
    /// In-memory configuration only; no disk reads or writes are performed.
    InMemory,
}

/// Nested `"theme"` section inside `rin_ui.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeSection {
    #[serde(default)]
    pub current_theme: ThemeMode,
}

impl Default for ThemeSection {
    fn default() -> Self {
        Self {
            current_theme: ThemeMode::Auto,
        }
    }
}

/// Nested `"win10_feat"` section inside `rin_ui.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Win10BackdropColors {
    #[serde(default = "default_win10_backdrop_light")]
    pub backdrop_light: u32,
    #[serde(default = "default_win10_backdrop_dark")]
    pub backdrop_dark: u32,
}

impl Default for Win10BackdropColors {
    fn default() -> Self {
        Self {
            backdrop_light: default_win10_backdrop_light(),
            backdrop_dark: default_win10_backdrop_dark(),
        }
    }
}

fn default_win10_backdrop_light() -> u32 {
    0xA6FF_FFFF
}

fn default_win10_backdrop_dark() -> u32 {
    0xA600_0000
}

fn default_theme_color() -> String {
    "#605ed2".to_owned()
}

/// Upstream-compatible `rin_ui.json` schema with forward-compatible `extra` field preservation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RinConfig {
    /// Schema version (`0` when deserialized from legacy Python `rin_ui.json`).
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub theme: ThemeSection,
    #[serde(default)]
    pub win10_feat: Win10BackdropColors,
    #[serde(default = "default_theme_color")]
    pub theme_color: String,
    #[serde(default)]
    pub backdrop_effect: BackdropEffect,
    /// Preserve unknown fields for forward/backward compatibility.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl RinConfig {
    /// Construct defaults tailored to `capabilities`.
    pub fn default_for(capabilities: PlatformCapabilities) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            theme: ThemeSection::default(),
            win10_feat: Win10BackdropColors::default(),
            theme_color: default_theme_color(),
            backdrop_effect: capabilities.default_backdrop(),
            extra: BTreeMap::new(),
        }
    }
}

impl Default for RinConfig {
    fn default() -> Self {
        Self::default_for(PlatformCapabilities::current())
    }
}

/// Outcome returned when loading a JSON configuration store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigLoadReport {
    Loaded,
    CreatedDefault,
    Migrated {
        from: u32,
        to: u32,
    },
    RecoveredFromCorruption {
        backup_path: Option<PathBuf>,
        reason: String,
    },
    InMemory,
}

/// Persistent manager for `rin_ui.json`.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: Option<PathBuf>,
    config: RinConfig,
    capabilities: PlatformCapabilities,
}

impl ConfigStore {
    /// Load `rin_ui.json` from the resolved path, creating defaults, migrating schema v0 -> v1,
    /// or recovering safely with a backup if the existing file is corrupted.
    pub fn load_or_default(
        location: &ConfigLocation,
        identity: &AppIdentity,
        qt_app_local_data: Option<&Path>,
        capabilities: PlatformCapabilities,
    ) -> Result<(Self, ConfigLoadReport), ConfigError> {
        let path = resolve_rinui_config_file(location, identity, qt_app_local_data);
        let Some(file_path) = path else {
            return Ok((
                Self {
                    path: None,
                    config: RinConfig::default_for(capabilities),
                    capabilities,
                },
                ConfigLoadReport::InMemory,
            ));
        };

        if !file_path.exists() {
            let config = RinConfig::default_for(capabilities);
            save_json_atomic(&file_path, &config)?;
            return Ok((
                Self {
                    path: Some(file_path),
                    config,
                    capabilities,
                },
                ConfigLoadReport::CreatedDefault,
            ));
        }

        let raw = match fs::read_to_string(&file_path) {
            Ok(content) => content,
            Err(err) => {
                warn!(path = %file_path.display(), error = %err, "Failed to read rin_ui.json; using defaults");
                let backup_path = backup_corrupt_file(&file_path);
                let config = RinConfig::default_for(capabilities);
                let _ = save_json_atomic(&file_path, &config);
                return Ok((
                    Self {
                        path: Some(file_path),
                        config,
                        capabilities,
                    },
                    ConfigLoadReport::RecoveredFromCorruption {
                        backup_path,
                        reason: err.to_string(),
                    },
                ));
            }
        };

        // Parse as generic Value first so partial/legacy/malformed fields don't lose valid sibling keys,
        // while invalid JSON triggers backup + default recovery.
        match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(serde_json::Value::Object(obj)) => {
                let raw_value = serde_json::Value::Object(obj);
                match parse_and_migrate_rin_config(raw_value, capabilities) {
                    Ok((config, migrated_from)) => {
                        let report = if let Some(from) = migrated_from {
                            save_json_atomic(&file_path, &config)?;
                            info!(
                                path = %file_path.display(),
                                from,
                                to = CURRENT_SCHEMA_VERSION,
                                "Migrated RinUI configuration schema"
                            );
                            ConfigLoadReport::Migrated {
                                from,
                                to: CURRENT_SCHEMA_VERSION,
                            }
                        } else {
                            ConfigLoadReport::Loaded
                        };
                        Ok((
                            Self {
                                path: Some(file_path),
                                config,
                                capabilities,
                            },
                            report,
                        ))
                    }
                    Err(reason) => {
                        let backup_path = backup_corrupt_file(&file_path);
                        let config = RinConfig::default_for(capabilities);
                        save_json_atomic(&file_path, &config)?;
                        warn!(
                            path = %file_path.display(),
                            reason = %reason,
                            "Recovered RinUI config from invalid schema"
                        );
                        Ok((
                            Self {
                                path: Some(file_path),
                                config,
                                capabilities,
                            },
                            ConfigLoadReport::RecoveredFromCorruption {
                                backup_path,
                                reason,
                            },
                        ))
                    }
                }
            }
            Ok(_) => {
                let reason = "top-level JSON value must be an object".to_owned();
                let backup_path = backup_corrupt_file(&file_path);
                let config = RinConfig::default_for(capabilities);
                save_json_atomic(&file_path, &config)?;
                warn!(path = %file_path.display(), "{reason}; backed up and restored defaults");
                Ok((
                    Self {
                        path: Some(file_path),
                        config,
                        capabilities,
                    },
                    ConfigLoadReport::RecoveredFromCorruption {
                        backup_path,
                        reason,
                    },
                ))
            }
            Err(err) => {
                let reason = err.to_string();
                let backup_path = backup_corrupt_file(&file_path);
                let config = RinConfig::default_for(capabilities);
                save_json_atomic(&file_path, &config)?;
                warn!(
                    path = %file_path.display(),
                    error = %reason,
                    "Corrupted rin_ui.json backed up and replaced with defaults"
                );
                Ok((
                    Self {
                        path: Some(file_path),
                        config,
                        capabilities,
                    },
                    ConfigLoadReport::RecoveredFromCorruption {
                        backup_path,
                        reason,
                    },
                ))
            }
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn capabilities(&self) -> PlatformCapabilities {
        self.capabilities
    }

    pub fn config(&self) -> &RinConfig {
        &self.config
    }

    pub fn theme(&self) -> ThemeMode {
        self.config.theme.current_theme
    }

    pub fn set_theme(&mut self, theme: ThemeMode) -> Result<(), ConfigError> {
        self.config.theme.current_theme = theme;
        self.save()
    }

    pub fn theme_color(&self) -> &str {
        &self.config.theme_color
    }

    pub fn set_theme_color(&mut self, color: &str) -> Result<(), ConfigError> {
        let trimmed = color.trim();
        if let Some(normalized) = normalize_theme_color(trimmed) {
            self.config.theme_color = normalized;
        } else if !trimmed.is_empty() {
            // Preserve non-empty color strings if QML passes Qt-named colors (e.g. "red")
            self.config.theme_color = trimmed.to_owned();
        } else {
            return Ok(());
        }
        self.save()
    }

    pub fn backdrop_effect(&self) -> BackdropEffect {
        self.config.backdrop_effect
    }

    pub fn set_backdrop_effect(&mut self, effect: BackdropEffect) -> Result<(), ConfigError> {
        self.config.backdrop_effect = effect;
        self.save()
    }

    pub fn win10_backdrop_colors(&self) -> &Win10BackdropColors {
        &self.config.win10_feat
    }

    /// Persist the current configuration atomically (no-op for `ConfigLocation::InMemory`).
    pub fn save(&self) -> Result<(), ConfigError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        save_json_atomic(path, &self.config)
    }
}

/// Generic atomic JSON configuration store for consumer applications (e.g. Gallery `config.json`).
#[derive(Debug, Clone)]
pub struct AppJsonStore<T> {
    path: Option<PathBuf>,
    value: T,
}

impl<T> AppJsonStore<T>
where
    T: Serialize + DeserializeOwned + Clone,
{
    /// Load `filename` under `<app_base_dir>/config/<filename>` or fall back to `default_value`.
    pub fn load_or_default(
        location: &ConfigLocation,
        identity: &AppIdentity,
        qt_app_local_data: Option<&Path>,
        filename: &str,
        default_value: T,
    ) -> Result<(Self, ConfigLoadReport), ConfigError> {
        let path = resolve_app_config_file(location, identity, qt_app_local_data, filename);
        let Some(file_path) = path else {
            return Ok((
                Self {
                    path: None,
                    value: default_value,
                },
                ConfigLoadReport::InMemory,
            ));
        };

        if !file_path.exists() {
            save_json_atomic(&file_path, &default_value)?;
            return Ok((
                Self {
                    path: Some(file_path),
                    value: default_value,
                },
                ConfigLoadReport::CreatedDefault,
            ));
        }

        match fs::read_to_string(&file_path)
            .map_err(|e| e.to_string())
            .and_then(|raw| serde_json::from_str::<T>(&raw).map_err(|e| e.to_string()))
        {
            Ok(parsed) => Ok((
                Self {
                    path: Some(file_path),
                    value: parsed,
                },
                ConfigLoadReport::Loaded,
            )),
            Err(reason) => {
                let backup_path = backup_corrupt_file(&file_path);
                save_json_atomic(&file_path, &default_value)?;
                warn!(
                    path = %file_path.display(),
                    reason = %reason,
                    "Corrupted app config backed up and reset to defaults"
                );
                Ok((
                    Self {
                        path: Some(file_path),
                        value: default_value,
                    },
                    ConfigLoadReport::RecoveredFromCorruption {
                        backup_path,
                        reason,
                    },
                ))
            }
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    pub fn update(&mut self, f: impl FnOnce(&mut T)) -> Result<(), ConfigError> {
        f(&mut self.value);
        self.save()
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        save_json_atomic(path, &self.value)
    }
}

/// Resolve the isolated per-application base directory.
///
/// Each consumer application MUST supply its own `AppIdentity` so different applications
/// never share the same configuration directory.
pub fn resolve_app_base_dir(identity: &AppIdentity, qt_app_local_data: Option<&Path>) -> PathBuf {
    if let Some(qt_dir) = qt_app_local_data {
        if !qt_dir.as_os_str().is_empty() {
            return qt_dir.to_path_buf();
        }
    }

    let org = identity.sanitized_org();
    let app = identity.sanitized_app();

    let os_base = match OsKind::current() {
        OsKind::MacOs => home_dir()
            .map(|h| h.join("Library").join("Application Support"))
            .unwrap_or_else(|| PathBuf::from(".rinui")),
        OsKind::Windows => std::env::var_os("LOCALAPPDATA")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join("AppData").join("Local")))
            .unwrap_or_else(|| PathBuf::from(".rinui")),
        OsKind::Linux | OsKind::Other => std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join(".local").join("share")))
            .unwrap_or_else(|| PathBuf::from(".rinui")),
    };

    os_base.join(org).join(app)
}

/// Resolve `<base>/RinUI/config/rin_ui.json` for a given `ConfigLocation` and `AppIdentity`.
pub fn resolve_rinui_config_file(
    location: &ConfigLocation,
    identity: &AppIdentity,
    qt_app_local_data: Option<&Path>,
) -> Option<PathBuf> {
    match location {
        ConfigLocation::InMemory => None,
        ConfigLocation::CustomDir(dir) => {
            Some(dir.join("RinUI").join("config").join("rin_ui.json"))
        }
        ConfigLocation::Standard => Some(
            resolve_app_base_dir(identity, qt_app_local_data)
                .join("RinUI")
                .join("config")
                .join("rin_ui.json"),
        ),
    }
}

/// Resolve `<base>/config/<filename>` for application-owned config files.
pub fn resolve_app_config_file(
    location: &ConfigLocation,
    identity: &AppIdentity,
    qt_app_local_data: Option<&Path>,
    filename: &str,
) -> Option<PathBuf> {
    match location {
        ConfigLocation::InMemory => None,
        ConfigLocation::CustomDir(dir) => Some(dir.join("config").join(filename)),
        ConfigLocation::Standard => Some(
            resolve_app_base_dir(identity, qt_app_local_data)
                .join("config")
                .join(filename),
        ),
    }
}

/// Normalize hex colors (`#RGB`, `#RRGGBB`, `#AARRGGBB`) when valid.
pub fn normalize_theme_color(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let hex = trimmed.strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        3 | 6 | 8 => Some(format!("#{}", hex.to_ascii_lowercase())),
        _ => None,
    }
}

fn parse_and_migrate_rin_config(
    mut value: serde_json::Value,
    capabilities: PlatformCapabilities,
) -> Result<(RinConfig, Option<u32>), String> {
    let Some(obj) = value.as_object_mut() else {
        return Err("expected JSON object".to_owned());
    };

    let existing_schema = obj
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(0);

    // Fill missing optional sections with platform-appropriate defaults before deserializing.
    if !obj.contains_key("backdrop_effect") {
        obj.insert(
            "backdrop_effect".to_owned(),
            serde_json::Value::String(capabilities.default_backdrop().as_str().to_owned()),
        );
    }

    if !obj.contains_key("theme_color") {
        obj.insert(
            "theme_color".to_owned(),
            serde_json::Value::String(default_theme_color()),
        );
    }

    // Coerce invalid/non-string theme or backdrop fields safely to defaults rather than crashing.
    let mut sanitized_migration = false;
    if let Some(theme_val) = obj.get_mut("theme") {
        if let Some(theme_obj) = theme_val.as_object_mut() {
            if let Some(ct) = theme_obj.get("current_theme").and_then(|v| v.as_str()) {
                if ThemeMode::parse_str(ct).is_err() {
                    theme_obj.insert(
                        "current_theme".to_owned(),
                        serde_json::Value::String(ThemeMode::Auto.as_str().to_owned()),
                    );
                    sanitized_migration = true;
                }
            }
        }
    }

    if let Some(backdrop_str) = obj.get("backdrop_effect").and_then(|v| v.as_str()) {
        if BackdropEffect::parse_str(backdrop_str).is_err() {
            obj.insert(
                "backdrop_effect".to_owned(),
                serde_json::Value::String(capabilities.default_backdrop().as_str().to_owned()),
            );
            sanitized_migration = true;
        }
    }

    let mut config: RinConfig = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let migrated_from = if existing_schema < CURRENT_SCHEMA_VERSION || sanitized_migration {
        config.schema_version = CURRENT_SCHEMA_VERSION;
        Some(existing_schema)
    } else {
        None
    };

    Ok((config, migrated_from))
}

/// Write JSON atomically using a temporary sibling file, `sync_all`, and rename.
pub fn save_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), ConfigError> {
    let parent = path.parent().ok_or_else(|| {
        ConfigError::InvalidPath(format!("path '{}' has no parent directory", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|source| ConfigError::Io {
        path: parent.to_path_buf(),
        source,
    })?;

    let payload = serde_json::to_vec_pretty(value)?;
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config.json");
    let tmp_path = parent.join(format!(".{file_name}.tmp.{pid}.{seq}"));

    {
        let mut tmp_file = File::create(&tmp_path).map_err(|source| ConfigError::Io {
            path: tmp_path.clone(),
            source,
        })?;
        tmp_file
            .write_all(&payload)
            .map_err(|source| ConfigError::Io {
                path: tmp_path.clone(),
                source,
            })?;
        tmp_file
            .write_all(b"\n")
            .map_err(|source| ConfigError::Io {
                path: tmp_path.clone(),
                source,
            })?;
        tmp_file.sync_all().map_err(|source| ConfigError::Io {
            path: tmp_path.clone(),
            source,
        })?;
    }

    if let Err(err) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(ConfigError::Io {
            path: path.to_path_buf(),
            source: err,
        });
    }

    Ok(())
}

fn backup_corrupt_file(path: &Path) -> Option<PathBuf> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config.json");
    let mut candidate = path.with_file_name(format!("{file_name}.corrupt.{ts}.bak"));
    let mut counter = 1u32;
    while candidate.exists() {
        candidate = path.with_file_name(format!("{file_name}.corrupt.{ts}.{counter}.bak"));
        counter += 1;
    }
    match fs::rename(path, &candidate) {
        Ok(()) => Some(candidate),
        Err(_) => None,
    }
}

fn sanitize_path_segment(raw: &str, fallback: &str) -> String {
    let filtered: String = raw
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    if filtered.is_empty() || filtered == "." || filtered == ".." {
        fallback.to_owned()
    } else {
        filtered
    }
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }
}
