# Rust 宿主与上游 RinUI 功能补全 Plan

> **实施状态（2026-11-20）**：已完成**阶段 0、A、B、C、D、E、F（代码与严格分层 CI 重构）、G（脚本与 manifest/assets）以及独立审查发现的四项修复（P1 QML 回调释放自身/他会话崩溃、P1 共享 Engine 动态窗口越权接管、P1 `ThemeManager` 读写借用保护与 API 安全调整、P2 已有 `Calendar`/`DatePicker`/`SpinBox` 区域实时切换），并通过 macOS `Qt 6.12.0` 离线全套与子进程回归验证；随后补齐外部事件驱动 Session 释放、最后 Runtime 释放两条 P1 的延迟析构，新增 8 个外部事件 worker（共 20 个），本次完整检查结果见验证报告第 9 节**。受限于当前执行环境为无签名证书的 macOS 主机（无 Windows SDK/桌面、无 Linux Wayland/X11 WM 桌面、无 Developer ID / Authenticode 凭证），所有需要 Windows/Linux Runner、真实桌面交互、MSIX 安装启动及正式凭据签名的项均如实保留为 `[ ]` 并在下文标注 `未执行`、缺失前提与复现命令；`upstream/Rin-UI` 与用户自有的 `mtktools/`、`skills/` 均保持完全未动。

> 嵌套事件处理后续补修：上一轮 20/20 未覆盖该路径；本轮 Runtime/Session 两个新增 worker 与原有 20 个均通过。具体实现与本轮检查状态见第 6 节和验证报告第 10 节。

## 1. 目标、基线与范围

目标：补全当前 Rust 宿主相对本地 `upstream/Rin-UI` 的主要能力缺口，同时保留现有独立应用接入方式和生命周期保证。

- 分析基线：宿主 `77b6a17`；上游 Submodule `46d77fa8371ff0e9f46a10a2eb66612dab123269`，RinUI `0.5.0`。
- 当前 115 个运行时资源全部进入 QRC，102 个 QML 文件与上游逐字节一致；Gallery 收录上游 67 个 QML 文件。补全重点是宿主、资源加载和交付流程。
- 上一轮实际通过：`python3 scripts/audit_upstream.py`、`cargo test --workspace --offline`。测试包含 12 个单元测试、3 个 `harness = false` 宿主集成程序及 1 个独立运行测试。
- 开始实施前重新检查 HEAD、工作区、适用的 `AGENTS.md` 和测试结果。当前有用户自有的未跟踪 `mtktools/`、`skills/`，不属于本次范围，不清理、不覆盖。

必须完成的范围：

| 编号 | 工作项 | 完成标志 |
|---|---|---|
| A | 国际化区域与翻译资源回退解耦 | 应用独有语种能正确工作，日期区域与状态一致 |
| B | 内嵌/开发模式的导出与页面来源一致 | 相同组件可导入；修改上游示例后无需重编译即可在下次启动看到 |
| C | 进程级运行时、已有 Qt Application 接入、多 Engine 会话 | 独立与共享 Engine 均可运行，释放互不干扰 |
| D | 通用根对象交互与受控 Engine 扩展 | 支持 QVariant 属性写入、QML 方法调用及必要的 Engine 配置 |
| E | macOS 明确的上游兼容配置 | 两种模式可显式选择，保持现有默认，消除文档中的等价误述 |
| F | 原生窗口自动化与实机验收 | 自动化结果不掩盖原生失败；人工项目有独立状态和证据 |
| G | Rust 产物的 MSIX、签名/公证入口 | 可生成 unsigned MSIX，具备可选凭证驱动的签名流程 |

不在本轮新增：上游没有实现的 DateField、TeachingTip、ScrollViewer、HyperlinkButton；Linux 原生模糊背景；全新控件；上游升级；性能提升承诺。`ErrorPage` 实际存在于 Navigation 目录，不能因错误导出被移除而判断页面缺失。Windows 10 关闭 Acrylic 等上游也可能存在的问题，若发现应单独标注并修复，不误报为移植缺口。

## 2. 全程约束

