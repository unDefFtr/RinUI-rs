#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: scripts/package_macos.sh [OPTIONS] [BINARY_PATH] [OUTPUT_DIR]

Packages rinui-gallery into a self-contained RinUIGallery.app bundle.
By default, applies ad-hoc local code signing (`codesign --sign -`).

Options:
  --binary <PATH>           Path to release binary (default: target/release/rinui-gallery)
  --output <DIR>            Output directory for .app (default: dist)
  --sign-identity <ID>      Developer ID Application identity (env: RINUI_MACOS_SIGN_IDENTITY)
  --entitlements <PLIST>    Entitlements plist when signing (env: RINUI_MACOS_ENTITLEMENTS)
  --notarize                Submit .app via `xcrun notarytool` & staple (env: RINUI_MACOS_NOTARIZE=1)
  --notary-profile <NAME>   Keychain profile for `xcrun notarytool` (env: RINUI_MACOS_NOTARY_PROFILE)
  -h, --help                Show this help message
EOF
}

BINARY_PATH=""
OUTPUT_DIR=""
SIGN_IDENTITY="${RINUI_MACOS_SIGN_IDENTITY:-}"
ENTITLEMENTS="${RINUI_MACOS_ENTITLEMENTS:-}"
NOTARY_PROFILE="${RINUI_MACOS_NOTARY_PROFILE:-}"
NOTARY_ARGS=()
if [[ -n "${RINUI_MACOS_NOTARY_KEYCHAIN:-}" ]]; then
    NOTARY_ARGS+=(--keychain "${RINUI_MACOS_NOTARY_KEYCHAIN}")
fi
NOTARIZE=0
if [[ "${RINUI_MACOS_NOTARIZE:-0}" == "1" || "${RINUI_MACOS_NOTARIZE:-}" == "true" ]]; then
    NOTARIZE=1
fi

