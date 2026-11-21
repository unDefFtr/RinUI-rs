# Third-Party Notices

`rinui-rs` integrates and depends on the following third-party software components:

---

## 1. RinUI (Upstream QML Library)

- **Repository**: https://github.com/RinLit-233-shiroko/Rin-UI
- **Submodule Path**: `upstream/Rin-UI`
- **Pinned Commit SHA**: `46d77fa8371ff0e9f46a10a2eb66612dab123269` (tag/ref `beta-0.5.0-16-g46d77fa`, version `0.5.0`)
- **License**: MIT License

```text
MIT License

Copyright (c) 2025 RinLit

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## 2. Fluent UI System Icons

Upstream RinUI bundles `FluentSystemIcons-Resizable.ttf` and `FluentSystemIcons-Index.js` inside `upstream/Rin-UI/RinUI/assets/fonts/`. Refer to Microsoft's Fluent UI System Icons license terms (MIT License) distributed with the upstream icon set.

---

## 3. Qt 6 Framework

`rinui-rs` links dynamically against Qt 6 (`QtCore`, `QtGui`, `QtQml`, `QtQuick`, `QtQuickControls2`) and requires Qt QML runtime plugins (`QtQuick`, `QtQuick.Controls`, `QtQuick.Controls.Basic`, `QtQuick.Templates`, `QtQuick.Layouts`, `QtQuick.Window`, `QtQuick.Effects`, `QtQuick.Shapes`, `QtQml`, `Qt5Compat.GraphicalEffects`) as well as Qt platform plugins (`qcocoa`, `qwindows`, `qxcb`, `qwayland`, etc.).

- **Qt License**: LGPLv3 / GPLv3 / Commercial (depending on your Qt distribution and licensing election).
- Embedding QML files and assets into Qt's resource system (`rcc`) does **not** remove Qt framework redistribution or LGPLv3/GPLv3/Commercial obligations when shipping binaries that link against Qt shared or static libraries.

---

## 4. Rust & C++ Bindings

- `cxx` (MIT OR Apache-2.0)
- `cxx-qt`, `cxx-qt-lib`, `cxx-qt-build`, `qt-build-utils` (MIT OR Apache-2.0, Copyright Klarälvdalens Datakonsult AB / KDAB)
- `serde`, `serde_json`, `thiserror`, `tracing`, `tracing-subscriber` (MIT OR Apache-2.0)