- 不修改 `upstream/Rin-UI`，包括其 QML、资源、打包文件和 Git 状态；修复放在宿主、生成目录或宿主包装层。
- 保留现有 `RinUIHost::new(HostConfig)` 独立应用路径及默认自绘 macOS 标题栏。优先采用新增 API；确需改变公开语义时同步迁移说明与测试。
- 保留 GUI 主线程约束、`!Send + !Sync`、FFI panic 边界、重入检查、QPointer 和析构顺序。新增句柄必须能检测所属会话已释放。
- Qt Application、默认 QLocale、已安装 QTranslator、Quick Controls style 等具有进程级语义，不能假装成互不影响的 Engine 局部状态。
- 不提交临时测试配置、生成目录、证书、密码、签名凭证或本机绝对路径。测试使用临时目录，不改用户现有配置。
- 不自动升级依赖、打 tag、推送、发布、安装 MSIX 或提交系统信任证书。本计划授权范围是代码、文档、构建及验证产物的准备。

## 3. 执行顺序与阶段任务

推荐顺序：`基线 → A → B → C → D → E → F/G → 文档与最终验证`。A、B 先做成可独立验证的改动；C 完成后必须重新验证 A、B 在多 Engine 下的行为。所有阶段可由同一 Agent 顺序完成。

### 阶段 0：建立可复现基线

- [x] 检查上游 SHA 和干净状态，确认工作区已有修改。（`upstream/Rin-UI` 锁定 `46d77fa8371ff0e9f46a10a2eb66612dab123269` 且干净；宿主起点 `77b6a174`。）
- [x] 执行审计与现有 workspace 测试，记录实际 Qt/Rust/OS 版本。失败时先判断环境问题还是已有缺陷。（macOS arm64，Qt `6.12.0`，`rustc 1.97.1`。）
- [x] 列出 Python 的公开窗口能力、ThemeManager/WinEventManager 的 QML 契约及 Gallery Backend 契约；后续按这些行为验收，避免用 README 宣称替代代码核对。

代码入口：`crates/rinui-host/src/{app,theme,translator,resources,window,error}.rs`、`native/src/`、`examples/gallery/`、`scripts/audit_upstream.py`、`docs/compatibility.md`。

### A：修复国际化语义

现状：`TranslationManager::load_locale()` 使用 RinUI 目录决定 `effective_locale`，C++ 随后据此设置默认 QLocale。应用虽有法语资源，仍可能被报告为英语并使用英语日期格式。Gallery 的语言切换又在调用 Host 前自行检查原始文件名，绕过 Host 的标签规范化。

- [x] 区分规范化的请求语言、应用活动区域、RinUI 实际资源语言和每个应用目录的实际资源语言。RinUI 的 `en_US` 回退不得覆盖合法的应用活动区域。
- [x] 明确 `current_locale()` 与 `TranslationLoadResult`：前者返回活动区域；结果额外提供各目录选中的语言、加载结果和回退状态。保留既有字段时说明其精确含义，避免同名字段静默改变而无迁移说明。
- [x] RinUI 和每个应用目录分别解析候选；支持已有 `zh-CN`、`zh_Hans_CN`、`zh` 行为。使用 Qt 的规范化能力处理合法区域；没有对应翻译不等于区域无效。
- [x] 将 `QLocale::setDefault()` 与 RinUI 目录加载解耦。只有切换成功后才提交新的活动状态；失败保留先前翻译和区域。可选目录缺失时移除其旧翻译，不能残留上一次语言。
- [x] Gallery 使用 Host 返回的规范化状态进行持久化和显示，移除重复的原始文件名判断；启动参数、保存配置与界面语言选择保持一致。
- [x] 保留空英语 `.qm` 在不同 Qt 版本上的兼容，以及翻译器替换、释放和 `retranslate()` 的安全顺序。（已通过 `i18n_semantics` 与 `parity_review_regressions` [`locale-live-embedded` / `locale-live-dev` / `translation-drop`]：切换语言时宿主侧 sanitized `Calendar`/`CalendarDatePicker`/`DatePicker`/`SpinBox` 默认 `locale` 绑定与 `QEvent::LocaleChange` 同步更新且保留显式覆盖，不改 `upstream/Rin-UI`。）

