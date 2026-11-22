use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

pub const RUNTIME_EXTENSIONS: &[&str] = &["qml", "js", "ttf", "otf", "png", "svg", "jpg", "qm"];

pub fn is_runtime_file(path: &Path) -> bool {
    if path.file_name().and_then(|n| n.to_str()) == Some("qmldir") {
        return true;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            RUNTIME_EXTENSIONS
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        })
        .unwrap_or(false)
}

pub fn collect_runtime_files_with_dirs(
    root: &Path,
    on_dir: &mut dyn FnMut(&Path),
) -> io::Result<Vec<String>> {
    let mut out = Vec::new();
    collect_recursive(root, root, on_dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_recursive(
    root: &Path,
    current: &Path,
    on_dir: &mut dyn FnMut(&Path),
    out: &mut Vec<String>,
) -> io::Result<()> {
    on_dir(current);
    let mut entries: Vec<_> = fs::read_dir(current)?.flatten().collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if path.is_dir() {
            if name_str == "__pycache__" || name_str.starts_with('.') {
                continue;
            }
            collect_recursive(root, &path, on_dir, out)?;
        } else if is_runtime_file(&path) {
            let rel = path
                .strip_prefix(root)
                .map_err(io::Error::other)?
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    Ok(())
}

/// Sanitize a single `qmldir` file against the actual files present beside it:
/// - Comment out declarations whose target QML/JS file does not exist.
/// - Normalize versionless `singleton Type File.qml` and `Type File.qml` entries to `1.0`.
/// - Append `ScrollView 1.0 components/ScrollView.qml` to the root `qmldir` when present.
pub fn sanitize_qmldir_file(
    rel_qmldir: &str,
    src_path: &Path,
) -> io::Result<(String, Vec<(String, String)>)> {
    let qmldir_dir = src_path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "qmldir must have parent"))?;
    let raw = fs::read_to_string(src_path)?;
    Ok(sanitize_qmldir_str(rel_qmldir, &raw, qmldir_dir))
}

pub fn sanitize_qmldir_str(
    rel_qmldir: &str,
    raw: &str,
    qmldir_dir: &Path,
) -> (String, Vec<(String, String)>) {
    let mut out_lines = Vec::new();
    let mut drops = Vec::new();
    let mut declared_types = BTreeSet::new();

    for raw_line in raw.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("module ") {
            out_lines.push(raw_line.to_owned());
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        let Some(target) = parts.last() else {
            out_lines.push(raw_line.to_owned());
            continue;
        };

        let target_path = qmldir_dir.join(target);
        if !target_path.is_file() {
            drops.push((rel_qmldir.to_owned(), trimmed.to_owned()));
            out_lines.push(format!("# [rinui-rs sanitized missing target] {trimmed}"));
            continue;
        }

        // Normalize versionless singleton/type declarations (e.g. `singleton Colors Colors.qml`).
        if parts[0] == "singleton" && parts.len() == 3 {
            let normalized = format!("singleton {} 1.0 {}", parts[1], parts[2]);
            declared_types.insert(parts[1].to_owned());
            out_lines.push(normalized);
        } else if parts[0] != "singleton" && parts.len() == 2 {
            let normalized = format!("{} 1.0 {}", parts[0], parts[1]);
            declared_types.insert(parts[0].to_owned());
            out_lines.push(normalized);
        } else {
            let type_name = if parts[0] == "singleton" {
                parts.get(1).copied().unwrap_or("")
            } else {
                parts[0]
            };
            declared_types.insert(type_name.to_owned());
            out_lines.push(raw_line.to_owned());
        }
    }

    if rel_qmldir == "qmldir"
        && !declared_types.contains("ScrollView")
        && qmldir_dir.join("components/ScrollView.qml").is_file()
    {
        out_lines.push(String::new());
        out_lines.push("# [rinui-rs added missing existing component]".to_owned());
        out_lines.push("ScrollView 1.0 components/ScrollView.qml".to_owned());
    }

    let mut result = out_lines.join("\n");
    result.push('\n');
    (result, drops)
}

/// Collected runtime files and dropped `qmldir` declarations `(file, line)` returned by
/// [`materialize_sanitized_rinui_tree`].
#[allow(dead_code)]
pub type SanitizedTree = (Vec<String>, Vec<(String, String)>);

fn inject_reactive_locale_after_id(text: &str, id_line: &str) -> Option<String> {
    if !text.contains(id_line) || text.contains("    locale:") {
        return None;
    }
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    Some(text.replacen(
        id_line,
        &format!("{id_line}{nl}    locale: qsTr(\"\") === \"\" ? Qt.locale() : Qt.locale()"),
        1,
    ))
}

/// Sanitize specific upstream QML runtime files (without touching `upstream/Rin-UI`)
/// so default locale bindings react to host `QQmlEngine::retranslate()` while preserving
/// any explicit caller `locale:` / `calendarLocale:` overrides.
pub fn sanitize_runtime_qml(rel: &str, src_path: &Path) -> io::Result<Option<String>> {
    let normalized_rel = rel.replace('\\', "/");
    match normalized_rel.as_str() {
        "components/DateAndTime/Calendar.qml" => {
            let text = fs::read_to_string(src_path)?;
            let target = "    property var locale: Qt.locale()";
            if text.contains(target) {
                let replacement =
                    "    property var locale: qsTr(\"\") === \"\" ? Qt.locale() : Qt.locale()";
                return Ok(Some(text.replacen(target, replacement, 1)));
            }
            Ok(None)
        }
        "components/DateAndTime/DatePicker.qml" => {
            let text = fs::read_to_string(src_path)?;
            let mut changed = false;
            let mut updated = match inject_reactive_locale_after_id(&text, "    id: datePicker") {
                Some(replaced) => {
                    changed = true;
                    replaced
                }
                None => text,
            };

            let month_target = "    readonly property var monthModel: (new Array(12)).fill(0).map((_, i) => Qt.locale().monthName(i))";
            if updated.contains(month_target) {
                let month_replacement = "    readonly property var monthModel: (new Array(12)).fill(0).map((_, i) => datePicker.locale.monthName(i))";
                updated = updated.replacen(month_target, month_replacement, 1);
                changed = true;
            }

            if changed {
                Ok(Some(updated))
            } else {
                Ok(None)
            }
        }
        "components/Text/SpinBox.qml" => {
            let text = fs::read_to_string(src_path)?;
            Ok(inject_reactive_locale_after_id(&text, "    id: root"))
        }
        _ => Ok(None),
    }
}

fn use_memory_backed_icon_font(text: String) -> String {
    const NAME: &str = "property string name: fontLoader.name";
    const SOURCE: &str = "source: Utils.fontIconSource";
    // Patch both bindings together; a checkout may have a different loader.
    if !text.contains(NAME) || !text.contains(SOURCE) {
        return text;
    }

    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    text.replacen(
        NAME,
        &format!(
            "readonly property url fontUrl: Qt.resolvedUrl(Utils.fontIconSource){newline}    \
             readonly property var localFamily: RinUIStagedFonts.loadLocalFont(fontUrl){newline}    \
             property string name: localFamily === undefined ? fontLoader.name : localFamily"
        ),
        1,
    )
    .replacen(
        SOURCE,
        "source: localFamily === undefined ? fontUrl : \"\"",
        1,
    )
}

/// Materialize a complete sanitized `RinUI` module directory from `src_rinui_dir` into
/// `dst_rinui_dir`, preserving directory layout while rewriting `qmldir` and host-patched QML files.
#[allow(dead_code)]
pub fn materialize_sanitized_rinui_tree(
    src_rinui_dir: &Path,
    dst_rinui_dir: &Path,
) -> io::Result<SanitizedTree> {
    let files = collect_runtime_files_with_dirs(src_rinui_dir, &mut |_| {})?;
    let mut drops = Vec::new();

    for rel in &files {
        let src_path = src_rinui_dir.join(rel);
        let dst_path = dst_rinui_dir.join(rel);
        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if Path::new(rel).file_name().and_then(|n| n.to_str()) == Some("qmldir") {
            let (sanitized_text, file_drops) = sanitize_qmldir_file(rel, &src_path)?;
            drops.extend(file_drops);
            fs::write(&dst_path, sanitized_text)?;
        } else if cfg!(windows) && rel == "utils/FontIconLoader.qml" {
            // Qt's FontLoader caches file-based application fonts beyond engine teardown.
            // Load local fonts from memory so Windows can delete the staged files.
            fs::write(
                &dst_path,
                use_memory_backed_icon_font(fs::read_to_string(&src_path)?),
            )?;
        } else if let Some(patched) = sanitize_runtime_qml(rel, &src_path)? {
            fs::write(&dst_path, patched)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok((files, drops))
}
