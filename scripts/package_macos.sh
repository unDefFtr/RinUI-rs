#!/usr/bin/env bash
set -euo pipefail

BINARY_PATH="${1:-target/release/rinui-gallery}"
OUTPUT_DIR="${2:-dist}"
APP_NAME="RinUIGallery"
BUNDLE_DIR="${OUTPUT_DIR}/${APP_NAME}.app"
CONTENTS_DIR="${BUNDLE_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
FRAMEWORKS_DIR="${CONTENTS_DIR}/Frameworks"
PLUGINS_DIR="${CONTENTS_DIR}/PlugIns"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

if [[ ! -x "${BINARY_PATH}" ]]; then
    echo "error: executable not found at ${BINARY_PATH}" >&2
    exit 1
fi

QMAKE_BIN="${QMAKE:-$(command -v qmake6 || command -v qmake || true)}"
if [[ -z "${QMAKE_BIN}" ]]; then
    echo "error: qmake6/qmake not found in PATH and QMAKE is unset" >&2
    exit 1
fi

QT_BINS="$("${QMAKE_BIN}" -query QT_INSTALL_BINS)"
QT_LIBS="$("${QMAKE_BIN}" -query QT_INSTALL_LIBS)"
QT_QML="$("${QMAKE_BIN}" -query QT_INSTALL_QML)"
MACDEPLOYQT="${QT_BINS}/macdeployqt"
if [[ ! -x "${MACDEPLOYQT}" ]]; then
    MACDEPLOYQT="$(command -v macdeployqt || true)"
fi
if [[ -z "${MACDEPLOYQT}" || ! -x "${MACDEPLOYQT}" ]]; then
    echo "error: macdeployqt not found (checked ${QT_BINS}/macdeployqt and PATH)" >&2
    exit 1
fi

rm -rf "${BUNDLE_DIR}"
mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

cp "${BINARY_PATH}" "${MACOS_DIR}/${APP_NAME}"
chmod +x "${MACOS_DIR}/${APP_NAME}"

if [[ -f "upstream/Rin-UI/examples/assets/icons/app/gallery.png" ]]; then
    cp "upstream/Rin-UI/examples/assets/icons/app/gallery.png" "${RESOURCES_DIR}/gallery.png"
fi
cp "LICENSE" "${RESOURCES_DIR}/LICENSE"
cp "upstream/Rin-UI/LICENSE" "${RESOURCES_DIR}/LICENSE.RinUI"
cp "THIRD_PARTY_NOTICES.md" "${RESOURCES_DIR}/THIRD_PARTY_NOTICES.md"

cat > "${CONTENTS_DIR}/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>RinUIGallery</string>
    <key>CFBundleIdentifier</key>
    <string>io.rinui.gallery.rs</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>RinUI Gallery</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0-alpha.1</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

if command -v plutil >/dev/null 2>&1; then
    plutil -lint "${CONTENTS_DIR}/Info.plist"
fi

"${MACDEPLOYQT}" "${BUNDLE_DIR}" \
    -qmldir="upstream/Rin-UI/RinUI" \
    -qmldir="upstream/Rin-UI/examples" \
    -qmldir="examples/gallery/qml" \
    -libpath="${QT_LIBS}" \
    -always-overwrite

# Qt 6 macdeployqt does not always write qt.conf; ensure bundle-relative lookup paths exist.
if [[ ! -f "${RESOURCES_DIR}/qt.conf" ]]; then
    cat > "${RESOURCES_DIR}/qt.conf" <<'QTCONF'
[Paths]
Plugins = PlugIns
Imports = Resources/qml
Qml2Imports = Resources/qml
QmlImports = Resources/qml
QTCONF
fi

# Ensure Qt5Compat/GraphicalEffects is present in case macdeployqt omitted it in any Qt layout.
if ! find "${CONTENTS_DIR}" -path "*Qt5Compat/GraphicalEffects*" | grep -q .; then
    if [[ -d "${QT_QML}/Qt5Compat/GraphicalEffects" ]]; then
        mkdir -p "${RESOURCES_DIR}/qml/Qt5Compat"
        cp -R "${QT_QML}/Qt5Compat/GraphicalEffects" "${RESOURCES_DIR}/qml/Qt5Compat/"
    fi
fi

list_macho_binaries() {
    while IFS= read -r -d '' candidate; do
        if file -b "${candidate}" | grep -q "Mach-O"; then
            printf '%s\0' "${candidate}"
        fi
    done < <(find "${CONTENTS_DIR}" -type f ! -name "*.qml*" ! -name "*.js" ! -name "*.png" ! -name "*.ttf" ! -name "*.otf" ! -name "*.qm" ! -name "*.plist" ! -name "*.conf" ! -name "qmldir" ! -name "*.md" ! -name "LICENSE*" -print0)
}

