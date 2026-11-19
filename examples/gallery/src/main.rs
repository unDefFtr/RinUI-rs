mod backend;
mod config;

use crate::backend::qobject::rinui_gallery_backend_new;
use crate::config::load_gallery_config;
use rinui_host::{
    AppIdentity, ConfigLocation, HostConfig, MacWindowOptions, QmlSource, ResourceMode, RinUIHost,
    TranslationCatalog,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use tracing::{error, info};

#[derive(Debug)]
struct CliOptions {
    dev_path: Option<PathBuf>,
    dev_flag: bool,
    smoke_test: bool,
    auto_close_ms: Option<i32>,
    locale: Option<String>,
    native_mac_frame: bool,
    config_dir: Option<PathBuf>,
}

fn parse_args() -> Result<CliOptions, String> {
    let mut opts = CliOptions {
        dev_path: None,
        dev_flag: false,
        smoke_test: false,
        auto_close_ms: None,
        locale: None,
        native_mac_frame: MacWindowOptions::default().enabled,
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
            "--smoke-test" => {
                opts.smoke_test = true;
            }
            "--native-mac-frame" => {
                opts.native_mac_frame = true;
            }
            "--no-native-mac-frame" => {
                opts.native_mac_frame = false;
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
                     --dev [PATH]             Load RinUI from filesystem checkout instead of embedded QRC\n  \
                     --smoke-test             Validate host contracts, multi-window, theme, i18n, and exit\n  \
                     --auto-close-ms <MS>     Quit cleanly after <MS> milliseconds\n  \
                     --locale <LOCALE>        Start with explicit locale (e.g. en_US, zh_CN)\n  \
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
        mac_window: MacWindowOptions {
            enabled: opts.native_mac_frame,
            ..MacWindowOptions::default()
        },
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

    let dev_lang_dir = match host.resources() {
        rinui_host::ResolvedResources::Filesystem { import_root } => {
            let candidate = import_root.join("examples").join("languages");
            candidate.is_dir().then_some(candidate)
        }
        rinui_host::ResolvedResources::Embedded => None,
    };

    let catalog = match &dev_lang_dir {
        Some(dir) => TranslationCatalog::Directory {
            id: "gallery".to_owned(),
            dir: dir.clone(),
        },
        None => TranslationCatalog::QrcPrefix {
            id: "gallery".to_owned(),
            prefix: ":/gallery/languages".to_owned(),
        },
    };
    host.register_translation_catalog(catalog)?;

    let initial_lang = opts
        .locale
        .clone()
        .unwrap_or_else(|| gallery_cfg.borrow().get().language.clone());
    if !initial_lang.is_empty() {
        let _ = host.set_language(&initial_lang);
    }

    let mut backend = rinui_gallery_backend_new();
    let backend_pin = backend
        .as_mut()
        .ok_or_else(|| "failed to allocate GalleryBackend".to_owned())?;
    backend_pin.attach_context(host.handle(), Rc::clone(&gallery_cfg), dev_lang_dir);
    host.expose_qobject("Backend", backend)?;

    // Select the frame before QML creates the window, including its first visible frame.
    host.set_context_property_variant(
        "NativeMacFrameEnabled",
        &rinui_host::cxx_qt_lib::QVariant::from(&opts.native_mac_frame),
    )?;

    let main_source = QmlSource::Url("qrc:/gallery/GalleryMain.qml".to_owned());
    let loaded = host.load(&main_source)?;
    host.set_icon(
        ":/gallery/assets/icons/app/gallery.png",
        Some(loaded.root_id),
    );
    host.set_root_string_property(loaded.root_id, "title", "RinUI Gallery (Rust Host)");

    info!(
        resource_mode = %host.resources().describe(),
        windows = loaded.windows.len(),
        "RinUI Gallery started"
    );

    if opts.smoke_test {
        run_smoke_assertions(&mut host, loaded.root_id, opts.native_mac_frame)?;
        return Ok(0);
    }

    if let Some(ms) = opts.auto_close_ms {
        host.handle().schedule_quit(ms);
    }

    Ok(host.exec())
}

fn run_smoke_assertions(
    host: &mut RinUIHost,
    main_root_id: rinui_host::RootId,
    native_mac_frame: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    host.process_events();
    assert!(host.is_root_alive(main_root_id));
    assert!(
        host.handle().open_window_count() >= 1,
        "Gallery main window should be registered"
    );

    if cfg!(target_os = "macos") {
        assert_eq!(
            host.root_property(main_root_id, "useNativeMacFrame")
                .value::<bool>(),
            Some(native_mac_frame),
            "Gallery must honor the selected macOS titlebar mode"
        );
        assert_eq!(
            host.root_property(main_root_id, "titleBarHeight")
                .value::<i32>(),
            Some(if native_mac_frame { 36 } else { 48 }),
            "Gallery must use the upstream titlebar height for the selected mode"
        );
    }

    // Theme switching & persistence
    let prev_theme = host.theme_manager().get_theme_name().to_string();
    host.theme_manager_mut()
        .toggle_theme(rinui_host::cxx_qt_lib::QString::from("Dark"));
    assert_eq!(host.theme_manager().get_theme().to_string(), "Dark");
    host.theme_manager_mut()
        .toggle_theme(rinui_host::cxx_qt_lib::QString::from("Light"));
    assert_eq!(host.theme_manager().get_theme().to_string(), "Light");
    host.theme_manager_mut()
        .toggle_theme(rinui_host::cxx_qt_lib::QString::from(prev_theme.as_str()));

    // Multi-window lifecycle
    let secondary = host.handle().load_secondary_window(&QmlSource::Url(
        "qrc:/gallery/MultiWindowDialog.qml".to_owned(),
    ))?;
    host.process_events();
    assert!(host.handle().open_window_count() >= 2);
    host.handle().release_root(secondary.root_id)?;
    host.process_events();

    // Language switch & fallback
    let zh = host.set_language("zh_CN")?;
    assert_eq!(zh.effective_locale, "zh_CN");
    let fb = host.set_language("zz_ZZ")?;
    assert!(fb.used_fallback);
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
