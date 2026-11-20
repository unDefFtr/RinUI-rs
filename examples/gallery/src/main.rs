mod backend;
mod config;
mod dev_view;

use crate::backend::qobject::rinui_gallery_backend_new;
use crate::config::load_gallery_config;
use crate::dev_view::GalleryDevView;
use rinui_host::{
    cxx_qt_lib::{QString, QVariant},
    AppIdentity, ConfigLocation, HostConfig, LoadedRoot, MacWindowOptions, NativeWindowRef,
    QmlSource, ResolvedResources, ResourceMode, RinUIHost, RootId, TranslationCatalog,
};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;
use tracing::{error, info};

#[derive(Debug)]
struct CliOptions {
    dev_path: Option<PathBuf>,
    dev_flag: bool,
    examples_dir: Option<PathBuf>,
    smoke_test: bool,
    auto_close_ms: Option<i32>,
    locale: Option<String>,
    mac_window: MacWindowOptions,
    mac_frame_mode: &'static str,
    config_dir: Option<PathBuf>,
}

fn parse_args() -> Result<CliOptions, String> {
    let mut opts = CliOptions {
        dev_path: None,
        dev_flag: false,
        examples_dir: None,
        smoke_test: false,
        auto_close_ms: None,
        locale: None,
        mac_window: MacWindowOptions::default(),
        mac_frame_mode: "self-drawn",
        config_dir: None,
    };

    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dev" => {
                opts.dev_flag = true;
                if let Some(next) = args.peek() {
                    if !next.starts_with("--") {
                        opts.dev_path = Some(PathBuf::from(args.next().unwrap()));
                    }
                }
            }
            "--examples-dir" => {
                let dir = args
                    .next()
                    .ok_or_else(|| "--examples-dir requires a directory path".to_owned())?;
                opts.examples_dir = Some(PathBuf::from(dir));
            }
            "--smoke-test" => {
                opts.smoke_test = true;
            }
            "--upstream-mac-frame" => {
                opts.mac_window = MacWindowOptions::upstream_compatible();
                opts.mac_frame_mode = "upstream-compatible";
            }
            "--native-mac-frame" => {
                opts.mac_window = MacWindowOptions::native(10, 10);
                opts.mac_frame_mode = "native";
            }
            "--no-native-mac-frame" => {
                opts.mac_window = MacWindowOptions::self_drawn();
                opts.mac_frame_mode = "self-drawn";
            }
            "--auto-close-ms" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "--auto-close-ms requires a millisecond value".to_owned())?;
                let ms = raw
                    .parse::<i32>()
                    .map_err(|_| format!("invalid --auto-close-ms value '{raw}'"))?;
                opts.auto_close_ms = Some(ms);
            }
            "--locale" => {
                let loc = args
                    .next()
                    .ok_or_else(|| "--locale requires a locale string (e.g. zh_CN)".to_owned())?;
                opts.locale = Some(loc);
            }
            "--config-dir" => {
                let dir = args
                    .next()
                    .ok_or_else(|| "--config-dir requires a directory path".to_owned())?;
                opts.config_dir = Some(PathBuf::from(dir));
            }
            "--help" | "-h" => {
                println!(
                    "RinUI Gallery (rinui-rs)\n\
                     Options:\n  \
                     --dev [PATH]             Load RinUI + examples from filesystem checkout instead of embedded QRC\n  \
                     --examples-dir <DIR>     Explicit upstream examples directory when --dev targets a RinUI-only dir\n  \
                     --smoke-test             Validate host contracts, multi-window, navigation, theme, i18n, and exit\n  \
                     --auto-close-ms <MS>     Quit cleanly after <MS> milliseconds\n  \
                     --locale <LOCALE>        Start with explicit locale (e.g. en_US, zh_CN)\n  \
                     --upstream-mac-frame     Use MacWindowOptions::upstream_compatible() (native NSWindow on macOS)\n  \
                     --native-mac-frame       Opt into native macOS NSWindow titlebar integration\n  \
                     --no-native-mac-frame    Use the default QML titlebar and traffic-light controls\n  \
                     --config-dir <DIR>       Override configuration base directory"
                );
                std::process::exit(0);
            }
            other => {
                return Err(format!("unknown argument '{other}' (try --help)"));
            }
        }
    }

    Ok(opts)
}

