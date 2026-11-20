# rinui-rs

面向 [RinUI](https://github.com/RinLit-233-shiroko/Rin-UI) Qt Quick / QML 库的独立、可复用 Rust + C++/Objective-C++ Qt 6 宿主层。

`rinui-rs` 以原生 Rust/C++ Qt 6 运行时替代 RinUI 原有的 Python / PySide6 运行时（`RinUI/core/`），同时保持 `upstream/Rin-UI` 作为锁定提交的 Git Submodule **完全不被修改**。

---

## 核心特性

- **零修改上游 Submodule**：`upstream/Rin-UI` 保持纯净（`46d77fa8371ff0e9f46a10a2eb66612dab123269`）；构建期（`$OUT_DIR/rinui_sanitized/`）与开发模式（每次启动重建的 `ResourceView` / `GalleryDevView` 视图）复用同一份 `qmldir` 清理逻辑。
- **完整保留 RinUI QML 契约**：提供完整的 `ThemeManager`（`themeChanged`、`backdropChanged` 信号与全部槽方法）、Windows `WinEventManager`，以及首帧前生效的 `RinUINativeMacFrame` 上下文控制与 `_rinuiMacFrameDiagnostic` 诊断属性。
- **Qt 6 原生颜色方案监听**：使用 `QStyleHints::colorSchemeChanged` 信号替代 Python 端 1 秒轮询 `darkdetect`。
- **应用隔离与原子持久化（`rinui-platform`）**：支持基于 `AppIdentity` 的跨平台配置目录隔离、原子写入、旧版 Python `rin_ui.json` 迁移与损坏配置备份恢复。
- **国际化与活动区域解耦**：系统区域与标签规范化（`zh-CN`、`zh_Hans_CN`、`zh`），RinUI 的 `en_US` 资源回退不再覆盖应用独有语种（如 `fr_FR`）或 `QLocale::setDefault()`；支持两阶段原子替换、失败回滚、可选目录卸载与多 Engine `retranslate()`。
- **4 层所有权、已有 Qt 应用接入与通用对象互操作**：提供进程级 `RinUIRuntime<'app>`（支持新建或借用外部 `QGuiApplication` / `QApplication`）、多会话 `RinUISession<'app>`（`EngineMode::Shared` / `EngineMode::Private`）、弱引用检测失效的 `SessionHandle<'app>` / `HostHandle`、便捷入口 `RinUIHost`，以及带类型检查的根对象属性读写（`set_root_property` / `get_root_property`）、同步方法调用（`invoke_root_method`）和受控 `configure_engine`。
- **原生平台适配**：
  - **macOS**：显式区分 `MacWindowOptions::self_drawn()`（默认自绘）与 `MacWindowOptions::upstream_compatible()` / `native()`，ARC Objective-C++ `NSWindow` 透明标题栏与交通灯定位（已在本地 `Qt 6.12.0` 验证 `offscreen` 及独立 `.app` 原生 Cocoa 两种模式启动冒烟；完整交互式视觉 QA 待执行）。
  - **Windows**：Runtime 寿命 C++ `WinEventFilter` & `WinEventManager`（`WM_NCCALCSIZE`、`WM_GETMINMAXINFO`、`WM_NCHITTEST`、Win11 Snap Layouts `HTMAXBUTTON`、DWM Mica/Acrylic/Tabbed、Win10 `ACCENT_DISABLED` 修复），及独立于 Python/PyInstaller 的 `package_msix.ps1` MSIX 与可选 Authenticode 签名入口（基线 `alpha.1` CI 曾验证；Parity 改动后 Windows CI、MSIX 安装启动及物理桌面视觉验收待执行）。
  - **Linux**：X11 / Wayland Qt 合成器集成，对不支持的 Windows DWM 背景特效安全返回 `-2`（基线 CI 仅验证 `offscreen` 与 headless `xvfb-xcb`；Parity 改动后 Linux CI、真实 X11 WM 及原生 Wayland 合成器实机验收待执行）。
- **双资源加载模式**：
  - **内嵌发布模式（`ResourceMode::Embedded`）**：通过 Qt `rcc` 将全部 QML、JS、字体、图片及 `.qm` 打包到 `qrc:/rinui/RinUI`，发布二进制不依赖源码目录。
  - **开发文件系统模式（`ResourceMode::Filesystem` / `--dev` / `RINUI_DEV_PATH`）**：每次启动在临时视图中准备最新 `RinUI/` 与 `examples/`（含 `--examples-dir` / `RINUI_EXAMPLES_DIR` 支持），重启即生效且不被 QRC 遮蔽。

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

| 平台 | 已实现 | Parity 改动后本地验证 | CI / 物理桌面与签名状态 |
|---|---|---|---|
| **macOS (arm64)** | 是 | 是（`Qt 6.12.0`：`audit_upstream.py`、`cargo test --workspace`、`qt-widgets-test`、`engine_config_demo`、隔离 `.app` 严格 Cocoa QPA 双模式冒烟） | 基线 `alpha.1` CI 曾通过 `Qt 6.9.3`；最新交互式视觉/全屏回归与 Developer ID 签名/公证尚未执行 |
| **Windows (x64 MSVC)** | 是 | macOS 主机无 Windows 工具链（已实现 `package_windows.ps1` 与无 Python `package_msix.ps1`） | 基线 `alpha.1` CI 曾通过；Parity 改动后 Windows CI（严格 `windows` QPA + unsigned MSIX 解包）、MSIX 安装启动、Authenticode 签名与 Win10/11 实机验收待执行 |
| **Linux (x64 GCC + `lld`)** | 是 | macOS 主机无 Linux 目标环境（已更新严格分层 CI 与 `xvfb-xcb` 标注） | 基线 `alpha.1` CI 曾通过 headless `xvfb-xcb`；Parity 改动后 Linux CI、真实 X11 WM 及原生 Wayland 合成器实机验收待执行 |

*(资源文件收录率与 `--smoke-test` 验证初始化、导航与核心路径，不等同于全部控件的桌面人工 UI 验收。)*

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

macOS 上，Gallery 和 `MacWindowOptions::default()` 默认采用自绘 QML 标题栏（属于本宿主的保守兼容策略，而上游 QML 在具备 PyObjC 时默认启用原生标题栏）。如需显式对齐上游 macOS 原生框架行为，请传入 `--upstream-mac-frame`（或使用 `MacWindowOptions::upstream_compatible()` / `HostConfig::with_upstream_mac_frame()`）；`--native-mac-frame` 与 `--no-native-mac-frame` 仍保留并按“最后一个参数生效”解析。

常用 Gallery 参数：`--dev [PATH]`、`--examples-dir <DIR>`、`--smoke-test`、`--auto-close-ms <MS>`、`--locale <LOCALE>`、`--upstream-mac-frame`、`--native-mac-frame`、`--no-native-mac-frame`、`--config-dir <DIR>`。（界面截图将在桌面交互式视觉验收后补充。）

### 4. 发布打包

```bash
cargo build --release -p rinui-gallery

# macOS（默认仅本地 ad-hoc 签名；可传 --sign-identity / --notarize / --notary-profile）：
./scripts/package_macos.sh target/release/rinui-gallery dist
# Windows 目录与无 Python MSIX（PowerShell 5.1/7；可传 -Sign / -CertPfx / -CertThumbprint）：
.\scripts\package_windows.ps1 -BinaryPath target\release\rinui-gallery.exe -OutputDir dist\RinUIGallery-win64
.\scripts\package_msix.ps1 -SourceDirectory dist\RinUIGallery-win64 -OutputDirectory dist\msix -SkipSign -VerifyLayout
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

- 交互式桌面视觉验收（macOS 最新变动下的标题栏拖动/交通灯几何与退出全屏恢复、Windows 10 Acrylic 与 Windows 11 DWM Mica/Acrylic/Snap Layouts/多屏 DPI、真实 Linux X11 WM 与原生 Wayland 合成器）以及 Parity 改动后的 Windows/Linux CI 运行尚待对应系统或 Runner 执行。
- 受限于本机缺少 Apple Developer ID 证书/公证凭据与 Windows 代码签名证书，Developer ID 签名/公证、Windows Authenticode 签名以及签名 MSIX 的安装启动卸载流程当前未执行（脚本默认保留 ad-hoc 与 `-SkipSign` 无凭证输出）。
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
