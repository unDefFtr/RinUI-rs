use rinui_host::cxx_qt_lib::QString;
use rinui_host::{
    AppIdentity, ConfigLocation, HostConfig, HostError, MacWindowOptions, QmlSource, ResourceMode,
    RinUIHost, TranslationCatalog, TranslationManager,
};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn elf_hash(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0;
    for &b in bytes {
        h = (h << 4).wrapping_add(b as u32);
        let g = h & 0xF000_0000;
        if g != 0 {
            h ^= g >> 24;
        }
        h &= !g;
    }
    if h == 0 {
        1
    } else {
        h
    }
}

/// Build a valid Qt `.qm` binary catalog containing `(context, source, translation)` entries.
fn build_test_qm(entries: &[(&str, &str, &str)]) -> Vec<u8> {
    const MAGIC: [u8; 16] = [
        0x3c, 0xb8, 0x64, 0x18, 0xca, 0xef, 0x9c, 0x95, 0xcd, 0x21, 0x1c, 0xbf, 0x60, 0xa1, 0xbd,
        0xdd,
    ];
    const TAG_HASHES: u8 = 0x42;
    const TAG_MESSAGES: u8 = 0x69;
    const MSG_END: u8 = 1;
    const MSG_TRANSLATION: u8 = 3;
    const MSG_SOURCE_TEXT: u8 = 6;
    const MSG_CONTEXT: u8 = 7;
    const MSG_COMMENT: u8 = 8;

    let mut messages = Vec::new();
    let mut hashes: Vec<(u32, u32)> = Vec::with_capacity(entries.len());

    for &(context, source, translation) in entries {
        let offset = messages.len() as u32;
        hashes.push((elf_hash(source.as_bytes()), offset));

        // Translation (UTF-16BE)
        let utf16_be: Vec<u8> = translation
            .encode_utf16()
            .flat_map(|unit| unit.to_be_bytes())
            .collect();
        messages.push(MSG_TRANSLATION);
        messages.extend_from_slice(&(utf16_be.len() as u32).to_be_bytes());
        messages.extend_from_slice(&utf16_be);

        // Empty disambiguation comment
        messages.push(MSG_COMMENT);
        messages.extend_from_slice(&0u32.to_be_bytes());

        // Source text (UTF-8)
        let src_bytes = source.as_bytes();
        messages.push(MSG_SOURCE_TEXT);
        messages.extend_from_slice(&(src_bytes.len() as u32).to_be_bytes());
        messages.extend_from_slice(src_bytes);

        // Context (UTF-8)
        let ctx_bytes = context.as_bytes();
        messages.push(MSG_CONTEXT);
        messages.extend_from_slice(&(ctx_bytes.len() as u32).to_be_bytes());
        messages.extend_from_slice(ctx_bytes);

        messages.push(MSG_END);
    }

    hashes.sort_unstable();

    let mut hash_bytes = Vec::with_capacity(hashes.len() * 8);
    for (hash, offset) in hashes {
        hash_bytes.extend_from_slice(&hash.to_be_bytes());
        hash_bytes.extend_from_slice(&offset.to_be_bytes());
    }

    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);

    if !hash_bytes.is_empty() {
        out.push(TAG_HASHES);
        out.extend_from_slice(&(hash_bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&hash_bytes);
    }

    if !messages.is_empty() {
        out.push(TAG_MESSAGES);
        out.extend_from_slice(&(messages.len() as u32).to_be_bytes());
        out.extend_from_slice(&messages);
    }

    out
}

fn write_qm(dir: &Path, name: &str, entries: &[(&str, &str, &str)]) {
    fs::write(dir.join(format!("{name}.qm")), build_test_qm(entries)).expect("write test .qm");
}

fn test_config(cfg_dir: &Path, locale: Option<&str>) -> HostConfig {
    HostConfig {
        identity: AppIdentity::new("RinUITest", "I18nSemantics"),
        resource_mode: ResourceMode::Embedded,
        config_location: ConfigLocation::CustomDir(cfg_dir.to_path_buf()),
        locale: locale.map(str::to_owned),
        mac_window: MacWindowOptions::default(),
        qt_quick_controls_style: "Basic".to_owned(),
    }
}