验收：生成最小测试应用 `.qm`，请求 `fr_FR` 且仅应用有法语资源时，应用显示法语、RinUI 回退英语、默认 QLocale/Host 活动区域仍为 `fr_FR`。覆盖标签别名、未知资源语言、可选目录缺失、损坏目录文件、重复切换和失败回滚。不要以“找不到翻译即英语区域”作为预期测试。

入口：`src/translator.rs`、`src/app.rs`、`native/src/rinui_qt_bridge.cpp` 的 translator 函数、`examples/gallery/src/{backend,main,config}.rs`。

### B：统一开发模式与内嵌模式

现状：`build.rs` 只修复内嵌的 `qmldir`；开发模式使用原始声明。Gallery 主入口始终是 QRC，即使启用 `--dev` 也不会从磁盘加载上游示例页面。

- [x] 提取共同的 `qmldir` 清理规则，供构建期和运行期使用；不复制两套容易漂移的修复逻辑。保留缺失条目排除、版本规范化与顶层 ScrollView 补充。（`crates/rinui-host/src/qmldir_sanitize.rs`。）
- [x] 开发模式在启动时建立由会话资源对象持有的临时目录视图，复制最新 QML/JS/字体/图片/翻译和目录结构，仅改写其中的 `qmldir`。整个模块以这个目录作为统一规范 URL 来源，不混合原始目录与影子目录造成单例重复。
- [x] 每次启动重新准备开发视图，确保构建之后修改磁盘源码可在下次启动生效。本轮不要求运行中热更新或文件监听；不得把一次 Cargo 构建生成的旧副本当成开发源码。
- [x] 区分用户指定的源码根和实际加载根，诊断信息同时显示二者。临时目录必须活到所有相关 Engine/根对象释放之后；API 变更避免破坏现有 `ResolvedResources` 的公开匹配方式。
- [x] Gallery 开发视图还包含最新 `examples/` 与宿主包装 QML；在同一目录层级生成 `UpstreamGallery.qml` 别名，保留相对 assets/pages/components 路径。Filesystem 模式从该视图加载包装入口，Embedded 模式继续从 QRC 加载。
- [x] `--dev`、`--dev PATH`、`RINUI_DEV_PATH` 都走相同资源解析。支持传入上游根或 `RinUI/`；仅包含 RinUI 库而没有 examples 的目录仍可供 Host 使用，Gallery 应明确报错或提供显式的示例来源配置（`--examples-dir` / `RINUI_EXAMPLES_DIR`），不悄悄混用内嵌示例。
- [x] Gallery 扩展页、第二窗口、翻译目录、图标等来源保持明确，不在发布二进制写入开发机源码路径。（发布二进制 `rinui-rs/{upstream,crates,examples}` 路径检查为 0；`filesystem_dev_mode`、`embedded_runtime` 与 `standalone_packaging` `2/2` 通过。）

验收：两种模式均能通过 `import RinUI` 实例化 ScrollView；不存在的组件继续不可用。以可写的临时上游副本测试，在构建完成后修改控件和一个示例页的可观察属性，再启动 Gallery，证明读取了修改。检查单例相等、字体、噪点图、动态导航、开发目录带空格和独立发布运行。原始 Submodule 最终仍干净。

入口：`crates/rinui-host/build.rs`、`src/resources.rs`、`examples/gallery/{build.rs,src/main.rs,qml/GalleryMain.qml}`、`tests/filesystem_dev_mode.rs`。

### C：支持已有 Application 与共享/独立 Engine

现状：Application、Engine、ThemeManager、翻译和窗口清理集中在一个 Host 中；C++ 也有全局表。仅删除 `HOST_ACTIVE` 检查会导致一个 Host 析构时释放另一个 Host 的窗口/翻译。

采用的架构方向：