list_load_dylibs() {
    otool -l "$1" | awk '
        /cmd LC_LOAD_DYLIB|cmd LC_LOAD_WEAK_DYLIB|cmd LC_REEXPORT_DYLIB/ { want=1; next }
        want && $1 == "name" { print $2; want=0 }
    '
}

list_rpaths() {
    otool -l "$1" | awk '
        /cmd LC_RPATH/ { want=1; next }
        want && $1 == "path" { print $2; want=0 }
    '
}

is_required_plugin() {
    local name
    name="$(basename "$1")"
    case "${name}" in
        libqcocoa.dylib|libqtgraphicaleffects*.dylib|libqtquick2plugin.dylib|libqtquickcontrols2*.dylib|libqtquicktemplates2plugin.dylib|libqtquicknativestyleplugin.dylib|libqtquickdialogs*.dylib|libeffectsplugin*.dylib|libqmlshapesplugin*.dylib|libqquicklayoutsplugin*.dylib|libquickwindowplugin*.dylib|libqsvg*.dylib|libqjpeg*.dylib|libqico*.dylib|libqicns*.dylib|libqmacstyle*.dylib)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

ensure_rpath() {
    local target="$1"
    local rpath="$2"
    if ! list_rpaths "${target}" | grep -qxF "${rpath}"; then
        chmod u+w "${target}" 2>/dev/null || true
        install_name_tool -add_rpath "${rpath}" "${target}" 2>/dev/null || true
    fi
}

