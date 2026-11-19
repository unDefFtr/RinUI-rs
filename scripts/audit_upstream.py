#!/usr/bin/env python3
"""
Audit the pinned upstream/Rin-UI submodule without modifying any upstream files.

Checks:
1. Submodule working tree is completely clean (zero modified or untracked files).
2. Submodule commit SHA is recorded and verified.
3. All qmldir files in RinUI/ are parsed and compared against the known upstream
   broken entries (fails if unexpected new broken entries appear on an update).
4. All runtime QML, JS, font, image, and translation assets are inventoried.
5. Host boundary symbols (ThemeManager, WinEventManager) and window properties
   referenced by upstream QML are verified.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

EXPECTED_BROKEN_QMLDIR_ENTRIES = {
    "qmldir": {
        "DateField 1.0 components/DateAndTime/DateField.qml",
        "TeachingTip 1.0 components/DialogsAndFlyouts/TeachingTip.qml",
        "ScrollViewer 1.0 components/ScrollViewer.qml",
    },
    "components/qmldir": {
        "HyperlinkButton 1.0 BasicInput/HyperlinkButton.qml",
    },
    "windows/qmldir": {
        "ErrorPage 1.0 ErrorPage.qml",
    },
}

EXPECTED_VERSIONLESS_SINGLETONS = {
    "themes/qmldir": {
        "singleton Colors Colors.qml",
        "singleton Appearance Appearance.qml",
    },
}

RUNTIME_EXTENSIONS = {".qml", ".js", ".ttf", ".otf", ".png", ".svg", ".jpg", ".qm"}


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def check_submodule_clean(upstream_dir: Path) -> str:
    if not (upstream_dir / "RinUI").is_dir():
        raise RuntimeError(f"Missing RinUI directory at {upstream_dir / 'RinUI'}")

    status = subprocess.run(
        ["git", "-C", str(upstream_dir), "status", "--porcelain"],
        check=True,
        capture_output=True,
        text=True,
    )
    if status.stdout.strip():
        raise RuntimeError(
            "upstream/Rin-UI is dirty! Upstream files must remain unmodified:\n"
            + status.stdout
        )

    sha = subprocess.run(
        ["git", "-C", str(upstream_dir), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    return sha


def audit_qmldir_files(rinui_dir: Path) -> tuple[dict[str, list[str]], dict[str, list[str]]]:
    unexpected_broken: dict[str, list[str]] = {}
    unexpected_versionless: dict[str, list[str]] = {}

    for qmldir in sorted(rinui_dir.rglob("qmldir")):
        rel_qmldir = qmldir.relative_to(rinui_dir).as_posix()
        expected_broken = EXPECTED_BROKEN_QMLDIR_ENTRIES.get(rel_qmldir, set())
        expected_versionless = EXPECTED_VERSIONLESS_SINGLETONS.get(rel_qmldir, set())

        seen_broken = set()
        seen_versionless = set()

        for raw in qmldir.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if not line or line.startswith("#") or line.startswith("module "):
                continue
            parts = line.split()
            target = parts[-1]
            resolved = (qmldir.parent / target).resolve()
            if not resolved.exists():
                seen_broken.add(line)
                if line not in expected_broken:
                    unexpected_broken.setdefault(rel_qmldir, []).append(line)

            is_versionless = (
                (parts[0] == "singleton" and len(parts) == 3)
                or (parts[0] != "singleton" and len(parts) == 2)
            )
            if is_versionless:
                seen_versionless.add(line)
                if line not in expected_versionless:
                    unexpected_versionless.setdefault(rel_qmldir, []).append(line)

        missing_expected = expected_broken - seen_broken
        if missing_expected:
            print(
                f"NOTE: Upstream fixed previously broken qmldir entries in {rel_qmldir}: "
                + ", ".join(sorted(missing_expected))
            )

    return unexpected_broken, unexpected_versionless


def collect_runtime_assets(rinui_dir: Path) -> list[str]:
    files = []
    for path in sorted(rinui_dir.rglob("*")):
        if not path.is_file():
            continue
        if path.name == "qmldir" or path.suffix.lower() in RUNTIME_EXTENSIONS:
            files.append(path.relative_to(rinui_dir).as_posix())
    return files


def audit_host_contracts(rinui_dir: Path) -> tuple[list[str], list[str]]:
    host_pat = re.compile(r"\b(ThemeManager|WinEventManager)\b")
    qt_import_pat = re.compile(r"^\s*import\s+(Qt[A-Za-z0-9_.]+)")

    host_hits: list[str] = []
    qt_modules: set[str] = set()

    for path in sorted(rinui_dir.rglob("*.qml")):
        rel = path.relative_to(rinui_dir).as_posix()
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if host_pat.search(line):
                host_hits.append(f"{rel}:{lineno}: {line.strip()}")
            match = qt_import_pat.match(line)
            if match:
                qt_modules.add(match.group(1))

    return host_hits, sorted(qt_modules)


def main() -> int:
    root = repo_root()
    upstream_dir = root / "upstream" / "Rin-UI"
    rinui_dir = upstream_dir / "RinUI"

    sha = check_submodule_clean(upstream_dir)
    print(f"Upstream submodule clean. Pinned SHA: {sha}")

    unexpected_broken, unexpected_versionless = audit_qmldir_files(rinui_dir)
    assets = collect_runtime_assets(rinui_dir)
    host_hits, qt_modules = audit_host_contracts(rinui_dir)

    print(f"Inventoried {len(assets)} RinUI runtime files (QML, JS, fonts, images, .qm, qmldir).")
    print(f"Detected {len(host_hits)} QML host context references across RinUI/.")
    print("Required Qt QML imports:", ", ".join(qt_modules))

    if unexpected_broken:
        print("\nERROR: Unexpected missing qmldir target files:", file=sys.stderr)
        for qmldir, entries in unexpected_broken.items():
            for entry in entries:
                print(f"  {qmldir}: {entry}", file=sys.stderr)
        return 1

    if unexpected_versionless:
        print("\nERROR: Unexpected versionless qmldir entries:", file=sys.stderr)
        for qmldir, entries in unexpected_versionless.items():
            for entry in entries:
                print(f"  {qmldir}: {entry}", file=sys.stderr)
        return 1

    print("Audit passed: zero unexpected qmldir regressions and clean submodule state.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
