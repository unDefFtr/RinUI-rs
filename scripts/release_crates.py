#!/usr/bin/env python3
"""Build release crates and publish or verify them against the public sparse index."""

import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import tarfile
import time
import tomllib
from urllib.error import HTTPError
from urllib.request import Request, urlopen

sys.dont_write_bytecode = True
from release import command, output, validate_source

CRATES = {"rinui-platform": "platform", "rinui-host": "host"}
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
MAX_SIZE = 10 * 1024 * 1024
GENERATED = {"Cargo.toml", "Cargo.toml.orig", "Cargo.lock", ".cargo_vcs_info.json"}


def cargo(*args):
    env = os.environ.copy()
    if args[0] != "publish":
        env.pop("CARGO_REGISTRY_TOKEN", None)
        env.pop("CARGO_REGISTRIES_CRATES_IO_TOKEN", None)
    env.pop("RINUI_UPSTREAM_DIR", None)
    subprocess.run(["cargo", *args], check=True, env=env)


def tracked(crate):
    root = Path("crates") / crate
    return {
        path.relative_to(root).as_posix(): path.read_bytes()
        for name in command("git", "ls-files", "--", str(root)).splitlines()
        if (path := Path(name)).is_file() and path.name != "Cargo.toml"
    }


def archive(data, crate, version, commit):
    label = f"{crate} {version}"
    if len(data) > MAX_SIZE:
        raise ValueError(f"{label} exceeds the crates.io 10 MiB size limit")
    files = {}
    root = f"{crate}-{version}"
    size = 0
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tar:
        for member in tar:
            path = PurePosixPath(member.name)
            if member.isdir():
                continue
            name = "/".join(path.parts[1:])
            size += member.size
            if (not member.isfile() or size > 100 * 1024 * 1024 or path.parts[0] != root
                    or not name or ".." in path.parts or name in files):
                raise ValueError(f"Unexpected archive entry in {label}: {member.name}")
            with tar.extractfile(member) as entry:
                files[name] = entry.read()
    expected = tracked(crate)
    if set(files) != set(expected) | GENERATED:
        missing = (set(expected) | GENERATED) - set(files)
        extra = set(files) - (set(expected) | GENERATED)
        raise ValueError(f"Incomplete or unexpected {label} contents: missing {sorted(missing)}, extra {sorted(extra)}")
    for name, content in expected.items():
        if files[name] != content:
            raise ValueError(f"{label} source differs from tag: {name}")
    if files["Cargo.toml.orig"] != (Path("crates") / crate / "Cargo.toml").read_bytes():
        raise ValueError(f"{label} original manifest differs from tag")
    vcs = json.loads(files[".cargo_vcs_info.json"])
    if vcs["git"]["sha1"] != commit or vcs["git"].get("dirty") or vcs["path_in_vcs"] != f"crates/{crate}":
        raise ValueError(f"{label} does not identify the clean release commit {commit}")
    manifest = tomllib.loads(files["Cargo.toml"].decode())
    if manifest["package"]["version"] != version or manifest["package"]["name"] != crate:
        raise ValueError(f"Unexpected manifest identity in {label}")
    required = ["LICENSE", "README.md", "src/lib.rs"]
    if crate == "rinui-host":
        required += ["build.rs", "THIRD_PARTY_NOTICES.md", "vendor/LICENSE.RinUI", "vendor/LICENSE.FluentUI",
                     "vendor/RinUI/qmldir", "native/src/rinui_qt_bridge.cpp", "tests/fixtures/SmokeTest.qml"]
        dep = manifest["dependencies"]["rinui-platform"]
        lock = tomllib.loads(files["Cargo.lock"].decode())
        platforms = [p for p in lock["package"] if p["name"] == "rinui-platform"]
        if dep != {"version": version} or len(platforms) != 1 or any(
            p["version"] != version or p.get("source") != REGISTRY for p in platforms
        ):
            raise ValueError("Packaged rinui-host does not resolve the matching crates.io rinui-platform")
    if any(not files.get(name) for name in required):
        raise ValueError(f"Necessary license, QML or source missing from {label}")
    return manifest