| 层级 | 所有权与职责 |
|---|---|
| 进程级 Runtime | 一个 Qt Application 的拥有/借用关系；应用标识、配置、主题服务、全局翻译、原生过滤器与 Engine 登记 |
| Engine 实例 | QQmlApplicationEngine、导入路径、上下文属性、组件缓存、资源视图；可被多个会话共享 |
| 窗口 Session | 自己创建的 roots、窗口归属、句柄和清理；选择 Runtime 的共享 Engine 或创建独立 Engine |
| 现有 RinUIHost | 保留便捷入口，内部组合独立 Runtime 与默认 Session |

名称可依仓库风格调整；必须先在代码/架构文档中写清所有权关系，再实现 API。

- [x] 新增 Runtime/Session API，允许一个 Runtime 创建多个共享或独立 Engine 会话。独立 `RinUIHost::new()` 的单实例保护保留；需要多个会话的调用方使用 Runtime，不能创建多个 Qt Application。
- [x] 支持借用已有 `QGuiApplication`，兼容其 `QApplication` 子类。仅有 `QCoreApplication`、错误线程、重复绑定等情况返回明确错误。
- [x] 外部 Application 采用带生命周期的借用入口；如果跨语言入口只能提供裸指针，则收敛到有明确安全契约的 `unsafe` 构造。安全 API 不得返回能逃逸外部 Application 生命周期的 Session/Handle。
- [x] 借用模式不销毁外部 Application、不默认启动嵌套事件循环、不在释放时主动退出外部应用。保留并恢复所接管的全局设置，移除自己安装的 translators、filters 与连接，不改动调用方已有对象。
- [x] HiDPI 策略只能在自有 Application 创建前设置；借用模式记录调用方负责前置配置。已有 Quick Controls 已创建时，不能迟到地静默切换全局 style；对不兼容配置返回错误并说明约束。
- [x] 多会话共享进程级主题与活动区域，一次语言切换 retranslate 所有登记的 Engine。独立 Engine 只隔离 QML 上下文/单例实例/缓存，不承诺独立的进程主题、身份或 QLocale。
- [x] Shared Engine 的上下文属性与 import paths 为共享状态，明确重复名称的覆盖/拒绝规则；每窗独立上下文需求使用 Private Engine。
- [x] roots/windows 登记增加 Runtime/Engine/Session 归属。`release_root`、`close_secondary_windows`、`sync_windows` 和句柄校验按归属工作，不能跨 Session 释放或误认其他会话的顶层窗口。动态窗口通过 `_rinuiOwnerSessionId`、`parent`/`transientParent` 与 QML 创建上下文归属确认；共享 Engine 上无法唯一归属（`session_id == 0`）的窗口不由 `sync_windows()` 自动认领，需通过 `register_window_from_property(root_id, property_name)` 显式登记（已在 `parity_review_regressions` `ownership` 中通过回归验证）。
- [x] C++ Windows 的 `WinEventManager` 目前以首个 Engine 为 parent，需改为 Runtime 寿命；共享过滤器、主题指针和 macOS 状态只能由对应层释放。（已在 `rinui_win_bridge.cpp` 与 `rinui_theme_bridge.cpp`/`rinui_mac_window.mm` 实现；`ThemeManager` 借用安全由 `ThemeManagerRef`/`ThemeManagerMut` 读写守卫保证，`theme_manager()` / `theme_manager_mut()` / `with_theme_manager*()` 统一返回 `Result<..., HostError>`；Windows `WinEventManager` 需在 Windows 环境执行编译测试。）
- [x] 拆分会话释放与最后一个 Runtime 释放：前者只清理自己的 roots/连接；后者才清理进程级服务。共享 Engine 的上下文 QObject 保持到 Engine 结束，避免 Session 释放后留下失效上下文。
- [x] 公共句柄校验所有者与存活状态，防止旧窗口/根 ID 在新会话中复用；初始化失败也只能回滚本次创建的资源。（`runtime_sessions`、`core_app_rejection`、`parity_review_regressions` 与 `--features qt-widgets-test --test external_qapplication` 均在 macOS 离线通过。）

