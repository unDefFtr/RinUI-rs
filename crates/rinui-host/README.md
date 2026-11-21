# rinui-host

Rust-native host and bindings for RinUI (Qt Quick/QML). The crate embeds RinUI's QML controls, themes, icons, and translations. Builds use the included C++/Objective-C++ bridges and require no Git submodule.

Use `rinui-host = "0.1.0-alpha.1"` in your application's `[dependencies]` and `import RinUI` in your QML. Create a `RinUIHost` on the Qt GUI thread, load a `QmlSource`, and call `host.exec()` to run the event loop.

Building requires Rust 1.88 or later, a C++ toolchain, and Qt 6 with Qt Quick, Qt Quick Controls 2, Qt5Compat, and Qt Shader Tools. Set `QMAKE` to your Qt 6 `qmake` if it is not on `PATH`.

Setup and Rust examples are in the [RinUI-rs documentation](https://rinui-rs.undef.work/). The crate ships the MIT license and third-party notices.
