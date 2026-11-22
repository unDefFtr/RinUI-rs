#pragma once

#include <QAbstractNativeEventFilter>
#include <QHash>
#include <QList>
#include <QObject>
#include <QPointer>
#include <QUrl>
#include <QVariant>

class QQuickWindow;

namespace rinui::win {

class StagedFonts : public QObject {
    Q_OBJECT

public:
    explicit StagedFonts(QObject* parent) : QObject(parent) {}
    ~StagedFonts() override;

    Q_INVOKABLE QVariant loadLocalFont(const QUrl& url);

private:
    QHash<QString, int> m_fonts;
};

class WinEventManager : public QObject {
    Q_OBJECT

public:
    explicit WinEventManager(QObject* parent = nullptr);
    ~WinEventManager() override = default;

    void add_window(QQuickWindow* window);
    void remove_window(QQuickWindow* window);
    void clear_windows();
    void flush_pending_frame_sync_windows();

    Q_INVOKABLE qint64 getWindowId(QObject* window) const;
    Q_INVOKABLE void drag_window_event(qint64 hwnd) const;
    Q_INVOKABLE void syncWindowFrame(QObject* window);
    Q_INVOKABLE void maximizeWindow(QObject* window) const;

private:
    QList<QPointer<QQuickWindow>> m_windows;
    QList<QPointer<QObject>> m_pending_frame_sync_windows;
};

} // namespace rinui::win