POSITIONAL=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --binary)
            [[ $# -ge 2 ]] || { echo "error: --binary requires a path" >&2; exit 1; }
            BINARY_PATH="$2"
            shift 2
            ;;
        --output)
            [[ $# -ge 2 ]] || { echo "error: --output requires a directory" >&2; exit 1; }
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --sign-identity)
            [[ $# -ge 2 ]] || { echo "error: --sign-identity requires an identity string" >&2; exit 1; }
            SIGN_IDENTITY="$2"
            shift 2
            ;;
        --entitlements)
            [[ $# -ge 2 ]] || { echo "error: --entitlements requires a plist path" >&2; exit 1; }
            ENTITLEMENTS="$2"
            shift 2
            ;;
        --notarize)
            NOTARIZE=1
            shift
            ;;
        --notary-profile)
            [[ $# -ge 2 ]] || { echo "error: --notary-profile requires a profile name" >&2; exit 1; }
            NOTARY_PROFILE="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            echo "error: unknown option '$1' (try --help)" >&2
            exit 1
            ;;
        *)
            POSITIONAL+=("$1")
            shift
            ;;
    esac
done

if [[ -z "${BINARY_PATH}" && ${#POSITIONAL[@]} -ge 1 ]]; then
    BINARY_PATH="${POSITIONAL[0]}"
fi
if [[ -z "${OUTPUT_DIR}" && ${#POSITIONAL[@]} -ge 2 ]]; then
    OUTPUT_DIR="${POSITIONAL[1]}"
fi
if [[ ${#POSITIONAL[@]} -gt 2 ]]; then
    echo "error: unexpected positional arguments (try --help)" >&2
    exit 1
fi

BINARY_PATH="${BINARY_PATH:-target/release/rinui-gallery}"
OUTPUT_DIR="${OUTPUT_DIR:-dist}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACE_VERSION="$(awk '
    /^\[workspace\.package\]$/ { in_section = 1; next }
    /^\[/ { in_section = 0 }
    in_section && /^version[[:space:]]*=/ {
        value = $0
        sub(/^[^=]*=[[:space:]]*"/, "", value)
        sub(/".*$/, "", value)
        print value
        exit
    }
' "${REPO_ROOT}/Cargo.toml")"
if [[ -z "${WORKSPACE_VERSION}" ]]; then
    echo "error: could not read [workspace.package] version from ${REPO_ROOT}/Cargo.toml" >&2
    exit 1
fi
BUNDLE_VERSION="${WORKSPACE_VERSION%%[-+]*}"
if [[ ! "${BUNDLE_VERSION}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "error: could not derive a numeric macOS bundle version from ${WORKSPACE_VERSION}" >&2
    exit 1
fi

if [[ -n "${ENTITLEMENTS}" && ! -f "${ENTITLEMENTS}" ]]; then
    echo "error: entitlements file not found at ${ENTITLEMENTS}" >&2
    exit 1
fi
if [[ "${NOTARIZE}" -eq 1 && -z "${SIGN_IDENTITY}" ]]; then
    echo "error: --notarize requires --sign-identity or RINUI_MACOS_SIGN_IDENTITY" >&2
    exit 1
fi
if [[ "${NOTARIZE}" -eq 1 && -z "${NOTARY_PROFILE}" ]]; then
    echo "error: --notarize requires --notary-profile or RINUI_MACOS_NOTARY_PROFILE" >&2
    exit 1
fi

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
cp "crates/rinui-host/vendor/LICENSE.FluentUI" "${RESOURCES_DIR}/LICENSE.FluentUI"
if [[ -n "${RINUI_QT_LICENSES:-}" ]]; then
    cp -R "${RINUI_QT_LICENSES}" "${RESOURCES_DIR}/qt-licenses"
fi
cp "THIRD_PARTY_NOTICES.md" "${RESOURCES_DIR}/THIRD_PARTY_NOTICES.md"

cat > "${CONTENTS_DIR}/Info.plist" <<PLIST
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
    <string>${BUNDLE_VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${BUNDLE_VERSION}</string>
    <key>RinUIWorkspaceVersion</key>
    <string>${WORKSPACE_VERSION}</string>
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

# Code signing: ad-hoc by default; Developer ID + Hardened Runtime when configured.
if [[ -n "${SIGN_IDENTITY}" ]]; then
    if ! command -v codesign >/dev/null 2>&1; then
        echo "error: codesign tool not found" >&2
        exit 1
    fi
    MAIN_EXE="${MACOS_DIR}/${APP_NAME}"
    while IFS= read -r -d '' mach_bin; do
        if [[ "${mach_bin}" == "${MAIN_EXE}" ]]; then
            continue
        fi
        codesign --force --sign "${SIGN_IDENTITY}" --options runtime --timestamp "${mach_bin}"
    done < <(list_macho_binaries)

    if [[ -d "${FRAMEWORKS_DIR}" ]]; then
        while IFS= read -r -d '' fw_dir; do
            codesign --force --sign "${SIGN_IDENTITY}" --options runtime --timestamp "${fw_dir}"
        done < <(find "${FRAMEWORKS_DIR}" -maxdepth 1 -type d -name "*.framework" -print0)
    fi

    APP_SIGN_ARGS=(--force --sign "${SIGN_IDENTITY}" --options runtime --timestamp)
    if [[ -n "${ENTITLEMENTS}" ]]; then
        APP_SIGN_ARGS+=(--entitlements "${ENTITLEMENTS}")
    fi
    codesign "${APP_SIGN_ARGS[@]}" "${BUNDLE_DIR}"
    codesign --verify --deep --strict --verbose=2 "${BUNDLE_DIR}"
else
    # Restore ad-hoc Mach-O signature after install_name_tool edits (local execution only;
    # NOT Developer ID signed or notarized).
    if command -v codesign >/dev/null 2>&1; then
        codesign --force --deep --sign - "${BUNDLE_DIR}"
    fi
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

if [[ -n "${SIGN_IDENTITY}" && "${NOTARIZE}" -eq 1 ]]; then
    NOTARY_DIR="$(mktemp -d "${TMPDIR:-/tmp}/RinUIGallery-notarize.XXXXXX")"
    NOTARY_ZIP="${NOTARY_DIR}/RinUIGallery.zip"
    trap 'rm -rf "${NOTARY_DIR}"' EXIT
    ditto -c -k --keepParent "${BUNDLE_DIR}" "${NOTARY_ZIP}"
    xcrun notarytool submit "${NOTARY_ZIP}" --keychain-profile "${NOTARY_PROFILE}" ${NOTARY_ARGS[@]+"${NOTARY_ARGS[@]}"} --wait
    rm -rf "${NOTARY_DIR}"
    trap - EXIT
    xcrun stapler staple "${BUNDLE_DIR}"
    xcrun stapler validate "${BUNDLE_DIR}"
    echo "Packaged, Developer ID signed, and notarized macOS bundle at ${BUNDLE_DIR}"
elif [[ -n "${SIGN_IDENTITY}" ]]; then
    echo "Packaged and Developer ID signed macOS bundle at ${BUNDLE_DIR} (notarization skipped; pass --notarize --notary-profile <profile> to notarize)"
else
    echo "Packaged macOS bundle at ${BUNDLE_DIR} (ad-hoc local signature; Developer ID signing/notarization NOT performed)"
fi