验收：同一 Runtime 创建两个 Private Engine，各自同名上下文值不串扰；Shared Engine 按声明共享上下文。关闭任一会话后其他窗口仍可切主题、翻译、导航；最后释放无悬挂过滤器。外部 `QGuiApplication` 与 `QApplication` 各有可运行的集成 fixture，释放 RinUI 后外部 Application 与调用方对象仍存活、事件可继续处理。覆盖失败初始化、错误线程、失效句柄、动态第二窗口、不同析构顺序及现有独立入口回归。

入口：`src/app.rs`（含 `Drop`、guards、`HostSharedInner`）、`src/{bridge,window,error,lib}.rs`、`native/src/rinui_qt_bridge.cpp` 的全局登记与清理、`native/src/rinui_win_bridge.cpp` 的全局管理器、macOS 适配器。Qt Widgets fixture 可通过可选构建配置启用，避免把 Widgets 强加给所有 Quick 应用。

### D：补齐对象交互和 Engine 扩展

- [x] 增加通用 `set_root_property(root_id, name, QVariant)`，保留字符串便捷 API。区分根对象已释放、属性不存在、只读、类型不兼容，不以动态属性写入掩盖拼写错误。
- [x] 增加同步 QML 方法调用，支持 QVariant 可承载的参数与返回值，明确支持的签名边界；通过 Qt 元对象机制调用，不拼接或 eval JavaScript。提供无参数/多参数/返回值用例，以及明确的不存在或不支持错误。
- [x] 提供主线程、受生命周期约束的 Engine 配置扩展入口，优先闭包式访问或有明确 Qt 互操作契约的受控接口，支持配置必要的 Engine 功能。不要通过安全 API 暴露无约束、长期保存的裸 Engine 指针；不能绕过会话注销和资源所有权。（`configure_engine` + `EngineConfigurator` 与 `examples/engine_config_demo.rs`。）
- [x] 对方法调用、属性读写、`load()` 期间、`set_language()` 翻译刷新、`process_events()`/`exec()` 真实事件与 `ThemeManager` 信号引发的 QML→Rust 回调检查重入；释放自身或同侪会话的回调不得导致 C++ 使用已删除 QObject / Engine。跨 FFI 生命周期风险必须用实际回归用例覆盖。（`QmlExecGuard`、`InvokeMethodGuard`、`is_deferred_context()`、`rinui_schedule_deferred_drain` 与 `parity_review_regressions` 20 项隔离子进程+20 秒超时测试。）
- [x] Host、Session、可克隆 Handle 的能力边界一致且有示例；只有窗口属性操作还不等于任意根对象交互。（`object_interop`、`parity_review_regressions` 与 `engine_config_demo` 通过。）

### E：明确 macOS 上游兼容配置

- [x] 保持 `MacWindowOptions::default().enabled == false` 及现有 Gallery 默认。
- [x] 增加明确命名的上游兼容 preset/构造方法：macOS 下启用原生 frame，其他平台保持正常窗口路径；保留现有 native/self-drawn CLI 参数及“最后一个参数生效”的语义。（`MacWindowOptions::upstream_compatible()`、`--upstream-mac-frame`。）
- [x] 删除“默认与上游 Python 完全一致”的表述。准确说明：上游 QML 在 macOS 默认原生模式，Python 的 PyObjC 缺失会触发降级；Rust 默认自绘是当前项目的兼容性选择。
- [x] 主窗口、第二窗口、动态创建窗口使用相同且可诊断的模式策略；选择尽可能在 QML 首帧前完成，失败时明确记录实际回退模式。（`RinUINativeMacFrame` + `_rinuiMacFrameDiagnostic`；Cocoa `.app` 在默认 `self_drawn` 与 `--upstream-mac-frame` `native_cocoa` 双模式实测通过。）
- [ ] 统一默认/原生模式的拖动、双击、搜索框输入、导航返回、缩放、交通灯及退出全屏恢复检查。（代码与自动化初始化/导航/ScrollView/titleBarHeight/多窗口继承已就绪；**最新变动后的人工实机物理鼠标交互、交通灯几何与全屏恢复检查 `未执行`**，需在 macOS 桌面手动启动 `./RinUIGallery.app/Contents/MacOS/RinUIGallery` 及加 `--upstream-mac-frame` 验证。）

