#!/usr/bin/env bash
set -euo pipefail

BINARY_PATH="${1:-target/release/rinui-gallery}"
OUTPUT_DIR="${2:-dist/rinui-gallery-linux}"

if [[ ! -x "${BINARY_PATH}" ]]; then
    echo "error: executable not found at ${BINARY_PATH}" >&2
    exit 1
fi

QMAKE_BIN="${QMAKE:-$(command -v qmake6 || command -v qmake || true)}"
if [[ -z "${QMAKE_BIN}" ]]; then
    echo "error: qmake6/qmake not found in PATH" >&2
    exit 1
fi

QT_LIBS="$("${QMAKE_BIN}" -query QT_INSTALL_LIBS)"
QT_PLUGINS="$("${QMAKE_BIN}" -query QT_INSTALL_PLUGINS)"
QT_QML="$("${QMAKE_BIN}" -query QT_INSTALL_QML)"

rm -rf "${OUTPUT_DIR}"
mkdir -p "${OUTPUT_DIR}/bin" "${OUTPUT_DIR}/lib" "${OUTPUT_DIR}/plugins" "${OUTPUT_DIR}/qml"

cp "${BINARY_PATH}" "${OUTPUT_DIR}/bin/rinui-gallery"
chmod +x "${OUTPUT_DIR}/bin/rinui-gallery"
cp "LICENSE" "${OUTPUT_DIR}/LICENSE"
cp "upstream/Rin-UI/LICENSE" "${OUTPUT_DIR}/LICENSE.RinUI"
cp "crates/rinui-host/vendor/LICENSE.FluentUI" "${OUTPUT_DIR}/LICENSE.FluentUI"
if [[ -n "${RINUI_QT_LICENSES:-}" ]]; then
    cp -R "${RINUI_QT_LICENSES}" "${OUTPUT_DIR}/qt-licenses"
fi
cp "THIRD_PARTY_NOTICES.md" "${OUTPUT_DIR}/THIRD_PARTY_NOTICES.md"

# Copy Qt platform, input context, OpenGL, & imageformat plugins needed by RinUI
for plugin_dir in \
    platforms \
    xcbglintegrations \
    platforminputcontexts \
    wayland-shell-integration \
    wayland-decoration-client \
    wayland-graphics-integration-client \
    imageformats \
    iconengines
do
    if [[ -d "${QT_PLUGINS}/${plugin_dir}" ]]; then
        cp -a "${QT_PLUGINS}/${plugin_dir}" "${OUTPUT_DIR}/plugins/"
    fi
done

# Copy required QML runtime modules (including Qt5Compat.GraphicalEffects, Quick, Controls, Effects, Shapes)
for qml_mod in QtQml QtQuick Qt5Compat QML; do
    if [[ -d "${QT_QML}/${qml_mod}" ]]; then
        cp -a "${QT_QML}/${qml_mod}" "${OUTPUT_DIR}/qml/"
    fi
done

# Copy Qt6 and ICU shared libraries while preserving SONAME symlinks
if [[ -d "${QT_LIBS}" ]]; then
    cp -a "${QT_LIBS}"/libQt6*.so* "${OUTPUT_DIR}/lib/" 2>/dev/null || true
    cp -a "${QT_LIBS}"/libicu*.so* "${OUTPUT_DIR}/lib/" 2>/dev/null || true
fi

if command -v ldd >/dev/null 2>&1; then
    while IFS= read -r lib; do
        [[ -z "${lib}" ]] && continue
        if [[ -e "${lib}" && ! -e "${OUTPUT_DIR}/lib/$(basename "${lib}")" ]]; then
            cp -L "${lib}" "${OUTPUT_DIR}/lib/" || true
        fi
    done < <(ldd "${BINARY_PATH}" | awk '/=> \// {print $3}' | grep -E 'libQt6|libicu' || true)
fi

for required in \
    "${OUTPUT_DIR}/bin/rinui-gallery" \
    "${OUTPUT_DIR}/plugins/platforms" \
    "${OUTPUT_DIR}/qml/Qt5Compat/GraphicalEffects" \
    "${OUTPUT_DIR}/LICENSE" \
    "${OUTPUT_DIR}/LICENSE.RinUI" \
    "${OUTPUT_DIR}/THIRD_PARTY_NOTICES.md"
do
    if [[ ! -e "${required}" ]]; then
        echo "error: expected Linux bundle artifact missing: ${required}" >&2
        exit 1
    fi
done

cat > "${OUTPUT_DIR}/run-gallery.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export LD_LIBRARY_PATH="${ROOT}/lib${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
export QT_PLUGIN_PATH="${ROOT}/plugins"
export QML_IMPORT_PATH="${ROOT}/qml"
exec "${ROOT}/bin/rinui-gallery" "$@"
EOF
chmod +x "${OUTPUT_DIR}/run-gallery.sh"

echo "Packaged Linux staging directory at ${OUTPUT_DIR}"
