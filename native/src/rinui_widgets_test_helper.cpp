#include <QApplication>
#include <QByteArray>
#include <QCoreApplication>
#include <QGuiApplication>
#include <QLabel>
#include <QObject>
#include <QPointer>
#include <QString>
#include <QWidget>

#include <cstddef>
#include <cstring>

namespace {

struct RinuiWidgetsTestState {
    int argc = 1;
    char arg0[32] = "rinui-widgets-test";
    char* argv[2] = {nullptr, nullptr};
    QApplication* app = nullptr;
    QPointer<QWidget> window;
    QPointer<QLabel> label;
};

} // namespace

extern "C" {

void* rinui_widgets_test_app_create(const char* label_text)
{
    auto* state = new RinuiWidgetsTestState();
    state->argv[0] = state->arg0;
    state->argv[1] = nullptr;
    state->app = new QApplication(state->argc, state->argv);

    auto* win = new QWidget();
    win->setObjectName(QStringLiteral("hostWidgetsMainWindow"));
    auto* label = new QLabel(QString::fromUtf8(label_text ? label_text : ""), win);
    label->setObjectName(QStringLiteral("hostWidgetsLabel"));

    state->window = QPointer<QWidget>(win);
    state->label = QPointer<QLabel>(label);
    return state;
}

void* rinui_widgets_test_app_qobject(void* handle)
{
    auto* state = static_cast<RinuiWidgetsTestState*>(handle);
    if (!state || !state->app) {
        return nullptr;
    }
    return static_cast<QObject*>(state->app);
}

bool rinui_widgets_test_widget_alive(void* handle, char* out_buf, std::size_t out_cap)
{
    auto* state = static_cast<RinuiWidgetsTestState*>(handle);
    if (!state || !state->app || QCoreApplication::instance() != state->app
        || state->window.isNull() || state->label.isNull()) {
        return false;
    }
    if (out_buf && out_cap > 0) {
        const QByteArray utf8 = state->label->text().toUtf8();
        const std::size_t copy_len =
            static_cast<std::size_t>(utf8.size()) < (out_cap - 1)
            ? static_cast<std::size_t>(utf8.size())
            : (out_cap - 1);
        std::memcpy(out_buf, utf8.constData(), copy_len);
        out_buf[copy_len] = '\0';
    }
    return true;
}

void rinui_widgets_test_app_process_events(void* handle)
{
    auto* state = static_cast<RinuiWidgetsTestState*>(handle);
    if (state && state->app && QCoreApplication::instance() == state->app) {
        QCoreApplication::processEvents();
    }
}

void rinui_widgets_test_app_destroy(void* handle)
{
    auto* state = static_cast<RinuiWidgetsTestState*>(handle);
    if (!state) {
        return;
    }
    delete state->window.data();
    delete state->app;
    delete state;
}

} // extern "C"