### F：原生窗口验证与 CI 证据分层

- [x] 保留 headless offscreen 测试，但将其与 Cocoa/Windows/X11 原生 QPA 检查分开报告。CI 现有原生失败后用 offscreen 兜底的分支不能继续被描述为原生验证成功。（`.github/workflows/ci.yml` 已删除 Cocoa/Windows 的 offscreen 静默兜底。）
- [x] 自动化 smoke 阶段只做启动、资源、窗口属性和可自动判断的行为断言；支持原生桌面的验证任务对原生失败返回失败。确无桌面的环境标记为未运行，保留原因和日志。
- [x] 组件回归至少覆盖本轮修改相关的导航、翻译、ScrollView、窗口生命周期；不以资源文件收录率冒充全控件交互覆盖。（`run_smoke_assertions` 实测 `TextArea.qml` / `ScrollableTextArea`、`unexist/page` `ErrorPage` + `safePop()` 恢复、主题、多窗口、解耦 i18n 并输出 `smoke_summary`。）
- [x] 记录实际结果：OS/Qt/QPA、构建提交、模式、缩放、多屏情况、操作、预期/实际、截图/日志、通过/失败/未执行。（已写入 `docs/validation-report.md` §8。）
- [ ] **Windows 与 Linux 及人工跨平台实机矩阵 `未执行`**：
  - macOS 最新变动后完整实机视觉/全屏交互 QA（需人眼与物理鼠标操作）。
  - Windows 10 Acrylic（含切换回 `"none"` 的 `ACCENT_DISABLED`）、Windows 11 21H2 回退与 22H2+ Snap/三种效果/工作区/自动隐藏任务栏/跨屏 DPI，以及 Post-Parity `windows-latest` CI（需 Windows 物理机或推送至 GitHub Actions：`cargo test --workspace --offline` + `QT_QPA_PLATFORM=windows ./rinui-gallery.exe --smoke-test`）。
  - Linux 真实 X11 WM 与原生 Wayland 合成器实机操作，以及 Post-Parity `ubuntu-latest` CI（需真实 X11/Wayland 会话或推送 CI：`./run-gallery.sh --smoke-test`）。

### G：MSIX 与可选签名/公证流程

- [x] 新增 Rust MSIX 脚本，以 `package_windows.ps1` 的目录产物为输入，完成 staging、宿主维护的 manifest/assets、版本/架构校验、MakeAppx 和可选 SignTool 签名，不引入 PyInstaller/Python 运行时。（`packaging/windows/` + `scripts/package_msix.ps1`。）
- [x] 可参考上游流程，但不直接依赖其 Gallery 专用的硬编码版本、Publisher、测试证书或密码。MSIX Identity、Publisher、版本、架构、输出目录均可配置；验证数值版本与限制，并明确 alpha 版本映射规则。（`packaging/windows/README.md` 与 `ConvertTo-MsixVersion`。）
- [ ] 支持无凭证 `-SkipSign`，CI 生成 unsigned MSIX；解包核对入口 exe、Qt/QML 依赖、图标、许可证和 manifest。目录产物仍需独立 smoke 验证；打包成功不等于 MSIX 安装启动已验证。（脚本与 `.github/workflows/ci.yml` 配置已就绪，因当前为 macOS 主机缺少 `makeappx.exe`/`windeployqt.exe`，**Windows 实际执行与解包验证 `未执行`**，需在 Windows/CI 运行：`.\scripts\package_msix.ps1 -SourceDirectory dist\RinUIGallery-win64 -OutputDirectory dist\msix -SkipSign -VerifyLayout`。）
- [x] 为 Windows 目录/MSIX 增加可选 Authenticode 入口，为 macOS 增加可选 Developer ID codesign、notarytool、stapler 入口。保留无凭证的现有产物路径，检测工具与输入错误并明确失败。（`package_macos.sh` `--help` 与负向 CLI 错误检查已在 macOS 实测通过；`package_windows.ps1`/`package_msix.ps1` 保持 PS 5.1/7 语法兼容。）
- [ ] 生产凭证通过环境/安全存储/CI secrets 注入，不输出密码或打包凭证。未配置凭证时报告签名/公证未运行，不能报告已验证。（脚本层面不回显密码，无凭证路径保持默认 ad-hoc / `-SkipSign`；**实际凭据驱动的签章与公证执行本身 `未执行`**，缺少 Apple Developer ID + notary profile 与 Windows Authenticode 证书。）
- [ ] 在 Windows 验证 unsigned 包布局；有授权桌面与测试凭证时再独立验证安装、启动和卸载。有正式凭证时记录签名/公证结果；缺少外部条件是验收依赖，不是代码已完成的证明。（**`未执行`**，需在 Windows 机器执行打包 -> 签章 -> 安装 MSIX -> 从开始菜单启动 -> 卸载全流程。）

