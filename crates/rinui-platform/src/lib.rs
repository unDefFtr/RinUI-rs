//! Platform capabilities, OS/Windows version detection, and atomic configuration persistence.

pub mod config;
pub mod error;
pub mod platform;

pub use config::{
    normalize_theme_color, resolve_app_base_dir, resolve_app_config_file,
    resolve_rinui_config_file, save_json_atomic, AppIdentity, AppJsonStore, ConfigLoadReport,
    ConfigLocation, ConfigStore, RinConfig, ThemeSection, Win10BackdropColors,
    CURRENT_SCHEMA_VERSION,
};
pub use error::ConfigError;
pub use platform::{
    BackdropEffect, BackdropSupportError, OsKind, PlatformCapabilities, ResolvedTheme, ThemeMode,
    WindowsVersion,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn windows_capabilities_and_fallbacks_match_upstream() {
        let win11_22h2 = PlatformCapabilities::for_os(
            OsKind::Windows,
            Some(WindowsVersion {
                major: 10,
                minor: 0,
                build: 22621,
            }),
        );
        assert_eq!(win11_22h2.default_backdrop(), BackdropEffect::Mica);
        assert_eq!(
            win11_22h2.resolve_backdrop(BackdropEffect::Tabbed),
            Ok(BackdropEffect::Tabbed)
        );
        assert!(win11_22h2.supports_snap_layouts());

        let win11_21h2 = PlatformCapabilities::for_os(
            OsKind::Windows,
            Some(WindowsVersion {
                major: 10,
                minor: 0,
                build: 22000,
            }),
        );
        assert_eq!(
            win11_21h2.resolve_backdrop(BackdropEffect::Acrylic),
            Ok(BackdropEffect::Mica)
        );
        assert_eq!(
            win11_21h2.resolve_backdrop(BackdropEffect::Tabbed),
            Ok(BackdropEffect::Mica)
        );

        let win10 = PlatformCapabilities::for_os(
            OsKind::Windows,
            Some(WindowsVersion {
                major: 10,
                minor: 0,
                build: 19045,
            }),
        );
        assert_eq!(win10.default_backdrop(), BackdropEffect::Acrylic);
        assert_eq!(
            win10.resolve_backdrop(BackdropEffect::Mica),
            Err(BackdropSupportError::UnsupportedEffect(
                BackdropEffect::Mica
            ))
        );
        assert!(!win10.supports_snap_layouts());

        let mac = PlatformCapabilities::for_os(OsKind::MacOs, None);
        assert_eq!(mac.default_backdrop(), BackdropEffect::None);
        assert_eq!(
            mac.resolve_backdrop(BackdropEffect::Mica),
            Err(BackdropSupportError::PlatformUnavailable)
        );
        assert!(mac.supports_native_mac_frame());
    }

    #[test]
    fn different_consumers_use_isolated_directories() {
        let app_a = AppIdentity::new("Acme", "Goosy");
        let app_b = AppIdentity::new("RinUI", "Gallery");
        let dir_a = resolve_app_base_dir(&app_a, None);
        let dir_b = resolve_app_base_dir(&app_b, None);
        assert_ne!(dir_a, dir_b);

        let cfg_a = resolve_rinui_config_file(&ConfigLocation::Standard, &app_a, None).unwrap();
        let cfg_b = resolve_rinui_config_file(&ConfigLocation::Standard, &app_b, None).unwrap();
        assert_ne!(cfg_a, cfg_b);
        assert!(cfg_a.ends_with("RinUI/config/rin_ui.json"));
    }

    #[test]
    fn creates_default_and_round_trips_persisted_values() {
        let tmp = TempDir::new().unwrap();
        let identity = AppIdentity::new("TestOrg", "TestApp");
        let caps = PlatformCapabilities::for_os(OsKind::MacOs, None);
        let loc = ConfigLocation::CustomDir(tmp.path().to_path_buf());

        let (mut store, report) =
            ConfigStore::load_or_default(&loc, &identity, None, caps).unwrap();
        assert_eq!(report, ConfigLoadReport::CreatedDefault);
        assert_eq!(store.theme(), ThemeMode::Auto);
        assert_eq!(store.theme_color(), "#605ed2");
        assert_eq!(store.backdrop_effect(), BackdropEffect::None);

        store.set_theme(ThemeMode::Dark).unwrap();
        store.set_theme_color("#AABBCC").unwrap();
        store.set_backdrop_effect(BackdropEffect::Acrylic).unwrap();

        let (reloaded, report2) =
            ConfigStore::load_or_default(&loc, &identity, None, caps).unwrap();
        assert_eq!(report2, ConfigLoadReport::Loaded);
        assert_eq!(reloaded.theme(), ThemeMode::Dark);
        assert_eq!(reloaded.theme_color(), "#aabbcc");
        assert_eq!(reloaded.backdrop_effect(), BackdropEffect::Acrylic);
    }

    #[test]
    fn migrates_legacy_python_config_and_preserves_extra_fields() {
        let tmp = TempDir::new().unwrap();
        let identity = AppIdentity::new("Legacy", "App");
        let caps = PlatformCapabilities::for_os(OsKind::Linux, None);
        let loc = ConfigLocation::CustomDir(tmp.path().to_path_buf());
        let file = resolve_rinui_config_file(&loc, &identity, None).unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();

        let legacy_json = r##"{
            "theme": { "current_theme": "Dark" },
            "win10_feat": { "backdrop_light": 2801795071, "backdrop_dark": 2785017856 },
            "theme_color": "#112233",
            "backdrop_effect": "none",
            "custom_user_flag": true
        }"##;
        fs::write(&file, legacy_json).unwrap();

        let (store, report) = ConfigStore::load_or_default(&loc, &identity, None, caps).unwrap();
        assert_eq!(
            report,
            ConfigLoadReport::Migrated {
                from: 0,
                to: CURRENT_SCHEMA_VERSION
            }
        );
        assert_eq!(store.config().schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(store.theme(), ThemeMode::Dark);
        assert_eq!(
            store.config().extra.get("custom_user_flag"),
            Some(&serde_json::Value::Bool(true))
        );
    }

    #[test]
    fn recovers_from_corrupted_json_with_backup() {
        let tmp = TempDir::new().unwrap();
        let identity = AppIdentity::new("Corrupt", "App");
        let caps = PlatformCapabilities::for_os(OsKind::MacOs, None);
        let loc = ConfigLocation::CustomDir(tmp.path().to_path_buf());
        let file = resolve_rinui_config_file(&loc, &identity, None).unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "{ not valid json !!!").unwrap();

        let (store, report) = ConfigStore::load_or_default(&loc, &identity, None, caps).unwrap();
        match report {
            ConfigLoadReport::RecoveredFromCorruption { backup_path, .. } => {
                let backup = backup_path.expect("backup should be created");
                assert!(backup.exists());
                assert_eq!(fs::read_to_string(backup).unwrap(), "{ not valid json !!!");
            }
            other => panic!("unexpected load report: {other:?}"),
        }
        assert_eq!(store.theme(), ThemeMode::Auto);
        assert!(file.exists());
    }

    #[test]
    fn in_memory_mode_does_not_touch_filesystem() {
        let identity = AppIdentity::new("Mem", "App");
        let caps = PlatformCapabilities::for_os(OsKind::MacOs, None);
        let (mut store, report) =
            ConfigStore::load_or_default(&ConfigLocation::InMemory, &identity, None, caps).unwrap();
        assert_eq!(report, ConfigLoadReport::InMemory);
        assert!(store.path().is_none());
        store.set_theme(ThemeMode::Light).unwrap();
        assert_eq!(store.theme(), ThemeMode::Light);
    }

    #[test]
    fn app_json_store_handles_isolation_and_corruption() {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
        struct SampleCfg {
            language: String,
            count: u32,
        }

        let tmp = TempDir::new().unwrap();
        let identity = AppIdentity::new("Test", "App");
        let loc = ConfigLocation::CustomDir(tmp.path().to_path_buf());
        let default = SampleCfg {
            language: "en_US".to_owned(),
            count: 0,
        };

        let (mut store, report) =
            AppJsonStore::load_or_default(&loc, &identity, None, "config.json", default.clone())
                .unwrap();
        assert_eq!(report, ConfigLoadReport::CreatedDefault);
        store.update(|c| c.count = 42).unwrap();

        let (reloaded, report2) =
            AppJsonStore::load_or_default(&loc, &identity, None, "config.json", default).unwrap();
        assert_eq!(report2, ConfigLoadReport::Loaded);
        assert_eq!(reloaded.get().count, 42);
    }

    #[test]
    fn recovers_from_invalid_utf8_and_non_object_json() {
        let tmp = TempDir::new().unwrap();
        let identity = AppIdentity::new("Utf8AndType", "App");
        let caps = PlatformCapabilities::for_os(OsKind::Linux, None);
        let loc = ConfigLocation::CustomDir(tmp.path().to_path_buf());
        let file = resolve_rinui_config_file(&loc, &identity, None).unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();

        // Invalid UTF-8 bytes trigger read error + backup + default recovery.
        fs::write(&file, [0xFFu8, 0xFEu8, 0xFDu8]).unwrap();
        let (store1, report1) = ConfigStore::load_or_default(&loc, &identity, None, caps).unwrap();
        assert!(matches!(
            report1,
            ConfigLoadReport::RecoveredFromCorruption {
                backup_path: Some(_),
                ..
            }
        ));
        assert_eq!(store1.theme(), ThemeMode::Auto);

        // Non-object JSON values ("[]", "\"str\"", "null") also recover with unique backups.
        for payload in ["[]", "\"hello\"", "null"] {
            fs::write(&file, payload).unwrap();
            let (store, report) =
                ConfigStore::load_or_default(&loc, &identity, None, caps).unwrap();
            assert!(matches!(
                report,
                ConfigLoadReport::RecoveredFromCorruption {
                    backup_path: Some(_),
                    ..
                }
            ));
            assert_eq!(store.theme(), ThemeMode::Auto);
        }

        let backup_count = fs::read_dir(file.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".corrupt."))
            .count();
        assert_eq!(
            backup_count, 4,
            "each corruption recovery should keep a unique backup file"
        );
    }

    #[test]
    fn recovers_or_coerces_invalid_nested_schema_types() {
        let tmp = TempDir::new().unwrap();
        let identity = AppIdentity::new("NestedSchema", "App");
        let caps = PlatformCapabilities::for_os(OsKind::MacOs, None);
        let loc = ConfigLocation::CustomDir(tmp.path().to_path_buf());
        let file = resolve_rinui_config_file(&loc, &identity, None).unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();

        // Wrong type for `theme` triggers backup + default recovery.
        fs::write(&file, r##"{"theme": 123, "theme_color": "#112233"}"##).unwrap();
        let (store1, report1) = ConfigStore::load_or_default(&loc, &identity, None, caps).unwrap();
        assert!(matches!(
            report1,
            ConfigLoadReport::RecoveredFromCorruption {
                backup_path: Some(_),
                ..
            }
        ));
        assert_eq!(store1.theme(), ThemeMode::Auto);

        // Invalid enum strings inside an otherwise valid object coerce safely and migrate.
        fs::write(
            &file,
            r#"{"theme": {"current_theme": "Bogus"}, "backdrop_effect": "bogus"}"#,
        )
        .unwrap();
        let (store2, report2) = ConfigStore::load_or_default(&loc, &identity, None, caps).unwrap();
        assert_eq!(
            report2,
            ConfigLoadReport::Migrated {
                from: 0,
                to: CURRENT_SCHEMA_VERSION
            }
        );
        assert_eq!(store2.theme(), ThemeMode::Auto);
        assert_eq!(store2.backdrop_effect(), BackdropEffect::None);
    }

    #[test]
    fn app_json_store_recovers_from_corruption_with_backup() {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
        struct SampleCfg {
            language: String,
            count: u32,
        }

        let tmp = TempDir::new().unwrap();
        let identity = AppIdentity::new("CorruptAppStore", "App");
        let loc = ConfigLocation::CustomDir(tmp.path().to_path_buf());
        let file = resolve_app_config_file(&loc, &identity, None, "config.json").unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "{broken-json").unwrap();

        let default = SampleCfg {
            language: "en_US".to_owned(),
            count: 7,
        };
        let (store, report) =
            AppJsonStore::load_or_default(&loc, &identity, None, "config.json", default.clone())
                .unwrap();
        match report {
            ConfigLoadReport::RecoveredFromCorruption { backup_path, .. } => {
                let backup = backup_path.expect("backup file expected");
                assert_eq!(fs::read_to_string(backup).unwrap(), "{broken-json");
            }
            other => panic!("unexpected report: {other:?}"),
        }
        assert_eq!(store.get(), &default);
    }
}
