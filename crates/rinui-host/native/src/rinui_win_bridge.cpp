#include "rinui_win_bridge.h"
#include "rinui_platform_hooks.h"
#ifdef _WIN32
#include "rinui-host/src/theme.cxxqt.h"
#endif

#include <QElapsedTimer>
#include <QGuiApplication>
#include <QHash>
#include <QMetaObject>
#include <QPointF>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QRectF>
#include <QScreen>
#include <QTimer>

#include <cstring>
#include <memory>
#include <utility>

#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <windowsx.h>
#include <dwmapi.h>
#include <shellapi.h>
#endif

namespace {

QPointer<QObject> s_theme_manager;

#ifdef _WIN32

constexpr DWORD kDwmwaNcrenderingPolicy = 2;
constexpr DWORD kDwmNcrpEnabled = 2;
constexpr DWORD kDwmwaUseImmersiveDarkMode = 20;
constexpr DWORD kDwmwaUseImmersiveDarkModeLegacy = 19;
constexpr DWORD kDwmwaWindowCornerPreference = 33;
constexpr DWORD kDwmwcpRound = 2;
constexpr DWORD kDwmwaBorderColor = 34;
constexpr DWORD kDwmwaCaptionColor = 35;
constexpr DWORD kDwmwaSystemBackdropType = 38;
constexpr DWORD kDwmwaMicaEffect = 1029;
constexpr DWORD kDwmColorNone = 0xFFFFFFFE;

constexpr int kWcaAccentPolicy = 19;
constexpr int kDefaultTitleBarHeight = 32;
constexpr int kResizeBorder = 8;
constexpr qreal kButtonWidthFallback = 144.0;
constexpr qreal kSafeWidthFallback = 32.0;
constexpr qint64 kInteractiveCacheTtlMs = 200;

struct AccentPolicy {
    int accent_state;
    int accent_flags;
    int gradient_color;
    int animation_id;
};

struct WindowCompositionAttribData {
    int attrib;
    PVOID pv_data;
    SIZE_T cb_data;
};

using SetWindowCompositionAttributeFn = BOOL(WINAPI*)(HWND, WindowCompositionAttribData*);

struct WindowsBuildInfo {
    DWORD major = 0;
    DWORD minor = 0;
    DWORD build = 0;
};

WindowsBuildInfo query_windows_version()
{
    static WindowsBuildInfo cached = []() {
        WindowsBuildInfo info;
        using RtlGetVersionFn = LONG(WINAPI*)(PRTL_OSVERSIONINFOW);
        if (HMODULE ntdll = GetModuleHandleW(L"ntdll.dll")) {
            auto rtl_get_version =
                reinterpret_cast<RtlGetVersionFn>(GetProcAddress(ntdll, "RtlGetVersion"));
            if (rtl_get_version) {
                RTL_OSVERSIONINFOW osvi{};
                osvi.dwOSVersionInfoSize = sizeof(osvi);
                if (rtl_get_version(&osvi) == 0) {
                    info.major = osvi.dwMajorVersion;
                    info.minor = osvi.dwMinorVersion;
                    info.build = osvi.dwBuildNumber;
                }
            }
        }
        return info;
    }();
    return cached;
}

bool is_win10()
{
    const auto v = query_windows_version();
    return v.major > 10 || (v.major == 10 && v.build >= 10240);
}

bool is_win11()
{
    const auto v = query_windows_version();
    return v.major > 10 || (v.major == 10 && v.build >= 22000);
}

bool is_win11_22h2()
{
    const auto v = query_windows_version();
    return v.major > 10 || (v.major == 10 && v.build >= 22621);
}

bool is_windows_qpa()
{
    return QGuiApplication::platformName() == QStringLiteral("windows");
}

bool is_hwnd_maximized(HWND hwnd)
{
    WINDOWPLACEMENT placement{};
    placement.length = sizeof(WINDOWPLACEMENT);
    if (!GetWindowPlacement(hwnd, &placement)) {
        return false;
    }
    return placement.showCmd == SW_MAXIMIZE;
}

bool is_dwm_composition_enabled()
{
    BOOL enabled = FALSE;
    if (SUCCEEDED(DwmIsCompositionEnabled(&enabled))) {
        return enabled != FALSE;
    }
    return false;
}

void refresh_window_frame(HWND hwnd)
{
    SetWindowPos(
        hwnd,
        nullptr,
        0,
        0,
        0,
        0,
        SWP_NOSIZE | SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
}

void set_window_styles(QQuickWindow* window, HWND hwnd)
{
    const LONG_PTR style = GetWindowLongPtrW(hwnd, GWL_STYLE);
    const LONG_PTR new_style = style | WS_CAPTION | WS_THICKFRAME;
    if (new_style == style) {
        return;
    }
    SetWindowLongPtrW(hwnd, GWL_STYLE, new_style);
    window->setProperty("_rinuiWindowStyleChanged", true);
    refresh_window_frame(hwnd);
}

void extend_frame_into_client_area(QQuickWindow* window, HWND hwnd)
{
    if (!window || !window->property("backdropEnabled").toBool()) {
        return;
    }
    if (!is_dwm_composition_enabled()) {
        return;
    }
    const MARGINS margins{-1, -1, -1, -1};
    DwmExtendFrameIntoClientArea(hwnd, &margins);
}

bool is_exact_monitor_sized_window(QQuickWindow* window, HWND hwnd)
{
    if (!window) {
        return false;
    }
    HMONITOR monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    if (!monitor) {
        return false;
    }
    MONITORINFO mi{};
    mi.cbSize = sizeof(MONITORINFO);
    if (!GetMonitorInfoW(monitor, &mi)) {
        return false;
    }

    RECT rect{};
    GetWindowRect(hwnd, &rect);
    if (rect.left == mi.rcMonitor.left && rect.top == mi.rcMonitor.top
        && rect.right == mi.rcMonitor.right && rect.bottom == mi.rcMonitor.bottom) {
        return true;
    }

    const qreal ratio = window->screen() ? window->screen()->devicePixelRatio() : window->devicePixelRatio();
    const int w = qRound(window->width() * ratio);
    const int h = qRound(window->height() * ratio);
    return w == (mi.rcMonitor.right - mi.rcMonitor.left)
        && h == (mi.rcMonitor.bottom - mi.rcMonitor.top);
}

void apply_fullscreen_opengl_border_workaround(QQuickWindow* window, HWND hwnd)
{
    if (!window || !window->property("enableFullscreenOpenGLBorderWorkaround").toBool()) {
        return;
    }
    if (!is_exact_monitor_sized_window(window, hwnd)) {
        return;
    }
    const LONG_PTR style = GetWindowLongPtrW(hwnd, GWL_STYLE);
    if (style & WS_BORDER) {
        return;
    }
    SetWindowLongPtrW(hwnd, GWL_STYLE, style | WS_BORDER);
    refresh_window_frame(hwnd);
}

int system_metric_for_hwnd(HWND hwnd, int index)
{
    using GetDpiForWindowFn = UINT(WINAPI*)(HWND);
    using GetSystemMetricsForDpiFn = int(WINAPI*)(int, UINT);
    if (hwnd) {
        if (HMODULE user32 = GetModuleHandleW(L"user32.dll")) {
            auto get_dpi = reinterpret_cast<GetDpiForWindowFn>(
                GetProcAddress(user32, "GetDpiForWindow"));
            auto get_metrics_dpi = reinterpret_cast<GetSystemMetricsForDpiFn>(
                GetProcAddress(user32, "GetSystemMetricsForDpi"));
            if (get_dpi && get_metrics_dpi) {
                const UINT dpi = get_dpi(hwnd);
                if (dpi > 0) {
                    return get_metrics_dpi(index, dpi);
                }
            }
        }
    }
    return GetSystemMetrics(index);
}

int get_resize_border_thickness(QQuickWindow* window, HWND hwnd, bool horizontal)
{
    const int frame_metric = horizontal ? SM_CXSIZEFRAME : SM_CYSIZEFRAME;
    const int result = system_metric_for_hwnd(hwnd, frame_metric)
        + system_metric_for_hwnd(hwnd, SM_CXPADDEDBORDER);
    if (result > 0) {
        return result;
    }
    const int thickness = is_dwm_composition_enabled() ? 8 : 4;
    const qreal ratio = window ? window->devicePixelRatio() : 1.0;
    return qRound(thickness * (ratio > 0.0 ? ratio : 1.0));
}

UINT taskbar_edge(HWND hwnd)
{
    HWND tray = FindWindowW(L"Shell_TrayWnd", nullptr);
    if (!tray) {
        return static_cast<UINT>(-1);
    }
    HMONITOR win_mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    HMONITOR tray_mon = MonitorFromWindow(tray, MONITOR_DEFAULTTOPRIMARY);
    if (!win_mon || win_mon != tray_mon) {
        return static_cast<UINT>(-1);
    }
    APPBARDATA abd{};
    abd.cbSize = sizeof(APPBARDATA);
    abd.hWnd = tray;
    SHAppBarMessage(ABM_GETTASKBARPOS, &abd);
    return abd.uEdge;
}

qreal dip_property(QQuickWindow* window, const char* name, qreal fallback)
{
    if (!window) {
        return fallback;
    }
    const QVariant v = window->property(name);
    if (!v.isValid()) {
        return fallback;
    }
    bool ok = false;
    const qreal d = v.toReal(&ok);
    return (ok && d > 0.0) ? d : fallback;
}

int title_bar_height_px(QQuickWindow* window, qreal dp_ratio)
{
    return static_cast<int>(dip_property(window, "titleBarHeight", kDefaultTitleBarHeight) * dp_ratio);
}

int button_width_px(QQuickWindow* window, qreal dp_ratio)
{
    return static_cast<int>(dip_property(window, "titleBarButtonWidth", kButtonWidthFallback) * dp_ratio);
}

int safe_width_px(QQuickWindow* window, qreal dp_ratio)
{
    return static_cast<int>(dip_property(window, "titleBarSafeWidth", kSafeWidthFallback) * dp_ratio);
}

bool max_button_rect_px(QQuickWindow* window, qreal dp_ratio, QRectF& out_rect)
{
    if (!window) {
        return false;
    }
    const QVariant v = window->property("titleBarMaxButtonRect");
    if (!v.canConvert<QRectF>()) {
        return false;
    }
    const QRectF r = v.toRectF();
    if (r.width() <= 1.0 || r.height() <= 1.0) {
        return false;
    }
    out_rect = QRectF(
        r.x() * dp_ratio,
        r.y() * dp_ratio,
        r.width() * dp_ratio,
        r.height() * dp_ratio);
    return true;
}

bool inside_rect(const QRectF& r, qreal x, qreal y, qreal slack = 0.0)
{
    return (r.x() - slack) <= x && x < (r.x() + r.width() + slack)
        && (r.y() - slack) <= y && y < (r.y() + r.height() + slack);
}

void set_window_bool_property(QQuickWindow* window, const char* name, bool value)
{
    if (!window) {
        return;
    }
    if (window->property(name).toBool() == value) {
        return;
    }
    window->setProperty(name, value);
}

bool is_plain_text_item(const QQuickItem* item)
{
    if (!item || !item->metaObject()) {
        return false;
    }
    const char* cls = item->metaObject()->className();
    if (!cls) {
        return false;
    }
    return std::strcmp(cls, "QQuickText") == 0
        || std::strcmp(cls, "QQuickPlaceholderText") == 0
        || std::strncmp(cls, "Text_QMLTYPE", 12) == 0
        || std::strncmp(cls, "Label_QMLTYPE", 13) == 0;
}

void collect_interactive_from(
    QQuickItem* item,
    QQuickItem* root,
    QList<QRectF>& out,
    int depth)
{
    if (!item || depth > 24) {
        return;
    }
    if (!item->isVisible() || !item->isEnabled()) {
        return;
    }
    const qreal w = item->width();
    const qreal h = item->height();
    if (w <= 1.0 || h <= 1.0) {
        return;
    }

    if ((item->acceptedMouseButtons() & Qt::LeftButton) && !is_plain_text_item(item)) {
        const QPointF pos = item->mapToItem(root, QPointF(0.0, 0.0));
        out.append(QRectF(pos.x(), pos.y(), w, h));
        return;
    }

    const auto children = item->childItems();
    for (QQuickItem* child : children) {
        collect_interactive_from(child, root, out, depth + 1);
    }
}

struct CachedInteractiveRects {
    int width = 0;
    int height = 0;
    qreal dpr = 1.0;
    qint64 timestamp_ms = 0;
    QList<QRectF> rects;
};

QHash<QQuickWindow*, CachedInteractiveRects> s_interactive_cache;
QElapsedTimer s_cache_clock;

QList<QRectF> interactive_rects_px(QQuickWindow* window, qreal dp_ratio)
{
    if (!s_cache_clock.isValid()) {
        s_cache_clock.start();
    }
    const qint64 now = s_cache_clock.elapsed();
    auto it = s_interactive_cache.constFind(window);
    if (it != s_interactive_cache.cend()
        && it->width == window->width()
        && it->height == window->height()
        && qFuzzyCompare(it->dpr, dp_ratio)
        && (now - it->timestamp_ms) < kInteractiveCacheTtlMs) {
        return it->rects;
    }

    QList<QRectF> dip_rects;
    QQuickItem* content_root = window->contentItem();
    if (content_root) {
        const char* host_names[] = {"titleBarLeadingHost", "titleBarHost"};
        for (const char* name : host_names) {
            QObject* host_obj = window->property(name).value<QObject*>();
            if (auto* host_item = qobject_cast<QQuickItem*>(host_obj)) {
                collect_interactive_from(host_item, content_root, dip_rects, 0);
            }
        }
    }

    QList<QRectF> px_rects;
    px_rects.reserve(dip_rects.size());
    for (const QRectF& r : dip_rects) {
        px_rects.append(QRectF(
            r.x() * dp_ratio,
            r.y() * dp_ratio,
            r.width() * dp_ratio,
            r.height() * dp_ratio));
    }

    CachedInteractiveRects entry;
    entry.width = window->width();
    entry.height = window->height();
    entry.dpr = dp_ratio;
    entry.timestamp_ms = now;
    entry.rects = px_rects;
    s_interactive_cache.insert(window, entry);
    return px_rects;
}

void invalidate_interactive_rects(QQuickWindow* window = nullptr)
{
    if (!window) {
        s_interactive_cache.clear();
    } else {
        s_interactive_cache.remove(window);
    }
}

bool pointer_over_max_button(QQuickWindow* window, HWND hwnd)
{
    const qreal dp_ratio = window->screen() ? window->screen()->devicePixelRatio() : 1.0;
    QRectF rect;
    if (!max_button_rect_px(window, dp_ratio, rect)) {
        return false;
    }
    POINT pt{};
    if (!GetCursorPos(&pt)) {
        return false;
    }
    POINT origin{0, 0};
    ClientToScreen(hwnd, &origin);
    return inside_rect(rect, pt.x - origin.x, pt.y - origin.y, 2.0);
}

void toggle_maximize(QQuickWindow* window, HWND hwnd)
{
    if (!window) {
        return;
    }
    if (is_hwnd_maximized(hwnd)) {
        window->showNormal();
    } else {
        window->showMaximized();
    }
}

class WinEventFilter final : public QAbstractNativeEventFilter {
public:
    void add_window(QQuickWindow* window)
    {
        if (!window) {
            return;
        }
        if (!window->property("_rinuiWinVisibleHooked").toBool()) {
            window->setProperty("_rinuiWinVisibleHooked", true);
            QPointer<QQuickWindow> safe_win(window);
            QObject::connect(window, &QQuickWindow::visibleChanged, window, [safe_win](bool visible) {
                if (visible && !safe_win.isNull()) {
                    rinui::platform::setup_window(safe_win.data(), false, 0, 0);
                }
            });
        }

        const HWND hwnd = reinterpret_cast<HWND>(window->winId());
        if (!hwnd) {
            return;
        }

        for (auto it = m_windows.begin(); it != m_windows.end();) {
            if (it.value().isNull() || (it.value().data() == window && it.key() != hwnd)) {
                if (m_max_button_pressed_hwnd == it.key()) {
                    m_max_button_pressed_hwnd = nullptr;
                }
                it = m_windows.erase(it);
            } else {
                ++it;
            }
        }

        m_windows.insert(hwnd, QPointer<QQuickWindow>(window));
        set_window_bool_property(window, "snapLayoutsSupported", is_win11());
        sync_window_backdrop(window, hwnd);
    }

    void remove_window(QQuickWindow* window)
    {
        if (!window) {
            return;
        }
        invalidate_interactive_rects(window);
        for (auto it = m_windows.begin(); it != m_windows.end();) {
            if (it.value().isNull() || it.value().data() == window) {
                if (m_max_button_pressed_hwnd == it.key()) {
                    m_max_button_pressed_hwnd = nullptr;
                }
                it = m_windows.erase(it);
            } else {
                ++it;
            }
        }
    }

    void clear()
    {
        m_windows.clear();
        m_max_button_pressed_hwnd = nullptr;
        invalidate_interactive_rects(nullptr);
    }

    bool empty() const
    {
        return m_windows.isEmpty();
    }

    QList<HWND> hwnds() const
    {
        QList<HWND> out;
        for (auto it = m_windows.cbegin(); it != m_windows.cend(); ++it) {
            if (!it.value().isNull()) {
                out.append(it.key());
            }
        }
        return out;
    }

    void sync_window_backdrop(QQuickWindow* window, HWND hwnd)
    {
        if (!window || !hwnd) {
            return;
        }
        set_window_styles(window, hwnd);
        extend_frame_into_client_area(window, hwnd);
        apply_fullscreen_opengl_border_workaround(window, hwnd);
    }

    bool nativeEventFilter(const QByteArray& eventType, void* message, qintptr* result) override
    {
        if (eventType != "windows_generic_MSG" && eventType != "windows_dispatcher_MSG") {
            return false;
        }
        if (!message || !result) {
            return false;
        }

        const MSG* msg = static_cast<const MSG*>(message);
        auto it = m_windows.constFind(msg->hwnd);
        if (it == m_windows.cend() || it.value().isNull()) {
            return false;
        }

        return dispatch(it.value().data(), msg->hwnd, msg->message, msg->wParam, msg->lParam, result);
    }

private:
    bool dispatch(
        QQuickWindow* window,
        HWND hwnd,
        UINT msg_id,
        WPARAM w_param,
        LPARAM l_param,
        qintptr* result)
    {
        if (msg_id == WM_ENTERSIZEMOVE) {
            m_in_move_loop = true;
        } else if (msg_id == WM_EXITSIZEMOVE) {
            m_in_move_loop = false;
            invalidate_interactive_rects(window);
        } else if (msg_id == WM_SIZE || msg_id == WM_SHOWWINDOW || msg_id == WM_DPICHANGED) {
            invalidate_interactive_rects(window);
        }

        if ((msg_id == WM_NCLBUTTONDOWN || msg_id == WM_NCLBUTTONDBLCLK) && w_param == HTMAXBUTTON) {
            m_max_button_pressed_hwnd = hwnd;
            set_window_bool_property(window, "titleBarMaxButtonPressed", true);
            SetCapture(hwnd);
            *result = 0;
            return true;
        }

        if (msg_id == WM_NCLBUTTONUP || msg_id == WM_LBUTTONUP) {
            if (m_max_button_pressed_hwnd == hwnd) {
                m_max_button_pressed_hwnd = nullptr;
                set_window_bool_property(window, "titleBarMaxButtonPressed", false);
                ReleaseCapture();
                if (pointer_over_max_button(window, hwnd)) {
                    QPointer<QQuickWindow> safe_win(window);
                    QTimer::singleShot(0, window, [safe_win, hwnd]() {
                        if (!safe_win.isNull()) {
                            toggle_maximize(safe_win.data(), hwnd);
                        }
                    });
                }
                *result = 0;
                return true;
            }
        }

        if ((msg_id == WM_LBUTTONDOWN || msg_id == WM_CANCELMODE || msg_id == WM_CAPTURECHANGED)
            && m_max_button_pressed_hwnd == hwnd) {
            m_max_button_pressed_hwnd = nullptr;
            set_window_bool_property(window, "titleBarMaxButtonPressed", false);
            if (msg_id != WM_CAPTURECHANGED) {
                ReleaseCapture();
            }
        }

        if (msg_id == WM_NCMOUSEMOVE) {
            set_window_bool_property(
                window, "titleBarMaxButtonHovered", pointer_over_max_button(window, hwnd));
        } else if (msg_id == WM_NCMOUSELEAVE || msg_id == WM_MOUSELEAVE) {
            set_window_bool_property(window, "titleBarMaxButtonHovered", false);
        }

        if (msg_id == WM_SIZE || msg_id == WM_WINDOWPOSCHANGED || msg_id == WM_SHOWWINDOW
            || msg_id == WM_ACTIVATE || msg_id == WM_NCACTIVATE || msg_id == WM_ACTIVATEAPP
            || msg_id == WM_DWMCOMPOSITIONCHANGED) {
            extend_frame_into_client_area(window, hwnd);
        }

        if (msg_id == WM_SIZE || msg_id == WM_WINDOWPOSCHANGED || msg_id == WM_DWMCOMPOSITIONCHANGED) {
            apply_fullscreen_opengl_border_workaround(window, hwnd);
        }

        if (msg_id == WM_NCHITTEST) {
            LRESULT code = 0;
            if (hit_test(window, hwnd, l_param, code)) {
                set_window_bool_property(window, "titleBarMaxButtonHovered", code == HTMAXBUTTON);
                *result = static_cast<qintptr>(code);
                return true;
            }
            set_window_bool_property(window, "titleBarMaxButtonHovered", false);
            return false;
        }

        if (msg_id == WM_NCCALCSIZE && w_param) {
            return apply_nccalcsize(window, hwnd, l_param, result);
        }

        if (msg_id == WM_GETMINMAXINFO) {
            return apply_getminmaxinfo(window, hwnd, l_param, result);
        }

        return false;
    }

    bool hit_test(QQuickWindow* window, HWND hwnd, LPARAM l_param, LRESULT& out_code) const
    {
        const int x = GET_X_LPARAM(l_param);
        const int y = GET_Y_LPARAM(l_param);

        qreal dp_ratio =
            window->screen() ? window->screen()->devicePixelRatio() : window->devicePixelRatio();
        if (dp_ratio <= 0.0) {
            dp_ratio = 1.0;
        }

        if (!is_hwnd_maximized(hwnd)) {
            RECT rect{};
            GetWindowRect(hwnd, &rect);
            const int left = rect.left;
            const int top = rect.top;
            const int right = rect.right;
            const int bottom = rect.bottom;
            const int resize_border = qMax(kResizeBorder, qRound(kResizeBorder * dp_ratio));

            if (left <= x && x < left + resize_border) {
                if (top <= y && y < top + resize_border) {
                    out_code = HTTOPLEFT;
                    return true;
                }
                if (bottom - resize_border <= y && y < bottom) {
                    out_code = HTBOTTOMLEFT;
                    return true;
                }
                out_code = HTLEFT;
                return true;
            }
            if (right - resize_border <= x && x < right) {
                if (top <= y && y < top + resize_border) {
                    out_code = HTTOPRIGHT;
                    return true;
                }
                if (bottom - resize_border <= y && y < bottom) {
                    out_code = HTBOTTOMRIGHT;
                    return true;
                }
                out_code = HTRIGHT;
                return true;
            }
            if (top <= y && y < top + resize_border) {
                out_code = HTTOP;
                return true;
            }
            if (bottom - resize_border <= y && y < bottom) {
                out_code = HTBOTTOM;
                return true;
            }
        }

        POINT origin{0, 0};
        ClientToScreen(hwnd, &origin);
        RECT client_rect{};
        GetClientRect(hwnd, &client_rect);
        const int client_w = client_rect.right - client_rect.left;

        const qreal cx = static_cast<qreal>(x - origin.x);
        const qreal cy = static_cast<qreal>(y - origin.y);
        const int tb_height = title_bar_height_px(window, dp_ratio);

        if (!(0.0 <= cy && cy < static_cast<qreal>(tb_height)
                && 0.0 <= cx && cx < static_cast<qreal>(client_w))) {
            return false;
        }

        QRectF max_rect;
        if (!m_in_move_loop && max_button_rect_px(window, dp_ratio, max_rect)
            && inside_rect(max_rect, cx, cy, 2.0)) {
            out_code = HTMAXBUTTON;
            return true;
        }

        const int button_px = button_width_px(window, dp_ratio);
        const int drag_limit = client_w - button_px - safe_width_px(window, dp_ratio);

        if (!m_in_move_loop && cx < drag_limit) {
            const auto rects = interactive_rects_px(window, dp_ratio);
            for (const QRectF& r : rects) {
                const qreal max_x = qMin(r.x() + r.width(), static_cast<qreal>(drag_limit));
                if (r.x() <= cx && cx < max_x && r.y() <= cy && cy < (r.y() + r.height())) {
                    out_code = HTCLIENT;
                    return true;
                }
            }
        }

        out_code = (cx < static_cast<qreal>(client_w - button_px)) ? HTCAPTION : HTCLIENT;
        return true;
    }

    static bool apply_nccalcsize(
        QQuickWindow* window,
        HWND hwnd,
        LPARAM l_param,
        qintptr* result)
    {
        auto* params = reinterpret_cast<NCCALCSIZE_PARAMS*>(l_param);
        if (!params) {
            return false;
        }
        RECT& client_rect = params->rgrc[0];
        if (is_hwnd_maximized(hwnd)) {
            const int ty = get_resize_border_thickness(window, hwnd, false);
            const int tx = get_resize_border_thickness(window, hwnd, true);
            client_rect.top += ty;
            client_rect.bottom -= ty;
            client_rect.left += tx;
            client_rect.right -= tx;

            APPBARDATA abd{};
            abd.cbSize = sizeof(APPBARDATA);
            const UINT_PTR state = SHAppBarMessage(ABM_GETSTATE, &abd);
            if (state & ABS_AUTOHIDE) {
                const UINT edge = taskbar_edge(hwnd);
                if (edge == ABE_TOP) {
                    client_rect.top += 1;
                } else if (edge == ABE_LEFT) {
                    client_rect.left += 1;
                } else if (edge == ABE_RIGHT) {
                    client_rect.right -= 1;
                } else if (edge == ABE_BOTTOM) {
                    client_rect.bottom -= 1;
                }
            }
        }
        *result = 0;
        return true;
    }

    static bool apply_getminmaxinfo(QQuickWindow* window, HWND hwnd, LPARAM l_param, qintptr* result)
    {
        HMONITOR monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        MONITORINFO mi{};
        mi.cbSize = sizeof(MONITORINFO);
        if (!monitor || !GetMonitorInfoW(monitor, &mi)) {
            return false;
        }
        auto* mmi = reinterpret_cast<MINMAXINFO*>(l_param);
        if (!mmi) {
            return false;
        }

        mmi->ptMaxPosition.x = mi.rcWork.left - mi.rcMonitor.left;
        mmi->ptMaxPosition.y = mi.rcWork.top - mi.rcMonitor.top;
        mmi->ptMaxSize.x = mi.rcWork.right - mi.rcWork.left;
        mmi->ptMaxSize.y = mi.rcWork.bottom - mi.rcWork.top;

        qreal dp_ratio =
            window->screen() ? window->screen()->devicePixelRatio() : window->devicePixelRatio();
        if (dp_ratio <= 0.0) {
            dp_ratio = 1.0;
        }

        mmi->ptMinTrackSize.x = qRound(window->minimumWidth() * dp_ratio);
        mmi->ptMinTrackSize.y = qRound(window->minimumHeight() * dp_ratio);

        constexpr int kQWindowSizeMax = 16777215;
        if (window->maximumWidth() > 0 && window->maximumWidth() < kQWindowSizeMax) {
            mmi->ptMaxTrackSize.x = qRound(window->maximumWidth() * dp_ratio);
        }
        if (window->maximumHeight() > 0 && window->maximumHeight() < kQWindowSizeMax) {
            mmi->ptMaxTrackSize.y = qRound(window->maximumHeight() * dp_ratio);
        }

        *result = 0;
        return true;
    }

    QHash<HWND, QPointer<QQuickWindow>> m_windows;
    bool m_in_move_loop = false;
    HWND m_max_button_pressed_hwnd = nullptr;
};

WinEventFilter* s_event_filter = nullptr;
bool s_filter_installed = false;
std::unique_ptr<rinui::win::WinEventManager> s_event_manager;
QString s_last_backdrop_effect = QStringLiteral("none");
uint32_t s_last_win10_light = 0xA6FFFFFFu;
uint32_t s_last_win10_dark = 0xA6000000u;

void ensure_filter_installed()
{
    if (!s_event_filter) {
        s_event_filter = new WinEventFilter();
    }
    if (!s_filter_installed) {
        if (auto* app = QGuiApplication::instance()) {
            app->installNativeEventFilter(s_event_filter);
            s_filter_installed = true;
        }
    }
}

void remove_filter_if_installed()
{
    if (s_filter_installed && s_event_filter) {
        if (auto* app = QGuiApplication::instance()) {
            app->removeNativeEventFilter(s_event_filter);
        }
        s_filter_installed = false;
    }
    delete s_event_filter;
    s_event_filter = nullptr;
}

void install_transparent_render_clear(QQuickWindow* window)
{
    if (!window || !window->property("backdropEnabled").toBool()) {
        return;
    }
    const QColor transparent(0, 0, 0, 0);
    window->setColor(transparent);

    if (window->property("_rinuiTransparentRenderClearInstalled").toBool()) {
        return;
    }
    window->setProperty("_rinuiTransparentRenderClearInstalled", true);

    QPointer<QQuickWindow> safe_win(window);
    auto enforce = [safe_win, transparent]() {
        if (safe_win.isNull() || !safe_win->property("backdropEnabled").toBool()) {
            return;
        }
        safe_win->setColor(transparent);
    };

    QObject::connect(window, &QQuickWindow::beforeFrameBegin, window, enforce);
    QObject::connect(window, &QQuickWindow::beforeRendering, window, enforce);
    QObject::connect(window, &QQuickWindow::beforeRenderPassRecording, window, enforce);
}

void apply_window_rounded_and_shadow(HWND hwnd)
{
    if (!hwnd) {
        return;
    }
    const int ncrp = static_cast<int>(kDwmNcrpEnabled);
    DwmSetWindowAttribute(hwnd, kDwmwaNcrenderingPolicy, &ncrp, sizeof(ncrp));

    if (is_win11()) {
        const int corner = static_cast<int>(kDwmwcpRound);
        DwmSetWindowAttribute(hwnd, kDwmwaWindowCornerPreference, &corner, sizeof(corner));
    }
}

void apply_dwm_dark_mode(HWND hwnd, bool is_dark)
{
    if (!hwnd) {
        return;
    }
    const int dark = is_dark ? 1 : 0;
    if (FAILED(DwmSetWindowAttribute(hwnd, kDwmwaUseImmersiveDarkMode, &dark, sizeof(dark)))) {
        DwmSetWindowAttribute(hwnd, kDwmwaUseImmersiveDarkModeLegacy, &dark, sizeof(dark));
    }
}

void apply_win11_backdrop(HWND hwnd, const QString& effect, int accent_state, bool is_dark)
{
    apply_dwm_dark_mode(hwnd, is_dark);

    if (is_win11_22h2()) {
        DwmSetWindowAttribute(hwnd, kDwmwaSystemBackdropType, &accent_state, sizeof(accent_state));
    } else {
        const int enable = (effect == QStringLiteral("mica")) ? 1 : 0;
        DwmSetWindowAttribute(hwnd, kDwmwaMicaEffect, &enable, sizeof(enable));
    }

    const int transparent_color = static_cast<int>(kDwmColorNone);
    DwmSetWindowAttribute(hwnd, kDwmwaCaptionColor, &transparent_color, sizeof(transparent_color));
    DwmSetWindowAttribute(hwnd, kDwmwaBorderColor, &transparent_color, sizeof(transparent_color));
}

void apply_win10_acrylic(HWND hwnd, bool is_dark, uint32_t win10_light, uint32_t win10_dark)
{
    HMODULE user32 = GetModuleHandleW(L"user32.dll");
    if (!user32) {
        return;
    }
    auto set_wca = reinterpret_cast<SetWindowCompositionAttributeFn>(
        GetProcAddress(user32, "SetWindowCompositionAttribute"));
    if (!set_wca) {
        return;
    }

    AccentPolicy accent{};
    accent.accent_state = 3; // ACCENT_ENABLE_ACRYLICBLURBEHIND
    accent.accent_flags = 2;
    accent.gradient_color = static_cast<int>(is_dark ? win10_dark : win10_light);

    WindowCompositionAttribData data{};
    data.attrib = kWcaAccentPolicy;
    data.pv_data = &accent;
    data.cb_data = sizeof(accent);
    set_wca(hwnd, &data);
}

void disable_win10_acrylic(HWND hwnd)
{
    if (HMODULE user32 = GetModuleHandleW(L"user32.dll")) {
        auto set_wca = reinterpret_cast<SetWindowCompositionAttributeFn>(
            GetProcAddress(user32, "SetWindowCompositionAttribute"));
        if (set_wca) {
            AccentPolicy accent{};
            accent.accent_state = 0; // ACCENT_DISABLED
            accent.accent_flags = 0;
            accent.gradient_color = 0;
            accent.animation_id = 0;

            WindowCompositionAttribData data{};
            data.attrib = kWcaAccentPolicy;
            data.pv_data = &accent;
            data.cb_data = sizeof(accent);
            set_wca(hwnd, &data);
        }
    }
    if (is_dwm_composition_enabled()) {
        const MARGINS zero_margins{0, 0, 0, 0};
        DwmExtendFrameIntoClientArea(hwnd, &zero_margins);
    }
}

void trigger_theme_manager_refresh()
{
    if (s_theme_manager.isNull()) {
        return;
    }
    if (auto* tm = qobject_cast<rinui::ThemeManager*>(s_theme_manager.data())) {
        tm->apply_backdrop_effect(tm->get_backdrop_effect());
    }
}

#endif // _WIN32

} // namespace

namespace rinui::win {

WinEventManager::WinEventManager(QObject* parent)
    : QObject(parent)
{
}

void WinEventManager::add_window(QQuickWindow* window)
{
    if (!window) {
        return;
    }
    for (const auto& existing : m_windows) {
        if (existing.data() == window) {
            return;
        }
    }
    m_windows.append(QPointer<QQuickWindow>(window));
}

void WinEventManager::remove_window(QQuickWindow* window)
{
    for (qsizetype i = m_windows.size() - 1; i >= 0; --i) {
        if (m_windows[i].isNull() || m_windows[i].data() == window) {
            m_windows.removeAt(i);
        }
    }
}

void WinEventManager::clear_windows()
{
    m_windows.clear();
    m_pending_frame_sync_windows.clear();
}

void WinEventManager::flush_pending_frame_sync_windows()
{
    const auto pending = m_pending_frame_sync_windows;
    m_pending_frame_sync_windows.clear();
    for (const auto& ptr : pending) {
        if (!ptr.isNull()) {
            syncWindowFrame(ptr.data());
        }
    }
}

qint64 WinEventManager::getWindowId(QObject* window) const
{
    auto* qw = qobject_cast<QWindow*>(window);
    if (!qw) {
        return 0;
    }
    return static_cast<qint64>(qw->winId());
}

void WinEventManager::drag_window_event(qint64 hwnd) const
{
#ifdef _WIN32
    if (!is_windows_qpa() || hwnd == 0) {
        return;
    }
    HWND native_hwnd = reinterpret_cast<HWND>(static_cast<qintptr>(hwnd));
    ReleaseCapture();
    SendMessageW(native_hwnd, WM_SYSCOMMAND, SC_MOVE | HTCAPTION, 0);
#else
    Q_UNUSED(hwnd);
#endif
}

void WinEventManager::syncWindowFrame(QObject* window)
{
#ifdef _WIN32
    if (!is_windows_qpa() || !window) {
        return;
    }
    auto* qw = qobject_cast<QQuickWindow*>(window);
    if (!qw) {
        return;
    }
    if (m_windows.isEmpty()) {
        m_pending_frame_sync_windows.append(QPointer<QObject>(window));
        return;
    }

    HWND hwnd = reinterpret_cast<HWND>(qw->winId());
    if (!hwnd) {
        m_pending_frame_sync_windows.append(QPointer<QObject>(window));
        return;
    }

    const LONG_PTR style = GetWindowLongPtrW(hwnd, GWL_STYLE);
    SetWindowLongPtrW(hwnd, GWL_STYLE, style | WS_CAPTION | WS_THICKFRAME);
    extend_frame_into_client_area(qw, hwnd);

    SendMessageW(hwnd, WM_ACTIVATEAPP, 1, 0);
    SendMessageW(hwnd, WM_NCACTIVATE, 1, 0);
    SendMessageW(hwnd, WM_ACTIVATE, 1, 0);
    refresh_window_frame(hwnd);
    qw->requestActivate();

    QTimer::singleShot(0, this, []() {
        trigger_theme_manager_refresh();
        if (s_event_filter) {
            for (HWND h : s_event_filter->hwnds()) {
                apply_window_rounded_and_shadow(h);
            }
        }
    });
#else
    Q_UNUSED(window);
#endif
}

void WinEventManager::maximizeWindow(QObject* window) const
{
#ifdef _WIN32
    if (!is_windows_qpa() || !window) {
        return;
    }
    auto* qw = qobject_cast<QQuickWindow*>(window);
    if (!qw) {
        return;
    }
    HWND hwnd = reinterpret_cast<HWND>(qw->winId());
    toggle_maximize(qw, hwnd);
#else
    Q_UNUSED(window);
#endif
}

} // namespace rinui::win

namespace rinui::platform {

void register_engine_objects(QQmlApplicationEngine& engine)
{
#ifdef _WIN32
    if (!s_event_manager) {
        s_event_manager = std::make_unique<rinui::win::WinEventManager>(nullptr);
    }
    if (auto* ctx = engine.rootContext()) {
        ctx->setContextProperty(QStringLiteral("WinEventManager"), s_event_manager.get());
    }
#else
    Q_UNUSED(engine);
#endif
}

void set_theme_manager(QObject* theme_manager)
{
    s_theme_manager = QPointer<QObject>(theme_manager);
}

void setup_window(QQuickWindow* window, bool, int, int)
{
#ifdef _WIN32
    if (!window || !is_windows_qpa()) {
        return;
    }
    ensure_filter_installed();
    if (s_event_manager) {
        s_event_manager->add_window(window);
    }
    if (s_event_filter) {
        s_event_filter->add_window(window);
    }
    if (s_event_manager) {
        s_event_manager->syncWindowFrame(window);
        s_event_manager->flush_pending_frame_sync_windows();
    }
    if (HWND hwnd = reinterpret_cast<HWND>(window->winId())) {
        apply_window_rounded_and_shadow(hwnd);
    }
    install_transparent_render_clear(window);
    trigger_theme_manager_refresh();
#else
    Q_UNUSED(window);
#endif
}

void teardown_window(QQuickWindow* window)
{
#ifdef _WIN32
    if (s_event_manager) {
        s_event_manager->remove_window(window);
    }
    if (s_event_filter) {
        s_event_filter->remove_window(window);
        if (s_event_filter->empty()) {
            remove_filter_if_installed();
        }
    }
#else
    Q_UNUSED(window);
#endif
}

void teardown_windows_and_filters()
{
#ifdef _WIN32
    if (s_event_manager) {
        s_event_manager->clear_windows();
    }
    remove_filter_if_installed();
    s_last_backdrop_effect = QStringLiteral("none");
#endif
}

void teardown_runtime_services()
{
#ifdef _WIN32
    s_event_manager.reset();
    s_last_backdrop_effect = QStringLiteral("none");
#endif
}

void teardown_all()
{
    teardown_windows_and_filters();
    teardown_runtime_services();
}

void apply_theme(const QString& actual_theme, const QString&)
{
#ifdef _WIN32
    if (!is_windows_qpa() || !s_event_filter) {
        return;
    }
    const bool is_dark = actual_theme == QStringLiteral("Dark");
    for (HWND hwnd : s_event_filter->hwnds()) {
        if (is_win10()) {
            apply_dwm_dark_mode(hwnd, is_dark);
        }
        if (!is_win11() && is_win10() && s_last_backdrop_effect == QStringLiteral("acrylic")) {
            apply_win10_acrylic(hwnd, is_dark, s_last_win10_light, s_last_win10_dark);
        }
    }
#else
    Q_UNUSED(actual_theme);
#endif
}

int32_t apply_backdrop(
    const QString& effect_type,
    bool is_dark,
    uint32_t win10_light,
    uint32_t win10_dark)
{
#ifdef _WIN32
    if (!is_windows_qpa() || !s_event_filter) {
        return -2;
    }
    const auto hwnds = s_event_filter->hwnds();
    if (hwnds.isEmpty()) {
        return -2;
    }

    s_last_backdrop_effect = effect_type;
    s_last_win10_light = win10_light;
    s_last_win10_dark = win10_dark;

    int accent_state = 0;
    if (effect_type == QStringLiteral("acrylic")) {
        accent_state = 3;
    } else if (effect_type == QStringLiteral("mica")) {
        accent_state = 2;
    } else if (effect_type == QStringLiteral("tabbed")) {
        accent_state = 4;
    }

    for (HWND hwnd : hwnds) {
        if (is_win11()) {
            apply_win11_backdrop(hwnd, effect_type, accent_state, is_dark);
        } else if (is_win10()) {
            if (effect_type == QStringLiteral("acrylic")) {
                apply_win10_acrylic(hwnd, is_dark, win10_light, win10_dark);
            } else if (effect_type == QStringLiteral("none")) {
                disable_win10_acrylic(hwnd);
            }
        }
        apply_window_rounded_and_shadow(hwnd);
    }
    return 0;
#else
    Q_UNUSED(effect_type);
    Q_UNUSED(is_dark);
    Q_UNUSED(win10_light);
    Q_UNUSED(win10_dark);
    return -2;
#endif
}

} // namespace rinui::platform
