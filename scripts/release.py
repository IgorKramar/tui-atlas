"""Сборка и проверка автономных Linux-архивов; только стандартная библиотека."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile

TARGETS = ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu")
PUBLIC_DOCS = {
    "tui-atlas": ["docs/data-contract.md", "docs/data-contract.ru.md",
                  "docs/screenshots/overview.png", "docs/screenshots/shortcuts.png",
                  "docs/screenshots/ru/overview.png", "docs/screenshots/ru/shortcuts.png"],
    "forge-deck": [
        "docs/adr/0001-terminal-and-authentication.md", "docs/adr/0002-multiple-forges.md",
        "docs/screenshots/projects.png", "docs/screenshots/pipelines.png", "docs/screenshots/settings.png",
        "docs/screenshots/github/projects.png", "docs/screenshots/github/pipelines.png", "docs/screenshots/github/settings.png",
        "docs/screenshots/ru/projects.png", "docs/screenshots/ru/pipelines.png", "docs/screenshots/ru/settings.png",
        "docs/screenshots/ru/github/projects.png", "docs/screenshots/ru/github/pipelines.png", "docs/screenshots/ru/github/settings.png",
    ],
}


def metadata(root):
    data = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"], cwd=root))
    package = next(p for p in data["packages"] if Path(p["manifest_path"]) == root / "Cargo.toml")
    return {"name": package["name"], "version": package["version"],
            "bins": sorted(t["name"] for t in package["targets"] if "bin" in t["kind"]),
            "target_directory": Path(data["target_directory"])}


def assets(meta):
    return ["README.md", "README.ru.md", "LICENSE", "CHANGELOG.md"] + PUBLIC_DOCS[meta["name"]]


def stem(meta, target, tag, commit):
    if target not in TARGETS:
        raise ValueError("Unsupported target: " + target)
    if tag != "v" + meta["version"]:
        raise ValueError("Tag must match Cargo version: v" + meta["version"])
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Expected a full Git commit SHA")
    return meta["name"] + "-" + tag + "-" + target


def regular_file(path, base):
    relative = path.relative_to(base)
    if any((base / part).is_symlink() for part in [relative, *relative.parents]):
        raise ValueError("Symlinks are not release assets: " + str(path))
    if not path.is_file():
        raise ValueError("Missing release file: " + str(path))


def package(root, meta, target, tag, commit, output):
    name = stem(meta, target, tag, commit)
    files = [(root / item, item, root) for item in assets(meta)]
    build = meta["target_directory"] / target / "release"
    files += [(build / item, item, meta["target_directory"]) for item in meta["bins"]]
    for path, item, base in files:
        regular_file(path, base)
        if item in meta["bins"] and not path.stat().st_mode & 0o111:
            raise ValueError("Binary is not executable: " + str(path))
    output.mkdir(parents=True, exist_ok=True)
    archive = output / (name + ".tar.gz")
    with tarfile.open(archive, "w:gz", dereference=False) as bundle:
        for path, item, _ in files:
            bundle.add(path, arcname=name + "/" + item, recursive=False)
        info = json.dumps({"commit": commit, "target": target, "version": meta["version"]}, sort_keys=True).encode() + b"\n"
        entry = tarfile.TarInfo(name + "/BUILDINFO.json")
        entry.size = len(info)
        entry.mode = 0o644
        bundle.addfile(entry, io.BytesIO(info))
    inspect_archive(meta, target, archive, tag, commit)
    return archive


def inspect_archive(meta, target, archive, tag, commit):
    name = stem(meta, target, tag, commit)
    if archive.name != name + ".tar.gz":
        raise ValueError("Unexpected archive name: " + archive.name)
    expected = {name + "/" + item for item in assets(meta) + meta["bins"] + ["BUILDINFO.json"]}
    with tarfile.open(archive) as bundle:
        entries = bundle.getmembers()
        if len(entries) != len(expected) or {item.name for item in entries} != expected:
            raise ValueError("Archive must contain exactly the public release allowlist")
        if any(not item.isfile() for item in entries):
            raise ValueError("Archive contains a link or a non-regular file")
        for binary in meta["bins"]:
            if not bundle.getmember(name + "/" + binary).mode & 0o111:
                raise ValueError("Archive lost binary executable permissions")
        info = json.load(bundle.extractfile(name + "/BUILDINFO.json"))
        if info != {"commit": commit, "target": target, "version": meta["version"]}:
            raise ValueError("Archive build information does not match the release")
    return name


def check(root, meta, target, archive, tag, commit):
    name = inspect_archive(meta, target, archive, tag, commit)
    with tempfile.TemporaryDirectory(prefix="release-check-") as directory:
        temp = Path(directory)
        with tarfile.open(archive) as bundle:
            for item in bundle:
                destination = temp / item.name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(bundle.extractfile(item).read())
                destination.chmod(item.mode & 0o777)
        home = temp / "home"
        home.mkdir()
        environment = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(home / "config")}
        for binary in meta["bins"]:
            executable = temp / name / binary
            version = subprocess.check_output([str(executable), "--version"], cwd=temp, env=environment, text=True, timeout=10)
            if meta["version"] not in version.split():
                raise ValueError("Extracted binary has an unexpected version")
            subprocess.run([str(executable), "--help"], cwd=temp, env=environment, check=True, stdout=subprocess.DEVNULL, timeout=10)
        executable = temp / name / meta["name"]
        scripts = ["terminal_smoke.py"]
        if meta["name"] == "forge-deck":
            scripts.append("provider_hosts.py")
        for script in scripts:
            subprocess.run([sys.executable, str(root / "tests" / script), str(executable)],
                           cwd=temp, env=environment, check=True, timeout=120)


def tag_commit(root, tag):
    return subprocess.check_output(["git", "rev-parse", "--verify", "refs/tags/" + tag + "^{commit}"], cwd=root, text=True).strip()


def checksums(root, meta, tag, commit, output):
    expected = [stem(meta, target, tag, commit) + ".tar.gz" for target in TARGETS]
    if tag_commit(root, tag) != commit:
        raise ValueError("Tag does not point to the workflow commit")
    if sorted(path.name for path in output.glob("*.tar.gz")) != sorted(expected):
        raise ValueError("Expected exactly one archive for each supported target")
    lines = []
    for target, name in zip(TARGETS, expected):
        archive = output / name
        inspect_archive(meta, target, archive, tag, commit)
        digest = hashlib.sha256()
        with archive.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
        lines.append(digest.hexdigest() + "  " + name + "\n")
    result = output / "SHA256SUMS"
    result.write_text("".join(lines))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["package", "check", "checksums"])
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--tag")
    parser.add_argument("--commit", required=True)
    parser.add_argument("--output", type=Path, default=Path("dist"))
    parser.add_argument("--archive", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    meta = metadata(root)
    tag = args.tag or "v" + meta["version"]
    if args.command == "checksums":
        print(checksums(root, meta, tag, args.commit, args.output.resolve()))
    else:
        if not args.target:
            parser.error("--target is required")
        if args.command == "package":
            print(package(root, meta, args.target, tag, args.commit, args.output.resolve()))
        else:
            if not args.archive:
                parser.error("--archive is required")
            check(root, meta, args.target, args.archive.resolve(), tag, args.commit)


if __name__ == "__main__":
    main()
