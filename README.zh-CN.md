# rinui-rs

面向 [RinUI](https://github.com/RinLit-233-shiroko/Rin-UI) Qt Quick / QML 库的独立、可复用 Rust + C++/Objective-C++ Qt 6 宿主层。

`rinui-rs` 以原生 Rust/C++ Qt 6 运行时替代 RinUI 原有的 Python / PySide6 运行时（`RinUI/core/`），同时保持 `upstream/Rin-UI` 作为锁定提交的 Git Submodule **完全不被修改**。

---

## 核心特性

- **零修改上游 Submodule**：`upstream/Rin-UI` 保持纯净（`46d77fa8371ff0e9f46a10a2eb66612dab123269`），缺失或无版本号的 `qmldir` 条目在构建阶段于 `$OUT_DIR/rinui_sanitized/` 中修正。
- **完整保留 RinUI QML 契约**：提供完整的 `ThemeManager`（`themeChanged`、`backdropChanged` 信号与全部 `toggle_theme` / `get_theme` / `apply_backdrop_effect` / `set_theme_color` 等槽方法）及 Windows `WinEventManager`。
- **Qt 6 原生颜色方案监听**：使用 `QStyleHints::colorSchemeChanged` 信号替代 Python 端 1 秒轮询 `darkdetect`。
- **应用隔离与原子持久化（`rinui-platform`）**：支持基于 `AppIdentity` 的跨平台配置目录隔离、原子写入、旧版 Python `rin_ui.json` 迁移与损坏配置备份恢复。
- **国际化支持**：系统区域检测、嵌入式 QRC / 开发文件系统 `.qm` 加载、`en_US` 回退、自定义应用翻译目录及运行时 `retranslate()`。
- **多窗口与安全所有权**：`!Send + !Sync` 的 `RinUIHost` 与 `HostHandle`、基于 `QPointer` 的安全句柄表、严格析构顺序以及 `WindowCustomizer` 扩展钩子。
- **双资源加载模式**：
  - **内嵌发布模式（`ResourceMode::Embedded`）**：通过 Qt `rcc` 将全部 QML、JS、字体、图片及 `.qm` 打包到 `qrc:/rinui/RinUI`，发布二进制不依赖源码目录。
  - **开发文件系统模式（`ResourceMode::Filesystem` / `--dev` / `RINUI_DEV_PATH`）**：直接从本地目录加载 RinUI，且不会被 QRC 遮蔽。

---

## 版本矩阵

| 组件 | 版本 |
|---|---|
| 上游 RinUI（`upstream/Rin-UI`） | `46d77fa8371ff0e9f46a10a2eb66612dab123269`（`beta-0.5.0-16-g46d77fa`，`0.5.0`） |
| Rust 工具链 | `rustc 1.97.1`（MSRV `1.85`） |
| `cxx-qt` / `cxx-qt-lib` / `cxx-qt-build` / `qt-build-utils` | `0.10.0` |
| Qt 6 | 本地验证 `Qt 6.12.0`（macOS Homebrew）；CI 使用 `Qt 6.9.3`（需 `Qt5Compat` 模块） |
| CMake | `>= 3.24` |

---

## 快速开始

### 1. 克隆仓库（含 Submodule）

```bash
git clone --recurse-submodules <repo-url>
cd rinui-rs
```

### 2. 审计、测试与运行 Gallery

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

### 3. 发布打包

```bash
cargo build --release -p rinui-gallery

# macOS:
./scripts/package_macos.sh target/release/rinui-gallery dist
# Windows (PowerShell):
.\scripts\package_windows.ps1 -BinaryPath target\release\rinui-gallery.exe -OutputDir dist\RinUIGallery-win64
# Linux:
./scripts/package_linux.sh target/release/rinui-gallery dist/rinui-gallery-linux
```

更多文档请参阅：
- [`docs/compatibility.md`](docs/compatibility.md) — 上游审计与兼容性矩阵
- [`docs/architecture.md`](docs/architecture.md) — 架构与内存/生命周期模型
- [`docs/consumer-guide.md`](docs/consumer-guide.md) — 下游 Rust 应用（如 Goosy）接入指南
- [`docs/packaging-and-licensing.md`](docs/packaging-and-licensing.md) — 跨平台打包与 Qt/MIT 许可说明
- [`docs/updating-upstream.md`](docs/updating-upstream.md) — 上游 RinUI Submodule 升级流程
