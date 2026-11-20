use rinui_host::cxx_qt_lib::{QString, QVariant};
use rinui_host::{
    AppIdentity, ConfigLocation, HostConfig, MacWindowOptions, QmlSource, ResourceMode, RinUIHost,
};
use std::path::PathBuf;
use tempfile::TempDir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("QT_QPA_PLATFORM").is_none() {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let tmp = TempDir::new()?;
    let offline_storage = tmp.path().join("qml_offline_storage");
    std::fs::create_dir_all(&offline_storage)?;

    let mut host = RinUIHost::new(HostConfig {
        identity: AppIdentity::new("RinUIExamples", "EngineConfigDemo"),
        resource_mode: ResourceMode::Embedded,
        config_location: ConfigLocation::CustomDir(tmp.path().join("config")),
        locale: Some("en_US".to_owned()),
        mac_window: MacWindowOptions::default(),
        qt_quick_controls_style: "Basic".to_owned(),
    })?;

    host.configure_engine(|cfg| {
        cfg.set_offline_storage_path(&offline_storage);
        cfg.set_extra_file_selectors(&["demo"]);
        println!(
            "Configured offlineStoragePath: {}",
            cfg.offline_storage_path()
        );
        println!("Configured importPaths: {:?}", cfg.import_paths());
    })?;

    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("qml")
        .join("ObjectInteropFixture.qml");
    let loaded = host.load(&QmlSource::File(fixture))?;
    host.process_events();

    host.set_root_property(
        loaded.root_id,
        "titleProp",
        &QVariant::from(&QString::from("EngineConfigDemo Title")),
    )?;
    let read_title = host.get_root_property(loaded.root_id, "titleProp")?;
    let sum = host.invoke_root_method(
        loaded.root_id,
        "addInts",
        &[QVariant::from(&10i32), QVariant::from(&32i32)],
    )?;
    let repeated = host.invoke_root_method(
        loaded.root_id,
        "typedRepeat",
        &[
            QVariant::from(&QString::from("RinUI!")),
            QVariant::from(&2i32),
        ],
    )?;

    println!(
        "Demo result: titleProp={}, addInts(10,32)={}, typedRepeat={}",
        read_title.value::<QString>().unwrap_or_default(),
        sum.value::<i32>().unwrap_or_default(),
        repeated.value::<QString>().unwrap_or_default(),
    );
    Ok(())
}