入口：`scripts/package_windows.ps1`、拟新增 `scripts/package_msix.ps1`、`scripts/package_macos.sh`、宿主自己的 packaging 目录、`.github/workflows/ci.yml`、`docs/packaging-and-licensing.md`。

## 4. 合并前检查与交付物

按每阶段实际影响运行相应测试，不必为文档和简单可逆改动机械添加测试。生命周期、翻译和资源来源变化必须有行为回归测试。Qt 集成程序继续遵守主线程和独立进程要求。

最终执行：

```sh
cargo --offline fmt --all
python3 scripts/audit_upstream.py
cargo --offline fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo clippy -p rinui-host --features qt-widgets-test --all-targets --offline -- -D warnings
QT_QPA_PLATFORM=offscreen cargo test --workspace --offline
QT_QPA_PLATFORM=offscreen cargo test -p rinui-host --offline --features qt-widgets-test --test external_qapplication
git diff --check
git -C upstream/Rin-UI status --porcelain
```

根据所处平台运行相应 release 打包与隔离启动检查；依赖未缓存时如实记录离线限制。新增可选 Qt Widgets/打包配置在相应 CI 矩阵中验证；macOS 编译不能代替 Windows MSIX 和真实 Wayland 检查。

交付物：

- [x] A–G 的实现、回归 fixture、运行示例及可复现命令。
- [x] 更新架构图/所有权说明、消费者 API 与迁移指南、开发模式来源说明、兼容矩阵、打包文档及变更日志。
- [x] README 的兼容声明限定为实际覆盖的契约；区分源码复用、自动化通过和交互验收。
- [x] 在验证报告中逐项记录完成状态；在本 Plan 勾选已完成任务，并为外部阻塞项写明缺少的平台/桌面/凭证和后续操作。
- [x] 最终交接说明列出已补齐功能、四项审查修复、API 行为与迁移变化（`ThemeManager` `Result` 返回及 `register_window_from_property`）、实际通过的检查、失败/未运行项目与风险；不因代码已写完就将全部实机/签名验收标记完成。（随本轮最终回复完成。）

建议把改动组织成可单独审阅的提交单元：国际化；开发资源；Runtime/Session；对象交互；macOS 配置；原生验证；MSIX/签名与文档。当前请求仅编写 Plan，不要求创建这些提交。


## 5. 第二轮 P1 生命周期修复（2026-11-20）

