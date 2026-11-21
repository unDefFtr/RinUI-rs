#!/usr/bin/env python3
"""Validate tagged source and assemble or verify its GitHub Release."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import tomllib


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def output(path_var, text):
    if path := os.environ.get(path_var):
        with open(path, "a", encoding="utf-8") as file:
            file.write(text + "\n")


def validate_source(tag, commit):
    if command("git", "rev-parse", "HEAD") != commit:
        raise ValueError("Checkout does not match the release commit")
    changes = command("git", "status", "--porcelain", "--untracked-files=no")
    untracked = command(
        "git", "ls-files", "--others", "--exclude-standard", "--",
        ".github", "scripts", "packaging", "crates", "examples", "upstream",
    )
    if changes or untracked:
        raise ValueError(f"Release checkout has uncommitted changes:\n{changes}\n{untracked}")
    version = tomllib.loads(Path("Cargo.toml").read_text())["workspace"]["package"]["version"]
    if tag != f"v{version}" or "+" in version:
        raise ValueError(f"Tag {tag!r} does not match workspace version {version!r}")
    packages = json.loads(command("cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"))["packages"]
    members = {package["name"]: package for package in packages}
    for name in ("rinui-platform", "rinui-host", "rinui-gallery"):
        if name not in members or members[name]["version"] != version:
            raise ValueError(f"{name} does not use workspace version {version}")
        if (members[name]["publish"] == []) != (name == "rinui-gallery"):
            raise ValueError(f"Unexpected Cargo publish policy for {name}")
    for name, dependency in (("rinui-host", "rinui-platform"), ("rinui-gallery", "rinui-host")):
        matches = [dep for dep in members[name]["dependencies"] if dep["name"] == dependency]
        if len(matches) != 1 or matches[0]["req"] not in (f"^{version}", f"={version}"):
            raise ValueError(f"{name} must depend on registry version {version} of {dependency}")
    submodules = command("git", "submodule", "status", "--recursive").splitlines()
    if not submodules or any(line.startswith(("-", "+", "U")) for line in submodules):
        raise ValueError("Submodules must be initialized at their recorded commits")
    print(f"Validated {tag} at {commit}")
    output("GITHUB_OUTPUT", f"version={version}\ncommit={commit}")


def assets(tag):
    prefix = f"RinUIGallery-{tag}"
    return {
        f"{prefix}-windows-x64.zip": "gallery-windows",
        f"{prefix}-windows-x64-unsigned.msix": "gallery-windows",
        f"{prefix}-macos-arm64.zip": "gallery-macos",
        f"{prefix}-linux-x64.tar.gz": "gallery-linux",
    }


def gh(*args):
    return command("gh", *args)


def api(path, *args):
    return json.loads(gh("api", f"repos/{os.environ['GH_REPO']}/{path}", *args))


def check_remote_tag(tag, commit):
    ref = api(f"git/ref/tags/{tag}")["object"]
    while ref["type"] == "tag":
        ref = api(f"git/tags/{ref['sha']}")["object"]
    if ref["type"] != "commit" or ref["sha"] != commit:
        raise ValueError(f"Remote {tag} does not resolve to {commit}")


def find_release(tag):
    # The list endpoint includes unpublished drafts for callers with push access.
    pages = api("releases?per_page=100", "--paginate", "--slurp")
    matches = [release for page in pages for release in page if release["tag_name"] == tag]
    if len(matches) > 1:
        raise ValueError(f"Multiple releases already exist for {tag}")
    return matches[0] if matches else None


def marker(commit, run):
    return f"<!-- rinui-release commit={commit} run={run} -->"


def release_run(release, commit, tag):
    match = re.search(r"<!-- rinui-release commit=([0-9a-f]{40}) run=([0-9]+) -->", release["body"] or "")
    if not match or match[1] != commit or release["target_commitish"] != commit:
        raise ValueError(f"Release {tag} is missing the pipeline's source commit metadata")
    if release["prerelease"] != ("-" in tag):
        raise ValueError(f"The GitHub prerelease flag does not match {tag}")
    return match[2]


def digest(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def remote_assets(release):
    pages = api(f"releases/{release['id']}/assets?per_page=100", "--paginate", "--slurp")
    return {asset["name"]: asset for page in pages for asset in page}


def verify_assets(release, tag, checksum_path):
    remote = remote_assets(release)
    names = set(assets(tag))
    if set(remote) != names | {"SHA256SUMS.txt"}:
        raise ValueError(f"Unexpected release asset set: {', '.join(sorted(remote))}")
    checksums = {}
    for line in checksum_path.read_text(encoding="ascii").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  (.+)", line)
        if not match or match[2] in checksums:
            raise ValueError("Malformed or duplicate entry in SHA256SUMS.txt")
        checksums[match[2]] = match[1]
    if set(checksums) != names:
        raise ValueError("SHA256SUMS.txt must list all four Gallery assets exactly once")
    checksums["SHA256SUMS.txt"] = digest(checksum_path)
    for name, sha in checksums.items():
        asset = remote[name]
        if asset["state"] != "uploaded" or asset["size"] <= 0 or asset["digest"] != f"sha256:{sha}":
            raise ValueError(f"GitHub asset is incomplete or its digest differs: {name}")


def verify_published(release, tag, commit):
    if release is None or release["draft"] or not release["published_at"]:
        raise ValueError(f"No published GitHub Release for {tag}")
    run_id = release_run(release, commit, tag)
    run = api(f"actions/runs/{run_id}")
    if run["path"] != ".github/workflows/release.yml" or run["conclusion"] != "success":
        raise ValueError(f"Release build run {run_id} must complete successfully before publishing crates")
    with tempfile.TemporaryDirectory() as directory:
        gh("release", "download", tag, "--pattern", "SHA256SUMS.txt", "--dir", directory)
        verify_assets(release, tag, Path(directory) / "SHA256SUMS.txt")
    print(f"Verified published release and all asset digests: {release['html_url']}")


def notes(tag, commit, run, signing):
    macos = {
        "adhoc": "Ad-hoc signed. Gatekeeper may block this download because it has no Developer ID or notarization.",
        "developer-id": "Developer ID signed. This build was not notarized.",
        "notarized": "Developer ID signed and notarized with a stapled ticket.",
    }[signing]
    version = tag[1:]
    return f"""{marker(commit, run)}