def request(url, limit):
    req = Request(url, headers={"User-Agent": "rinui-rs releases (https://github.com/unDefFtr/rinui-rs)",
                               "Cache-Control": "no-cache"})
    with urlopen(req, timeout=30) as response:
        data = response.read(limit + 1)
    if len(data) > limit:
        raise ValueError(f"Oversize response: {url}")
    return data


def verify(crate, version, commit, local_manifest):
    try:
        records = request(f"https://index.crates.io/ri/nu/{crate}", MAX_SIZE)
    except HTTPError as error:
        if error.code == 404:
            return False
        raise
    entries = [record for line in records.splitlines() if (record := json.loads(line))["vers"] == version]
    if not entries:
        return False
    if len(entries) != 1 or entries[0]["yanked"]:
        raise ValueError(f"Registry {crate} {version} is yanked or its index entry is ambiguous")
    data = request(f"https://static.crates.io/crates/{crate}/{crate}-{version}.crate", MAX_SIZE)
    if hashlib.sha256(data).hexdigest() != entries[0]["cksum"]:
        raise ValueError(f"Registry checksum mismatch for {crate} {version}")
    published = archive(data, crate, version, commit)
    if published != local_manifest:
        raise ValueError(f"Registry {crate} {version} has a different normalized manifest")
    return True


def local(crate, version, commit):
    metadata = json.loads(command("cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"))
    path = Path(metadata["target_directory"]) / "package" / f"{crate}-{version}.crate"
    return archive(path.read_bytes(), crate, version, commit), path.stat().st_size


def status(version, commit):
    states = {}
    try:
        for crate in CRATES:
            try:
                manifest, _ = local(crate, version, commit)
                states[crate] = verify(crate, version, commit, manifest)
                output("GITHUB_OUTPUT", f"{CRATES[crate]}={str(states[crate]).lower()}")
            except (OSError, ValueError, KeyError, tarfile.TarError):
                states[crate] = None
                raise
    finally:
        lines = [f"### crates.io `{version}`\n"]
        for crate in CRATES:
            state = {True: "published and verified", False: "version absent", None: "verification failed or not run"}[states.get(crate)]
            lines.append(f"- `{crate}` {state}")
        print("\n".join(lines), flush=True)
        output("GITHUB_STEP_SUMMARY", "\n".join(lines))
    return states


def publish(crate, version, commit):
    states = status(version, commit)
    if states[crate]:
        return
    if crate == "rinui-host" and not states["rinui-platform"]:
        raise ValueError("Publish and verify rinui-platform before publishing rinui-host")
    manifest, _ = local(crate, version, commit)
    cargo("publish", "--registry", "crates-io", "-p", crate)
    for _ in range(30):
        if verify(crate, version, commit, manifest):
            print(f"Published and verified {crate} {version}")
            return
        time.sleep(10)
    raise ValueError(f"Upload returned successfully but {crate} {version} is not in the public index yet. Retry after it propagates.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest="action", required=True)
    for action in ("preflight", "status", "publish"):
        sub = actions.add_parser(action)
        if action == "publish":
            sub.add_argument("crate", choices=CRATES)
        sub.add_argument("version")
        sub.add_argument("commit")
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.commit):
        parser.error("commit must be a full lowercase SHA")
    validate_source(f"v{args.version}", args.commit)
    if args.action == "preflight":
        subprocess.run([sys.executable, "scripts/audit_upstream.py"], check=True)
        cargo("package", "--registry", "crates-io", "-p", "rinui-platform", "-p", "rinui-host")
        for crate in CRATES:
            _, size = local(crate, args.version, args.commit)
            message = f"{crate} {args.version}: {size:,} bytes, verified standalone by Cargo"
            print(message)
            output("GITHUB_STEP_SUMMARY", f"- {message}")
    elif args.action == "status":
        status(args.version, args.commit)
    else:
        publish(args.crate, args.version, args.commit)


if __name__ == "__main__":
    try:
        main()
    except (OSError, KeyError, ValueError, tarfile.TarError, subprocess.CalledProcessError) as error:
        sys.exit(f"Crate release error: {error}")