- [x] 先补完 C++ `rinui_runtime_release_windows_deferred`，Session 释放始终使用 `release_root_record(rec, true)` / `deleteLater()`，不在回调栈上同步 hide、releaseResources 或 flush。
- [x] Private Engine 始终排入延迟队列，借用外部 app 时守卫退出仅调度；保留 QmlExecGuard、ThemeManager 读写守卫及 mut_slot_active、共享窗口归属和实时区域切换修复。
- [x] Runtime 两阶段析构：立即注销、保存配置、恢复借用状态；保留 Engine / ThemeManager / RuntimeActiveGuard 到全局待清理队列。最终按 deferred deletes → engines → flush → filters → services → ThemeManager → flush → active guard 的顺序排空。
- [x] C++ 合并零延时调度，aboutToQuit 与 qAddPostRoutine 兜底；GUI 已销毁时跳过缓存清理和 GC。外部事件循环排空前重新 attach 返回 ApplicationAlreadyInitialized，排空后允许重新接入。
- [x] 先以 `cargo build --offline -p rinui-host` 确认编译，再运行保留的 12 个和新增的 8 个隔离 worker。20/20 通过，包含 schedule_quit(0) 和无再次 process_events 的 app drop；各 worker 验证回调后 JS 继续、HostDisposed、同侪调用、临时资源目录删除及重新 attach。
- 本轮完整 audit/fmt/clippy/workspace/Qt Widgets 检查的最终状态以 [validation-report.md](validation-report.md#9-second-round-p1-lifecycle-validation-2026-11-20) 为准。Windows/Linux 实机、MSIX 安装启动、Developer ID / Authenticode 签名仍为 **未执行**。本轮未重跑发布打包或 engine_config_demo；此前记录不代表本次执行。
- 不重置、不提交、不推送、不发布；保持 upstream、mtktools、skills 原状。


## 6. 外部回调中嵌套事件处理 P1 补修（2026-11-20）

上一轮 20/20 没有覆盖安全 attach 后，在外部 QML Timer 回调中先释放 Private Session、再调用仍存活的 Runtime/Session process_events 的路径，不能据此判定生命周期修复完整。

- [x] 删除两个 process_events 在 QmlExecGuard 退出后的额外同步 flush；由守卫统一调用 finish_deferred_work_or_schedule，借用 app 时仅调度。独立宿主原有的守卫退出排空行为保留。
- [x] 新增 external-process-nested-pump 与 external-process-nested-pump-session 安全 attach 子进程 worker，各 20 秒超时；同一回调内连续嵌套处理三次事件并验证资源仍存活、后续 JS 执行、HostDisposed、同侪可调用。外层回调退出后，在 Runtime 和同侪尚存活时验证 Private Engine 实际回收，随后验证全部资源删除、重新 attach 与 app 正常析构。workspace 内 22/22 worker 实际通过，无超时或 SIGABRT。
- [x] 按离线/offscreen 约束重跑 audit、fmt、两组 clippy、workspace、Qt Widgets 和 git 检查，全部通过，结果记录在验证报告第 10 节；upstream status --porcelain 为空。
- Windows/Linux 实机、MSIX 安装启动、Developer ID / Authenticode 签名继续标注 **未执行**；不重置、提交、推送或发布，不修改 upstream、mtktools、skills。


## 7. 直接嵌套外部 Application 事件处理 P1（2026-11-20）

前次仅保护 Runtime/Session 包装入口，25 项中的新增外部 app 路径才覆盖无 RinUI 守卫的递归事件处理。22/22 不能证明该路径安全。

- [x] 每个借用 Engine 各自投递 QObject DeferredDelete 标记；仅实际投递后才允许销毁。共享 Engine 进入 Runtime 待拆除队列时同样投递标记，正常最终拆除需全部标记就绪。普通清理回调不再强制 flush borrowed app 的全局 DeferredDeletes，旧唤醒不能提前销毁新 Engine。
- [x] 回到规划检查首轮失败：新增 worker 无 SIGABRT，后续 JS 已执行，但第二阶段回收等待达到 5 秒期限。根据 Qt 文档与 backtrace，将手动 Qt driver 的 DeferredDelete 分发明确放在外层回调退出之后；不在嵌套 app 调用中加守卫或强制删除，不削弱资源保留断言。仅 processEvents 不保证处理所有 DeferredDeletes 的限制必须写入消费者文档。
- [x] 保留原有 22 个 worker，新增 external-process-nested-app-pump、external-process-nested-app-pump-last-private、external-process-nested-app-pump-last-shared，保持每子进程 20 秒超时与完整断言；25/25 已实际通过，成功轮次无 SIGABRT 或子进程超时。
- [x] 完成 audit / fmt / 两组 Clippy / workspace / Qt Widgets / git 检查，全部通过，最终 upstream status 为空。验证报告第 11 节记录逐 worker 结果、首次失败及编译/链接警告；所有外部平台、实机、MSIX、正式签名和 release 打包仍为未执行。
- 不重置、提交、推送、发布，不修改 upstream、mtktools、skills。
