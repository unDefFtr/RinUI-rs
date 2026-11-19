//! Independent Rust/CXX-Qt 6 host library for the upstream RinUI QML library.

mod app;
mod bridge;
mod error;
mod resources;
mod theme;
mod translator;
mod window;

pub use app::{HostConfig, HostHandle, QmlSource, RinUIHost};
pub use error::HostError;
pub use resources::{
    ResolvedResources, ResourceMode, EMBEDDED_IMPORT_PREFIX, EMBEDDED_QRC_ROOT,
    PACKAGED_RINUI_FILES, SANITIZED_QMLDIR_DROPS,
};
pub use theme::ThemeManager;
pub use translator::{TranslationCatalog, TranslationLoadResult, TranslationManager};
pub use window::{
    LoadedRoot, MacWindowOptions, NativeWindowRef, RootId, WindowCustomizer, WindowId,
};

// Re-export key platform & Qt interop crates so downstream consumers (such as Goosy)
// can define custom QObjects and share consistent crate versions.
pub use cxx;
pub use cxx_qt;
pub use cxx_qt_lib;
pub use rinui_platform::{
    resolve_app_base_dir, AppIdentity, AppJsonStore, BackdropEffect, ConfigError, ConfigLoadReport,
    ConfigLocation, ConfigStore, OsKind, PlatformCapabilities, ResolvedTheme, RinConfig, ThemeMode,
    WindowsVersion,
};

/// Check whether a Qt resource or local filesystem file exists using Qt's VFS.
pub fn qt_file_exists(path: &str) -> bool {
    bridge::ffi::rinui_qt_file_exists(&cxx_qt_lib::QString::from(path))
}

/// Return `true` when there is no active Qt application or the caller is on its main thread.
pub fn is_qt_main_thread() -> bool {
    bridge::ffi::rinui_is_on_app_thread()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn packaged_manifest_includes_core_rinui_files_and_expected_drops() {
        assert!(PACKAGED_RINUI_FILES.contains(&"qmldir"));
        assert!(PACKAGED_RINUI_FILES.contains(&"themes/theme.qml"));
        assert!(PACKAGED_RINUI_FILES.contains(&"themes/qmldir"));
        assert!(PACKAGED_RINUI_FILES.contains(&"assets/fonts/FluentSystemIcons-Resizable.ttf"));
        assert!(PACKAGED_RINUI_FILES.contains(&"assets/img/noise.png"));
        assert!(PACKAGED_RINUI_FILES.contains(&"languages/en_US.qm"));
        assert!(PACKAGED_RINUI_FILES.contains(&"languages/zh_CN.qm"));

        assert!(SANITIZED_QMLDIR_DROPS
            .iter()
            .any(|(q, l)| { *q == "qmldir" && l.contains("DateField") }));
        assert!(SANITIZED_QMLDIR_DROPS
            .iter()
            .any(|(q, l)| { *q == "qmldir" && l.contains("TeachingTip") }));
        assert!(SANITIZED_QMLDIR_DROPS
            .iter()
            .any(|(q, l)| { *q == "qmldir" && l.contains("ScrollViewer") }));
        assert!(SANITIZED_QMLDIR_DROPS
            .iter()
            .any(|(q, l)| { *q == "components/qmldir" && l.contains("HyperlinkButton") }));
        assert!(SANITIZED_QMLDIR_DROPS
            .iter()
            .any(|(q, l)| { *q == "windows/qmldir" && l.contains("ErrorPage") }));
    }

    #[test]
    fn resource_mode_resolution_supports_embedded_and_filesystem() {
        let embedded = ResolvedResources::resolve(&ResourceMode::Embedded).unwrap();
        assert_eq!(embedded.qml_import_path(), EMBEDDED_IMPORT_PREFIX);
        assert_eq!(
            embedded.rinui_qm_path("zh_CN"),
            ":/rinui/RinUI/languages/zh_CN.qm"
        );

        let upstream_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("upstream")
            .join("Rin-UI");
        let fs_res =
            ResolvedResources::resolve(&ResourceMode::Filesystem(upstream_root.clone())).unwrap();
        let canonical =
            resources::strip_windows_verbatim_prefix(upstream_root.canonicalize().unwrap());
        assert_eq!(fs_res.qml_import_path(), canonical.to_string_lossy());

        // Passing RinUI/ subdirectory directly should normalize to its parent.
        let fs_sub =
            ResolvedResources::resolve(&ResourceMode::Filesystem(upstream_root.join("RinUI")))
                .unwrap();
        assert_eq!(fs_sub.qml_import_path(), canonical.to_string_lossy());

        let missing = ResolvedResources::resolve(&ResourceMode::Filesystem(PathBuf::from(
            "/nonexistent/rinui-path",
        )));
        assert!(matches!(
            missing,
            Err(HostError::InvalidResourcePath { .. })
        ));
    }
}