fn root_str(host: &RinUIHost, root_id: rinui_host::RootId, prop: &str) -> String {
    host.root_property(root_id, prop)
        .value::<QString>()
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn main() {
    if std::env::var_os("QT_QPA_PLATFORM").is_none() {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let i18n_qml = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/I18nTest.qml");

    let tmp = TempDir::new().expect("create temp dir");
    let cfg_dir = tmp.path().join("cfg");
    let app_lang_dir = tmp.path().join("app_langs");
    let opt_lang_dir = tmp.path().join("opt_langs");
    fs::create_dir_all(&app_lang_dir).expect("create app_langs");
    fs::create_dir_all(&opt_lang_dir).expect("create opt_langs");

    // Primary app catalog: has en_US, fr_FR, zh_CN, and a corrupted de_DE.qm for rollback testing.
    write_qm(
        &app_lang_dir,
        "en_US",
        &[
            ("AppI18n", "Hello", "Hello"),
            ("AppI18n", "Goodbye", "Goodbye"),
        ],
    );
    write_qm(
        &app_lang_dir,
        "fr_FR",
        &[
            ("AppI18n", "Hello", "Bonjour"),
            ("AppI18n", "Goodbye", "Au revoir"),
        ],
    );
    write_qm(
        &app_lang_dir,
        "zh_CN",
        &[("AppI18n", "Hello", "你好"), ("AppI18n", "Goodbye", "再见")],
    );
    fs::write(app_lang_dir.join("de_DE.qm"), b"not-a-valid-qm-file")
        .expect("write corrupted de_DE.qm");

    // Optional second app catalog: ONLY provides fr_FR (no en_US.qm!).
    // When switching away from fr_FR to a locale without an optional match, it must unload.
    write_qm(
        &opt_lang_dir,
        "fr_FR",
        &[("AppI18n", "Goodbye", "À bientôt")],
    );

    let mut host =
        RinUIHost::new(test_config(&cfg_dir, Some("fr_FR"))).expect("init host in fr_FR");
    assert_eq!(host.handle().current_locale().unwrap(), "fr_FR");
    assert_eq!(TranslationManager::default_qt_locale(), "fr_FR");

    let reg_app = host
        .register_translation_catalog(TranslationCatalog::Directory {
            id: "app".to_owned(),
            dir: app_lang_dir.clone(),
        })
        .expect("register app catalog");
    assert_eq!(reg_app.active_locale, "fr_FR");
    assert_eq!(reg_app.effective_locale, "fr_FR");
    assert_eq!(reg_app.rinui_locale, "en_US");
    assert!(reg_app.rinui_used_fallback);
    assert!(!reg_app.active_locale_fallback);
    assert_eq!(reg_app.app_catalogs.len(), 1);
    assert_eq!(
        reg_app.app_catalogs[0].selected_locale.as_deref(),
        Some("fr_FR")
    );
    assert!(!reg_app.app_catalogs[0].used_fallback);
    assert!(reg_app.app_catalogs[0].loaded);

    let reg_opt = host
        .register_translation_catalog(TranslationCatalog::Directory {
            id: "opt".to_owned(),
            dir: opt_lang_dir.clone(),
        })
        .expect("register optional catalog");
    assert_eq!(reg_opt.app_catalogs.len(), 2);
    assert_eq!(
        reg_opt.app_catalogs[1].selected_locale.as_deref(),
        Some("fr_FR")
    );

    let loaded = host
        .load(&QmlSource::File(i18n_qml))
        .expect("load I18nTest.qml");
    host.process_events();

    // 1. App uses French while RinUI falls back to English, and QLocale/Calendar stay fr_FR.
    assert_eq!(root_str(&host, loaded.root_id, "appGreeting"), "Bonjour");
    assert_eq!(root_str(&host, loaded.root_id, "appFarewell"), "À bientôt");
    assert_eq!(root_str(&host, loaded.root_id, "rinuiToday"), "Today");
    assert_eq!(
        root_str(&host, loaded.root_id, "calendarLocaleName"),
        "fr_FR"
    );
    assert_eq!(
        root_str(&host, loaded.root_id, "calendarMonthZero").to_lowercase(),
        "janvier"
    );
    assert_eq!(TranslationManager::default_qt_locale(), "fr_FR");

    // 2. Valid locale with no translation files anywhere (`ja_JP`):
    //    - Active locale and QLocale stay `ja_JP` (NOT forced to `en_US`)
    //    - RinUI falls back to `en_US`
    //    - Primary app catalog falls back to `en_US`
    //    - Optional catalog without `en_US.qm` unloads cleanly (no lingering French!).
    let ja_res = host.set_language("ja_JP").expect("switch to ja_JP");
    host.process_events();
    assert_eq!(ja_res.active_locale, "ja_JP");
    assert_eq!(ja_res.effective_locale, "ja_JP");
    assert_eq!(ja_res.rinui_locale, "en_US");
    assert!(ja_res.rinui_used_fallback);
    assert!(!ja_res.active_locale_fallback);
    assert!(ja_res.used_fallback);
    assert_eq!(host.handle().current_locale().unwrap(), "ja_JP");
    assert_eq!(TranslationManager::default_qt_locale(), "ja_JP");
    assert_eq!(
        ja_res.app_catalogs[0].selected_locale.as_deref(),
        Some("en_US")
    );
    assert!(ja_res.app_catalogs[0].used_fallback);
    assert_eq!(ja_res.app_catalogs[1].selected_locale, None);
    assert!(!ja_res.app_catalogs[1].loaded);
    assert_eq!(root_str(&host, loaded.root_id, "appGreeting"), "Hello");
    assert_eq!(root_str(&host, loaded.root_id, "appFarewell"), "Goodbye");

    // 3. Alias tags normalize cleanly (`fr-FR`, `fr`, `zh-CN`, `zh_Hans_CN`, `zh`).
    let fr_hyphen = host.set_language("fr-FR").expect("switch to fr-FR");
    assert_eq!(fr_hyphen.active_locale, "fr_FR");
    assert!(!fr_hyphen.active_locale_fallback);
    assert!(fr_hyphen.rinui_used_fallback);
    host.process_events();
    assert_eq!(root_str(&host, loaded.root_id, "appGreeting"), "Bonjour");
    assert_eq!(root_str(&host, loaded.root_id, "appFarewell"), "À bientôt");

    let fr_bare = host.set_language("fr").expect("switch to bare fr");
    assert_eq!(fr_bare.active_locale, "fr_FR");
    assert_eq!(TranslationManager::default_qt_locale(), "fr_FR");

    let zh_script = host
        .set_language("zh_Hans_CN")
        .expect("switch to zh_Hans_CN");
    host.process_events();
    assert_eq!(zh_script.active_locale, "zh_CN");
    assert_eq!(zh_script.rinui_locale, "zh_CN");
    assert!(!zh_script.rinui_used_fallback);
    assert!(!zh_script.active_locale_fallback);
    assert_eq!(root_str(&host, loaded.root_id, "appGreeting"), "你好");
    assert_eq!(root_str(&host, loaded.root_id, "appFarewell"), "再见");
    assert_eq!(root_str(&host, loaded.root_id, "rinuiToday"), "今天");

    // 4. Corrupted catalog failure rollback:
    //    Switch back to fr_FR first, then attempt corrupted de_DE.
    //    Active locale, QLocale, and all installed translators MUST stay at fr_FR.
    host.set_language("fr").expect("switch back to fr");
    host.process_events();
    assert_eq!(root_str(&host, loaded.root_id, "appGreeting"), "Bonjour");

    let err = host
        .set_language("de_DE")
        .expect_err("corrupted de_DE.qm must fail");
    assert!(matches!(err, HostError::TranslationLoadFailed { .. }));
    host.process_events();
    assert_eq!(host.handle().current_locale().unwrap(), "fr_FR");
    assert_eq!(TranslationManager::default_qt_locale(), "fr_FR");
    assert_eq!(root_str(&host, loaded.root_id, "appGreeting"), "Bonjour");
    assert_eq!(root_str(&host, loaded.root_id, "appFarewell"), "À bientôt");

    // 5. Unrecognized locale tag falls back active locale to `en_US`.
    let bogus = host
        .set_language("xx_NONEXISTENT")
        .expect("unrecognized locale falls back to en_US");
    host.process_events();
    assert!(bogus.active_locale_fallback);
    assert!(bogus.used_fallback);
    assert_eq!(bogus.active_locale, "en_US");
    assert_eq!(bogus.effective_locale, "en_US");
    assert_eq!(TranslationManager::default_qt_locale(), "en_US");
    assert_eq!(root_str(&host, loaded.root_id, "appGreeting"), "Hello");
    assert_eq!(root_str(&host, loaded.root_id, "appFarewell"), "Goodbye");

    println!("i18n_semantics integration tests passed.");
}