fn default_dev_upstream_path() -> Option<PathBuf> {
    if let Some(env_path) = std::env::var_os("RINUI_DEV_PATH") {
        if !env_path.is_empty() {
            return Some(PathBuf::from(env_path));
        }
    }
    let cwd_candidate = PathBuf::from("upstream").join("Rin-UI");
    if cwd_candidate.join("RinUI").join("qmldir").is_file() {
        return Some(cwd_candidate);
    }
    #[cfg(debug_assertions)]
    {
        let manifest_candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("upstream")
            .join("Rin-UI");
        if manifest_candidate.join("RinUI").join("qmldir").is_file() {
            return Some(manifest_candidate);
        }
    }
    None
}

fn resolve_gallery_examples_dir(
    cli_dir: Option<&PathBuf>,
    import_root: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let candidate = if let Some(explicit) = cli_dir {
        explicit.clone()
    } else if let Some(env_dir) = std::env::var_os("RINUI_EXAMPLES_DIR").filter(|v| !v.is_empty()) {
        PathBuf::from(env_dir)
    } else {
        import_root.join("examples")
    };

    if !candidate.join("gallery.qml").is_file() {
        return Err(format!(
            "filesystem dev mode requires upstream examples/gallery.qml (missing in '{}'); \
             pass `--examples-dir <PATH>` or set RINUI_EXAMPLES_DIR (refusing to silently mix embedded QRC examples)",
            candidate.display()
        )
        .into());
    }
    Ok(candidate)
}

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let opts = parse_args().map_err(|e| format!("CLI error: {e}"))?;

    let resource_mode = if let Some(path) = opts.dev_path.clone() {
        ResourceMode::Filesystem(path)
    } else if opts.dev_flag {
        let fallback = default_dev_upstream_path().ok_or_else(|| {
            "unable to locate upstream/Rin-UI for --dev; pass `--dev <path>` or set RINUI_DEV_PATH"
                .to_owned()
        })?;
        ResourceMode::Filesystem(fallback)
    } else {
        ResourceMode::Auto
    };

    // Resolve resources up front and prepare GalleryDevView BEFORE RinUIHost so
    // Rust reverse local drop order keeps the staged directory alive until RinUIHost drops.
    let resolved_origin = ResolvedResources::resolve(&resource_mode)?;
    let _gallery_dev_view = match &resolved_origin {
        ResolvedResources::Filesystem { import_root } => {
            let examples_dir =
                resolve_gallery_examples_dir(opts.examples_dir.as_ref(), import_root)?;
            Some(GalleryDevView::prepare(&examples_dir)?)
        }
        ResolvedResources::Embedded => None,
    };

    let config_location = match &opts.config_dir {
        Some(dir) => ConfigLocation::CustomDir(dir.clone()),
        None => ConfigLocation::Standard,
    };

    let identity = AppIdentity::new("RinUI", "Gallery").with_version(env!("CARGO_PKG_VERSION"));

    let host_config = HostConfig {
        identity: identity.clone(),
        resource_mode,
        config_location: config_location.clone(),
        locale: opts.locale.clone(),
        mac_window: opts.mac_window,
        qt_quick_controls_style: "Basic".to_owned(),
    };

    cxx_qt::init_crate!(rinui_gallery);

    let mut host = RinUIHost::new(host_config)?;
    let qt_local_data = host
        .config_store()
        .borrow()
        .path()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf());
    let (gallery_cfg_store, _report) =
        load_gallery_config(&config_location, &identity, qt_local_data.as_deref())?;
    let gallery_cfg = Rc::new(RefCell::new(gallery_cfg_store));

    let (main_source, secondary_source, catalog, icon_source) = match &_gallery_dev_view {
        Some(view) => {
            let icon = view.icon_path();
            (
                QmlSource::File(view.main_qml()),
                QmlSource::File(view.secondary_dialog_qml()),
                TranslationCatalog::Directory {
                    id: "gallery".to_owned(),
                    dir: view.languages_dir(),
                },
                icon.is_file().then(|| icon.to_string_lossy().into_owned()),
            )
        }
        None => (
            QmlSource::Url("qrc:/gallery/GalleryMain.qml".to_owned()),
            QmlSource::Url("qrc:/gallery/MultiWindowDialog.qml".to_owned()),
            TranslationCatalog::QrcPrefix {
                id: "gallery".to_owned(),
                prefix: ":/gallery/languages".to_owned(),
            },
            Some(":/gallery/assets/icons/app/gallery.png".to_owned()),
        ),
    };

    host.register_translation_catalog(catalog)?;

    if let Some(cli_locale) = &opts.locale {
        if let Err(err) = host.set_language(cli_locale) {
            tracing::warn!(locale = %cli_locale, error = %err, "Failed to apply CLI --locale");
        }
    } else {
        let saved_lang = gallery_cfg.borrow().get().language.clone();
        if !saved_lang.is_empty() {
            match host.set_language(&saved_lang) {
                Ok(res) if res.active_locale != saved_lang => {
                    let _ = gallery_cfg
                        .borrow_mut()
                        .update(|cfg| cfg.language = res.active_locale);
                }
                Err(err) => {
                    tracing::warn!(
                        locale = %saved_lang,
                        error = %err,
                        "Failed to apply persisted gallery language"
                    );
                }
                _ => {}
            }
        }
    }

    let mut backend = rinui_gallery_backend_new();
    let backend_pin = backend
        .as_mut()
        .ok_or_else(|| "failed to allocate GalleryBackend".to_owned())?;
    backend_pin.attach_context(
        host.handle(),
        Rc::clone(&gallery_cfg),
        secondary_source.clone(),
    );
    host.expose_qobject("Backend", backend)?;

    // Keep NativeMacFrameEnabled populated for backwards compatibility with older QML checks
    // while RinUINativeMacFrame (set by Host/Runtime pre-load) drives RinUI windows.
    host.set_context_property_variant(
        "NativeMacFrameEnabled",
        &QVariant::from(&opts.mac_window.effective_native_frame()),
    )?;

    let loaded = host.load(&main_source)?;
    if let Some(icon) = icon_source {
        host.set_icon(&icon, Some(loaded.root_id));
    }
    host.set_root_string_property(loaded.root_id, "title", "RinUI Gallery (Rust Host)");

    info!(
        resource_mode = %host.resource_view().describe(),
        examples_source = ?_gallery_dev_view.as_ref().map(|v| v.source_examples_dir()),
        examples_staged = ?_gallery_dev_view.as_ref().map(|v| v.staged_dir()),
        windows = loaded.windows.len(),
        "RinUI Gallery started"
    );

    if opts.smoke_test {
        run_smoke_assertions(
            &mut host,
            &loaded,
            &secondary_source,
            opts.mac_window,
            opts.mac_frame_mode,
        )?;
        return Ok(0);
    }

    if let Some(ms) = opts.auto_close_ms {
        host.handle().schedule_quit(ms);
    }

    Ok(host.exec())
}