## rinui-rs {tag}

RinUI is a Rust-native Qt Quick/QML host with Fluent-style controls. The Gallery demonstrates theme switching, localization, multi-window sessions, and native window integration. See the [changelog](https://github.com/{os.environ['GH_REPO']}/blob/{commit}/CHANGELOG.md) for details.

| Asset | How to run | Verification and requirements |
| --- | --- | --- |
| Windows x64 ZIP | Extract the full ZIP and run `rinui-gallery.exe`. | Windows QPA smoke test on Windows Server 2025. Requires the [x64 Visual C++ Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist). The executable is unsigned. |
| Windows x64 MSIX | Test package for packaging and sideloading evaluation. Use the ZIP for normal use. | Unsigned. Pack and unpack layout verified. Installation and launch from MSIX were not tested. Manifest requires Windows 10 build 19041 or newer. |
| macOS arm64 ZIP | Expand the ZIP and open `RinUIGallery.app` on Apple Silicon. | Cocoa smoke tests on macOS 26, including both window frame modes. {macos} |
| Linux x64 tar.gz | Extract the archive and run `./run-gallery.sh` from the extracted directory. | Tested on Ubuntu 24.04 x64 with offscreen and Xvfb/XCB. Requires the host glibc, C++ runtime, XCB/X11 and graphics libraries. Other distributions and native Wayland were not tested. |

The Gallery packages bundle Qt 6.12.0 and the required Qt QML modules. No system Qt installation is needed for the Gallery. Desktop/window-manager behavior outside the smoke tests has not been verified. Verify the downloads with `sha256sum -c SHA256SUMS.txt` or `shasum -a 256 -c SHA256SUMS.txt`.

## Rust libraries

- [rinui-platform {version}](https://crates.io/crates/rinui-platform/{version})
- [rinui-host {version}](https://crates.io/crates/rinui-host/{version})

crates.io publication is tracked separately and the links become available after successful registry publication. Building the host from crates.io requires Rust and a Qt development installation. See the [build instructions](https://github.com/{os.environ['GH_REPO']}/blob/{commit}/README.md).
"""


def draft(tag, commit, directory):
    release = find_release(tag)
    if release and not release["draft"]:
        verify_published(release, tag, commit)
        return
    if release:
        release_run(release, commit, tag)
    files = {name: directory / folder / name for name, folder in assets(tag).items()}
    for file in files.values():
        if not file.is_file() or not file.stat().st_size:
            raise ValueError(f"Missing or empty Gallery artifact: {file}")
    signing = (directory / "gallery-macos" / "macos-signing.txt").read_text().strip()
    with tempfile.TemporaryDirectory() as temp:
        checksum_path = Path(temp) / "SHA256SUMS.txt"
        checksums = {name: digest(file) for name, file in sorted(files.items())}
        checksum_path.write_text("".join(f"{sha}  {name}\n" for name, sha in checksums.items()), encoding="ascii")
        files[checksum_path.name] = checksum_path
        checksums[checksum_path.name] = digest(checksum_path)
        notes_path = Path(temp) / "notes.md"
        notes_path.write_text(notes(tag, commit, os.environ["GITHUB_RUN_ID"], signing), encoding="utf-8")
        if release is None:
            gh(
                "release", "create", tag, "--draft", "--verify-tag", "--target", commit,
                f"--prerelease={'true' if '-' in tag else 'false'}", "--title", f"rinui-rs {tag}",
                "--notes-file", str(notes_path),
            )
            release = find_release(tag)
            if release is None or not release["draft"]:
                raise ValueError(f"New draft for {tag} could not be located")
        remote = remote_assets(release)
        if set(remote) - set(files):
            raise ValueError(f"Draft {tag} contains an unexpected asset. Review the draft before retrying.")
        # Keep matching uploads after a failure. Overwrite only changed assets on our draft.
        for name, file in files.items():
            if name in remote and remote[name]["state"] == "uploaded" and remote[name]["digest"] == f"sha256:{checksums[name]}":
                continue
            gh("release", "upload", tag, str(file), "--clobber")
        verify_assets(release, tag, checksum_path)
        gh("release", "edit", tag, "--notes-file", str(notes_path))
    message = f"Verified draft and all asset digests: {release['html_url']}"
    print(message)
    output("GITHUB_STEP_SUMMARY", message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("source", "draft", "published"))
    parser.add_argument("tag")
    parser.add_argument("commit")
    parser.add_argument("directory", nargs="?", type=Path)
    args = parser.parse_args()
    if args.operation == "source":
        validate_source(args.tag, args.commit)
    else:
        check_remote_tag(args.tag, args.commit)
        if args.operation == "draft":
            if args.directory is None:
                parser.error("draft requires the downloaded artifacts directory")
            draft(args.tag, args.commit, args.directory)
        else:
            verify_published(find_release(args.tag), args.tag, args.commit)


if __name__ == "__main__":
    try:
        main()
    except (KeyError, OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f"Release error: {error}")