strip_external_rpaths() {
    local target="$1"
    local rpath
    while IFS= read -r rpath; do
        [[ -z "${rpath}" ]] && continue
        if [[ "${rpath}" == /opt/homebrew/* || "${rpath}" == /usr/local/* || "${rpath}" == /Users/* || "${rpath}" == */Cellar/* ]]; then
            chmod u+w "${target}" 2>/dev/null || true
            install_name_tool -delete_rpath "${rpath}" "${target}" 2>/dev/null || true
        fi
    done < <(list_rpaths "${target}")
}

fix_external_dylib_id() {
    local target="$1"
    local cur_id
    cur_id="$(otool -D "${target}" | sed -n '2p' || true)"
    if [[ -n "${cur_id}" && ( "${cur_id}" == /opt/homebrew/* || "${cur_id}" == /usr/local/* || "${cur_id}" == /Users/* || "${cur_id}" == */Cellar/* ) ]]; then
        local new_id
        if [[ "${target}" == *"Contents/Frameworks/"* ]]; then
            new_id="@rpath/${target#*Contents/Frameworks/}"
        else
            new_id="@rpath/$(basename "${target}")"
        fi
        chmod u+w "${target}" 2>/dev/null || true
        install_name_tool -id "${new_id}" "${target}"
    fi
}

relocate_external_deps() {
    local target="$1"
    local dep
    while IFS= read -r dep; do
        [[ -z "${dep}" ]] && continue
        if [[ "${dep}" == /opt/homebrew/* || "${dep}" == /usr/local/* ]]; then
            local base
            base="$(basename "${dep}")"
            local dest="${FRAMEWORKS_DIR}/${base}"
            if [[ ! -f "${dest}" && -f "${dep}" ]]; then
                mkdir -p "${FRAMEWORKS_DIR}"
                cp -L "${dep}" "${dest}"
                chmod 755 "${dest}"
                install_name_tool -id "@rpath/${base}" "${dest}"
            fi
            chmod u+w "${target}" 2>/dev/null || true
            install_name_tool -change "${dep}" "@rpath/${base}" "${target}"
        fi
    done < <(list_load_dylibs "${target}")
}

# Remove optional/unrelated Qt plugins whose @rpath frameworks were not deployed by macdeployqt
# or whose host-only dylibs are unavailable (e.g. QtPdf, Qt3D, QtVirtualKeyboard, sqldrivers) so
# dyld cannot fall back to external system Qt installations.
if [[ -d "${PLUGINS_DIR}" ]]; then
    while IFS= read -r -d '' plugin_bin; do
        missing_ref=""
        while IFS= read -r ref; do
            [[ -z "${ref}" ]] && continue
            if [[ "${ref}" == @rpath/* ]]; then
                rel="${ref#@rpath/}"
                if [[ ! -e "${FRAMEWORKS_DIR}/${rel}" ]]; then
                    missing_ref="${ref}"
                    break
                fi
            elif [[ "${ref}" == /opt/homebrew/* || "${ref}" == /usr/local/* || "${ref}" == /Users/* ]]; then
                # Optional Qt plugins (e.g. sqldrivers) may reference host-only
                # libraries; prune them if the external dylib cannot be bundled.
                if [[ ! -e "${ref}" ]]; then
                    missing_ref="${ref}"
                    break
                fi
            fi
        done < <(list_load_dylibs "${plugin_bin}" | grep -E '^(@rpath/|/opt/homebrew/|/usr/local/|/Users/)' || true)

        if [[ -n "${missing_ref}" ]]; then
            if is_required_plugin "${plugin_bin}"; then
                echo "error: required plugin ${plugin_bin} has missing framework ${missing_ref}" >&2
                exit 1
            fi
            echo "Pruning unbundled optional plugin: ${plugin_bin} (missing ${missing_ref})"
            rm -f "${plugin_bin}"
        fi
    done < <(find "${PLUGINS_DIR}" -type f -name "*.dylib" -print0)
fi

# Clean up any symlinks in Resources/qml that pointed to pruned optional plugins.
if [[ -d "${RESOURCES_DIR}/qml" ]]; then
    while IFS= read -r -d '' link_path; do
        if [[ ! -e "${link_path}" ]]; then
            rm -f "${link_path}"
        fi
    done < <(find "${RESOURCES_DIR}/qml" -type l -print0)
fi

ensure_rpath "${MACOS_DIR}/${APP_NAME}" "@executable_path/../Frameworks"

# Relocate transitive Homebrew/local dylibs, normalize dylib IDs, and strip external LC_RPATHs.
for _pass in 1 2 3; do
    found_external=0
    while IFS= read -r -d '' mach_bin; do
        strip_external_rpaths "${mach_bin}"
        fix_external_dylib_id "${mach_bin}"
        if list_load_dylibs "${mach_bin}" | grep -qE '^(/opt/homebrew/|/usr/local/|/Users/)'; then
            found_external=1
            ensure_rpath "${mach_bin}" "@loader_path"
            ensure_rpath "${mach_bin}" "@executable_path/../Frameworks"
            relocate_external_deps "${mach_bin}"
        fi
    done < <(list_macho_binaries)
    if [[ "${found_external}" -eq 0 ]]; then
        break
    fi
done

# Restore ad-hoc Mach-O signature after install_name_tool edits (local execution only;
# NOT Developer ID signed or notarized).
if command -v codesign >/dev/null 2>&1; then
    codesign --force --deep --sign - "${BUNDLE_DIR}"
fi

# Verify self-contained bundle structure, rpaths, and load commands.
for required in \
    "${CONTENTS_DIR}/Info.plist" \
    "${MACOS_DIR}/${APP_NAME}" \
    "${FRAMEWORKS_DIR}/QtCore.framework" \
    "${PLUGINS_DIR}/platforms/libqcocoa.dylib" \
    "${RESOURCES_DIR}/qt.conf" \
    "${RESOURCES_DIR}/LICENSE" \
    "${RESOURCES_DIR}/LICENSE.RinUI" \
    "${RESOURCES_DIR}/THIRD_PARTY_NOTICES.md"
do
    if [[ ! -e "${required}" ]]; then
        echo "error: expected bundle artifact missing: ${required}" >&2
        exit 1
    fi
done

if ! find "${CONTENTS_DIR}" -path "*Qt5Compat/GraphicalEffects*" | grep -q .; then
    echo "error: Qt5Compat/GraphicalEffects missing from bundle" >&2
    exit 1
fi

while IFS= read -r -d '' mach_bin; do
    if otool -L "${mach_bin}" | tail -n +2 | awk '{print $1}' | grep -qE '^(/opt/homebrew/|/usr/local/|/Users/)'; then
        echo "error: external non-system dependency or ID remains in ${mach_bin}:" >&2
        otool -L "${mach_bin}" >&2
        exit 1
    fi
    if list_rpaths "${mach_bin}" | grep -qE '^(/opt/homebrew/|/usr/local/|/Users/|.*/Cellar/)'; then
        echo "error: external LC_RPATH remains in ${mach_bin}:" >&2
        list_rpaths "${mach_bin}" >&2
        exit 1
    fi
    while IFS= read -r ref; do
        [[ -z "${ref}" ]] && continue
        rel="${ref#@rpath/}"
        if [[ ! -e "${FRAMEWORKS_DIR}/${rel}" ]]; then
            echo "error: unresolved @rpath reference in ${mach_bin}: ${ref}" >&2
            exit 1
        fi
    done < <(list_load_dylibs "${mach_bin}" | grep '^@rpath/' || true)
done < <(list_macho_binaries)

echo "Packaged macOS bundle at ${BUNDLE_DIR} (ad-hoc local signature; not notarized)"