fn pump_until<F>(host: &RinUIHost, max_iters: usize, mut cond: F) -> bool
where
    F: FnMut(&RinUIHost) -> bool,
{
    for _ in 0..max_iters {
        host.process_events();
        if cond(host) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    host.process_events();
    cond(host)
}

fn invoke_bool_method(
    host: &RinUIHost,
    root_id: RootId,
    method: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let v = host.invoke_root_method(root_id, method, &[])?;
    Ok(v.value::<bool>()
        .ok_or_else(|| format!("QML method '{method}' did not return bool"))?)
}

fn invoke_string_method(
    host: &RinUIHost,
    root_id: RootId,
    method: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let v = host.invoke_root_method(root_id, method, &[])?;
    Ok(v.value::<QString>()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("QML method '{method}' did not return QString"))?)
}

fn window_diag(win: &NativeWindowRef) -> String {
    win.property("_rinuiMacFrameDiagnostic")
        .value::<QString>()
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn run_smoke_assertions(
    host: &mut RinUIHost,
    main_loaded: &LoadedRoot,
    secondary_source: &QmlSource,
    mac_window: MacWindowOptions,
    mac_frame_mode: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let main_root_id = main_loaded.root_id;
    let main_window = *main_loaded
        .windows
        .first()
        .ok_or("expected main Gallery NativeWindowRef")?;

    let settled_initial = pump_until(host, 60, |h| {
        let busy = invoke_bool_method(h, main_root_id, "smokeIsNavigationBusy").unwrap_or(true);
        let page =
            invoke_string_method(h, main_root_id, "smokeNavigationCurrentPage").unwrap_or_default();
        !busy && !page.is_empty()
    });
    assert!(settled_initial, "Gallery initial page did not settle");
    assert!(host.is_root_alive(main_root_id));
    assert!(
        host.handle().open_window_count() >= 1,
        "Gallery main window should be registered"
    );

    let qpa = host.handle().platform_name();
    let expected_native = mac_window.effective_native_frame();
    let expected_mac_diag = if cfg!(target_os = "macos") {
        if !expected_native {
            "self_drawn"
        } else if qpa == "cocoa" {
            "native_cocoa"
        } else {
            "fallback:non_cocoa_qpa"
        }
    } else {
        "na"
    };

    let main_mac_diag = if cfg!(target_os = "macos") {
        if expected_native && qpa == "cocoa" {
            let realized = pump_until(host, 40, |_| window_diag(&main_window) == "native_cocoa");
            assert!(
                realized,
                "Cocoa QPA did not attach native NSWindow frame cleanly (diag={})",
                window_diag(&main_window)
            );
        }
        assert_eq!(
            host.get_root_property(main_root_id, "useNativeMacFrame")?
                .value::<bool>(),
            Some(expected_native),
            "Gallery must honor the selected macOS titlebar mode"
        );
        assert_eq!(
            host.get_root_property(main_root_id, "titleBarHeight")?
                .value::<i32>(),
            Some(if expected_native { 36 } else { 48 }),
            "Gallery FluentWindow must use the upstream titlebar height for the selected mode"
        );
        let diag = window_diag(&main_window);
        assert_eq!(
            diag, expected_mac_diag,
            "Unexpected macOS frame diagnostic on main Gallery window"
        );
        diag
    } else {
        "na".to_owned()
    };

    let min_width = host
        .get_root_property(main_root_id, "minimumWidth")?
        .value::<i32>()
        .unwrap_or_default();
    let page_obj = host
        .get_root_property(main_root_id, "smokeCurrentPageObjectName")?
        .value::<QString>()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let page_title = host
        .get_root_property(main_root_id, "smokeCurrentPageTitle")?
        .value::<QString>()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let rinui_probe = host
        .root_property(main_root_id, "rinuiDevProbe")
        .value::<QString>()
        .map(|s| s.to_string())
        .unwrap_or_default();
    println!(
        "smoke_probe minimumWidth={min_width} currentPageObjectName={page_obj} currentPageTitle={page_title} rinuiDevProbe={rinui_probe}"
    );

    // Interactive Gallery smoke checks: NavigationView sample push, ScrollView/Flickable,
    // missing-page ErrorPage load, and safePop recovery.
    assert!(
        invoke_bool_method(host, main_root_id, "smokePushValidSample")?,
        "smokePushValidSample() should initiate navigation"
    );
    let sample_loaded = pump_until(host, 60, |h| {
        let busy = invoke_bool_method(h, main_root_id, "smokeIsNavigationBusy").unwrap_or(true);
        let page =
            invoke_string_method(h, main_root_id, "smokeNavigationCurrentPage").unwrap_or_default();
        !busy && page.contains("TextArea.qml")
    });
    assert!(
        sample_loaded,
        "NavigationView did not navigate to pages/controls/TextArea.qml"
    );
    assert!(
        invoke_bool_method(host, main_root_id, "smokeHasFlickable")?,
        "TextArea sample page should contain FluentPage Flickable"
    );
    assert!(
        invoke_bool_method(host, main_root_id, "smokeHasScrollView")?,
        "TextArea sample page should contain ScrollableTextArea (RinUI ScrollView)"
    );

    assert!(
        invoke_bool_method(host, main_root_id, "smokePushErrorPage")?,
        "smokePushErrorPage() should initiate unexist/page navigation"
    );
    let error_loaded = pump_until(host, 60, |h| {
        let err_msg = invoke_string_method(h, main_root_id, "smokeCurrentPageErrorMessage")
            .unwrap_or_default();
        let page =
            invoke_string_method(h, main_root_id, "smokeNavigationCurrentPage").unwrap_or_default();
        !err_msg.is_empty() && page.contains("unexist/page")
    });
    assert!(
        error_loaded,
        "NavigationView did not load ErrorPage for 'unexist/page'"
    );

    assert!(
        invoke_bool_method(host, main_root_id, "smokePopNavigation")?,
        "smokePopNavigation() should initiate safePop"
    );
    let error_popped = pump_until(host, 60, |h| {
        let busy = invoke_bool_method(h, main_root_id, "smokeIsNavigationBusy").unwrap_or(true);
        let err_msg = invoke_string_method(h, main_root_id, "smokeCurrentPageErrorMessage")
            .unwrap_or_default();
        !busy && err_msg.is_empty()
    });
    assert!(
        error_popped,
        "NavigationView safePop() did not recover from ErrorPage"
    );

    // Theme switching & persistence
    let prev_theme = host.theme_manager()?.get_theme_name().to_string();
    host.theme_manager_mut()?
        .toggle_theme(QString::from("Dark"));
    assert_eq!(host.theme_manager()?.get_theme().to_string(), "Dark");
    host.theme_manager_mut()?
        .toggle_theme(QString::from("Light"));
    assert_eq!(host.theme_manager()?.get_theme().to_string(), "Light");
    host.theme_manager_mut()?
        .toggle_theme(QString::from(prev_theme.as_str()));

    // Multi-window lifecycle using the active resource source (QRC or dev view)
    let secondary = host.handle().load_secondary_window(secondary_source)?;
    host.process_events();
    assert!(host.handle().open_window_count() >= 2);
    if cfg!(target_os = "macos") {
        let sec_window = *secondary
            .windows
            .first()
            .ok_or("expected secondary NativeWindowRef")?;
        if expected_native && qpa == "cocoa" {
            let sec_realized = pump_until(host, 40, |_| window_diag(&sec_window) == "native_cocoa");
            assert!(
                sec_realized,
                "Secondary dialog did not attach native Cocoa frame (diag={})",
                window_diag(&sec_window)
            );
        }
        assert_eq!(
            host.get_root_property(secondary.root_id, "useNativeMacFrame")?
                .value::<bool>(),
            Some(expected_native),
            "Secondary MultiWindowDialog must inherit RinUINativeMacFrame policy"
        );
        assert_eq!(
            window_diag(&sec_window),
            expected_mac_diag,
            "Secondary MultiWindowDialog must match host macOS frame diagnostic"
        );
    }
    let window_count_with_secondary = host.handle().open_window_count();
    host.handle().release_root(secondary.root_id)?;
    host.process_events();

    // Language switch & decoupled RinUI fallback
    let zh = host.set_language("zh_CN")?;
    assert_eq!(zh.active_locale, "zh_CN");
    assert_eq!(zh.effective_locale, "zh_CN");
    assert!(!zh.rinui_used_fallback);

    let ja = host.set_language("ja_JP")?;
    assert_eq!(ja.active_locale, "ja_JP");
    assert_eq!(ja.effective_locale, "ja_JP");
    assert_eq!(ja.rinui_locale, "en_US");
    assert!(ja.rinui_used_fallback);
    assert!(!ja.active_locale_fallback);

    let fb = host.set_language("zz_ZZ")?;
    assert!(fb.used_fallback);
    assert!(fb.active_locale_fallback);
    assert_eq!(fb.active_locale, "en_US");
    assert_eq!(fb.effective_locale, "en_US");

    // Ensure rin_ui.json and config.json were persisted
    let rinui_cfg_path = host
        .config_store()
        .borrow()
        .path()
        .map(|p| p.to_path_buf())
        .ok_or("missing persisted rin_ui.json path")?;
    if !rinui_cfg_path.is_file() {
        return Err(format!("expected rin_ui.json at {}", rinui_cfg_path.display()).into());
    }

    println!(
        "smoke_summary os={} arch={} qpa={} mac_frame_mode={} native_mac_frame={} mac_diagnostic={} windows={} nav_sample=TextArea error_page_recovered=true scroll_view=true",
        std::env::consts::OS,
        std::env::consts::ARCH,
        qpa,
        mac_frame_mode,
        expected_native,
        main_mac_diag,
        window_count_with_secondary
    );
    info!("Gallery --smoke-test assertions passed");
    Ok(())
}

fn main() -> ExitCode {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

    match run() {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from((code & 0xFF) as u8),
        Err(err) => {
            error!(error = %err, "RinUI Gallery terminated with error");
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
