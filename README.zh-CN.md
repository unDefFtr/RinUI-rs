# rinui-rs

面向 [RinUI](https://github.com/RinLit-233-shiroko/Rin-UI) Qt Quick / QML 库的独立、可复用 Rust + C++/Objective-C++ Qt 6 宿主层。

`rinui-rs` 以原生 Rust/C++ Qt 6 运行时替代 RinUI 原有的 Python / PySide6 运行时（`RinUI/core/`），同时保持 `upstream/Rin-UI` 作为锁定提交的 Git Submodule **完全不被修改**。

---

## 核心特性

- **零修改上游 Submodule**：`upstream/Rin-UI` 保持纯净（`46d77fa8371ff0e9f46a10a2eb66612dab123269`），缺失或无版本号的 `qmldir` 条目在构建阶段于 `$OUT_DIR/rinui_sanitized/` 中修正。
- **完整保留 RinUI QML 契约**：提供完整的 `ThemeManager`（`themeChanged`、`backdropChanged` 信号与全部 `toggle_theme` / `get_theme` / `apply_backdrop_effect` / `set_theme_color` 等槽方法）及 Windows `WinEventManager`。
- **Qt 6 原生颜色方案监听**：使用 `QStyleHints::colorSchemeChanged` 信号替代 Python 端 1 秒轮询 `darkdetect`。
- **应用隔离与原子持久化（`rinui-platform`）**：支持基于 `AppIdentity` 的跨平台配置目录隔离、原子写入、旧版 Python `rin_ui.json` 迁移与损坏配置备份恢复。
- **国际化支持**：系统区域与标签规范化（`zh-CN` / `zh_Hans_CN` / `zh`）、嵌入式 QRC / 开发文件系统 `.qm` 加载、`en_US` 回退、自定义应用翻译目录及运行时 `retranslate()`（已兼容 Qt 6.9 空目录 `.qm` 加载语义）。
- **多窗口、线程亲和与生命周期加固**：`!Send + !Sync` 的 `RinUIHost` 与 `HostHandle`、进程级单实例守卫、可重入检查、CXX-Qt FFI `catch_unwind` 边界、基于 `QPointer` 的安全句柄表、严格析构顺序以及 `WindowCustomizer` 扩展钩子。
- **原生平台适配**：
  - **macOS**：ARC Objective-C++ `NSWindow` 透明标题栏、`NSWindowStyleMaskFullSizeContentView`、`NSAppearance` 同步及交通灯偏移定位。（已在 CI `Qt 6.9.3` 与本地 `Qt 6.12.0` / `6.9.3` 编译测试，含隔离 `.app` Cocoa QPA 冒烟测试；交互式桌面视觉验收待完成。）
  - **Windows**：C++ `WinEventFilter` & `WinEventManager`（`WM_NCCALCSIZE`、`WM_GETMINMAXINFO`、`WM_NCHITTEST`、Win11 Snap Layouts `HTMAXBUTTON`、DWM Mica/Acrylic/Tabbed 及 Win10 `SetWindowCompositionAttribute`）。（已在 CI `windows-latest` MSVC + `Qt 6.9.3` 编译测试，含 `offscreen` 与隔离 `windeployqt` `windows` QPA 冒烟测试；物理桌面与多屏 DPI 视觉验收待完成。）
  - **Linux**：X11 / Wayland Qt 合成器集成，对不支持的 Windows DWM 背景特效安全返回 `-2`。（已在 CI `ubuntu-latest` `Qt 6.9.3` 编译测试，含 `offscreen` 与 Xvfb `xcb` 冒烟测试；真实 X11 窗口管理器与原生 Wayland 合成器视觉验收待完成。）
- **双资源加载模式**：
  - **内嵌发布模式（`ResourceMode::Embedded`）**：通过 Qt `rcc` 将全部 QML、JS、字体、图片及 `.qm` 打包到 `qrc:/rinui/RinUI`，发布二进制不依赖源码目录。
  - **开发文件系统模式（`ResourceMode::Filesystem` / `--dev` / `RINUI_DEV_PATH`）**：直接从本地目录加载 RinUI，且不会被 QRC 遮蔽。

---

## 版本与验证矩阵

| 组件 | 版本 / 基线 |
|---|---|
| `rinui-rs` | `0.1.0-alpha.1` |
| 上游 RinUI（`upstream/Rin-UI`） | `46d77fa8371ff0e9f46a10a2eb66612dab123269`（`beta-0.5.0-16-g46d77fa`，`0.5.0`） |
| Rust 工具链 | CI：`rustc 1.100.0`；本地 macOS：`rustc 1.97.1`（MSRV `1.85`） |
| `cxx-qt` / `cxx-qt-lib` / `cxx-qt-build` / `qt-build-utils` | `0.10.0` |
| `cxx` | `1.0` |
| Qt 6 | CI：`Qt 6.9.3`（macOS arm64、Windows MSVC、Ubuntu GCC+`lld`）；本地 macOS：`Qt 6.12.0` & `6.9.3`（需 `Qt5Compat` 与 `QtShaderTools`） |
| CMake | `>= 3.24`（本地验证 `4.4.0`） |

| 平台 | 已实现 | 已编译 | 自动化测试与隔离打包冒烟 | 交互式桌面视觉验证 |
|---|---|---|---|---|
| **macOS (arm64)** | 是 | 是 | 是（`cargo test --workspace` + 隔离 `.app` Cocoa QPA） | 待人工视觉检查与截图 |
| **Windows (x64 MSVC)** | 是 | 是 | 是（CI `cargo test --workspace` + 隔离 `windeployqt` `windows` QPA） | 待物理 Windows 桌面与多屏检查 |
| **Linux (x64 GCC + `lld`)** | 是 | 是 | 是（CI `cargo test --workspace` + `offscreen` / Xvfb `xcb`） | 待真实 X11 WM / Wayland 检查 |

---

## 快速开始

### 1. 克隆仓库（含 Submodule）

```bash
git clone --recurse-submodules https://github.com/unDefFtr/rinui-rs
cd rinui-rs
```

### 2. 环境依赖

- **macOS**：Xcode Command Line Tools、Rust（`rustup`）、带 `qt5compat` 与 `qtshadertools` 的 Qt 6（如 `brew install qt`，确保 `qmake6` 在 `PATH` 或设置 `QMAKE=/path/to/qmake6`）。Apple Silicon + Xcode 26（Apple Clang 21）搭配 Qt `< 6.11.1` 时，需设置 `CXXFLAGS="-include arm_acle.h"` 以规避 Qt `qyieldcpu.h` 的 [QTBUG-145239](https://qt-project.atlassian.net/browse/QTBUG-145239)。
- **Windows**：MSVC Build Tools 2022、Rust（`msvc` 工具链）、Qt 6（`6.9+`，含 `Qt 5 Compatibility Module` 与 `Qt Shader Tools`），确保 `qmake` 在 `PATH` 或设置 `QMAKE`。
- **Linux**：GCC/Clang、Mesa/X11/Wayland 开发包、Rust、带 `qt5compat` + `qtshadertools` 的 Qt 6、`qmake6`/`qmake`，以及 `lld`（或 `gold`/`mold`）。CXX-Qt 链接生成的 QRC / QML 初始化器时依赖 `+whole-archive`，GNU `ld.bfd` 无法正确链接，`cxx-qt-build` 会自动优先选用 `lld`。

### 3. 审计、测试与运行 Gallery

```bash
# 检查上游 submodule 状态与 qmldir 契约
python3 scripts/audit_upstream.py

# 运行全量单元测试、Qt 集成测试与独立二进制打包冒烟测试
cargo test --workspace

# 运行嵌入式资源模式 RinUI Gallery
cargo run -p rinui-gallery

# 开发模式：直接从 upstream/Rin-UI 文件系统加载 RinUI
cargo run -p rinui-gallery -- --dev
```

常用 Gallery 参数：`--dev [PATH]`、`--smoke-test`、`--auto-close-ms <MS>`、`--locale <LOCALE>`、`--no-native-mac-frame`、`--config-dir <DIR>`。（界面截图将在桌面交互式视觉验收后补充。）

### 4. 发布打包

```bash
cargo build --release -p rinui-gallery

# macOS（仅本地 ad-hoc 签名，未配置 Apple Developer ID 公证）：
./scripts/package_macos.sh target/release/rinui-gallery dist
# Windows (PowerShell)：
.\scripts\package_windows.ps1 -BinaryPath target\release\rinui-gallery.exe -OutputDir dist\RinUIGallery-win64
# Linux：
./scripts/package_linux.sh target/release/rinui-gallery dist/rinui-gallery-linux
```

---

## 下游集成示例（自动 Drop 安全退出）

请通过 `Result` + `ExitCode` 返回退出码，避免在 `RinUIHost` 析构前调用 `std::process::exit`，从而确保配置保存与 Qt 逆序释放完整执行：

```rust
use rinui_host::{
    AppIdentity, ConfigLocation, HostConfig, MacWindowOptions, QmlSource, ResourceMode, RinUIHost,
};
use std::process::ExitCode;

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let mut host = RinUIHost::new(HostConfig {
        identity: AppIdentity::new("Acme", "Goosy"),
        resource_mode: ResourceMode::Auto,
        config_location: ConfigLocation::Standard,
        locale: None,
        mac_window: MacWindowOptions::default(),
        qt_quick_controls_style: "Basic".to_owned(),
    })?;

    let loaded = host.load(&QmlSource::Url("qrc:/app/Main.qml".to_owned()))?;
    host.set_root_string_property(loaded.root_id, "title", "Goosy");
    Ok(host.exec())
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from((code & 0xFF) as u8),
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
```

---

## 已知限制（`v0.1.0-alpha.1`）

- 交互式桌面视觉验收（macOS 原生标题栏与交通灯几何、Windows 11 DWM Mica/Acrylic/Snap Layouts 与多屏 DPI、真实 Linux X11 WM 与原生 Wayland 合成器）尚待人工实机检查。
- `scripts/package_macos.sh` 仅做本地 ad-hoc 签名，未进行 Apple Developer ID 签名或公证（Notarization）。
- 尽管 QML 与资源通过 Qt `rcc` 内嵌至二进制，分发链接 Qt 6 动态库的二进制仍需遵守 Qt LGPLv3 / GPLv3 / Commercial 许可要求（见 [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)）。

更多文档与发布资料请参阅：
- [`CHANGELOG.md`](CHANGELOG.md) — 变更日志（`0.1.0-alpha.1`）
- [`docs/compatibility.md`](docs/compatibility.md) — 上游审计与兼容性矩阵
- [`docs/architecture.md`](docs/architecture.md) — 架构与内存/生命周期模型
- [`docs/consumer-guide.md`](docs/consumer-guide.md) — 下游 Rust 应用（如 Goosy）接入指南
- [`docs/packaging-and-licensing.md`](docs/packaging-and-licensing.md) — 跨平台打包与 Qt/MIT 许可说明
- [`docs/updating-upstream.md`](docs/updating-upstream.md) — 上游 RinUI Submodule 升级流程
- [`docs/validation-report.md`](docs/validation-report.md) — Phase 2 跨平台 CI 与本地验证报告
- [`docs/release-checklist.md`](docs/release-checklist.md) — `v0.1.0-alpha.1` 发布检查清单
- [`docs/release-notes-v0.1.0-alpha.1.md`](docs/release-notes-v0.1.0-alpha.1.md) — `v0.1.0-alpha.1` 发布说明草案
