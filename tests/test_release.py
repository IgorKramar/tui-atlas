"""Проверки релизных архивов без Rust-сборки и сети."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("release", Path(__file__).resolve().parents[1] / "scripts/release.py")
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


class ReleaseTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.commit = "a" * 40
        self.target = release.TARGETS[0]
        self.meta = {"name": "forge-deck", "version": "0.3.0", "bins": ["forge-deck", "gitlab-deck"],
                     "target_directory": self.root / "custom-target"}
        for name in release.assets(self.meta):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(name)
        for target in release.TARGETS:
            for name in self.meta["bins"]:
                path = self.meta["target_directory"] / target / "release" / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("#!/bin/sh\nprintf 'forge-deck 0.3.0\\n'\n")
                path.chmod(0o755)
        for name in ["docs/plans/private.md", "docs/solutions/secret.md", ".env", ".git/config", "config.json", "src/private.rs"]:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("PRIVATE")
        self.output = self.root / "dist"

    def package(self, target=None, tag="v0.3.0"):
        return release.package(self.root, self.meta, target or self.target, tag, self.commit, self.output)

    def test_archive_contains_exact_allowlist_all_binaries_and_executable_bits(self):
        archive = self.package()
        self.assertEqual(archive.name, "forge-deck-v0.3.0-" + self.target + ".tar.gz")
        with tarfile.open(archive) as bundle:
            stem = archive.name.removesuffix(".tar.gz")
            expected = set(release.assets(self.meta) + self.meta["bins"] + ["BUILDINFO.json"])
            names = {item.name.removeprefix(stem + "/") for item in bundle}
            self.assertEqual(names, expected)
            self.assertTrue(names.isdisjoint({"docs/plans/private.md", "docs/solutions/secret.md", ".env", ".git/config", "config.json", "src/private.rs"}))
            self.assertFalse(any(b"PRIVATE" in bundle.extractfile(item).read() for item in bundle))
            for name in self.meta["bins"]:
                self.assertTrue(bundle.getmember(stem + "/" + name).mode & 0o111)
            info = json.load(bundle.extractfile(stem + "/BUILDINFO.json"))
            self.assertEqual(info, {"commit": self.commit, "target": self.target, "version": "0.3.0"})
        release.inspect_archive(self.meta, self.target, archive, "v0.3.0", self.commit)

    def test_invalid_tag_target_missing_docs_and_symlinks_are_rejected(self):
        with self.assertRaises(ValueError):
            self.package(tag="v9.0.0")
        with self.assertRaises(ValueError):
            self.package(target="../../escape")
        readme = self.root / "README.md"
        readme.unlink()
        with self.assertRaises(ValueError):
            self.package()
        readme.symlink_to(self.root / "LICENSE")
        with self.assertRaises(ValueError):
            self.package()
        readme.unlink()
        readme.write_text("README")
        screenshots = self.root / "docs/screenshots"
        screenshots.rename(self.root / "images")
        screenshots.symlink_to(self.root / "images", target_is_directory=True)
        with self.assertRaises(ValueError):
            self.package()

    def test_archive_rejects_traversal_links_duplicates_and_wrong_buildinfo(self):
        for mutation in ["traversal", "link", "duplicate", "info", "permissions"]:
            archive = self.package()
            with tarfile.open(archive) as bundle:
                entries = [(item, bundle.extractfile(item).read()) for item in bundle]
            with tarfile.open(archive, "w:gz") as bundle:
                for index, (item, data) in enumerate(entries):
                    if mutation == "info" and item.name.endswith("BUILDINFO.json"):
                        data = b"{}"
                        item.size = len(data)
                    if index == 0 and mutation == "traversal":
                        item.name = "../outside"
                    if index == 0 and mutation == "link":
                        item.type = tarfile.SYMTYPE
                        item.linkname = "../outside"
                    if mutation == "permissions" and item.name.endswith("/forge-deck"):
                        item.mode = 0o644
                    bundle.addfile(item, io.BytesIO(data))
                if mutation == "duplicate":
                    item, data = entries[0]
                    bundle.addfile(item, io.BytesIO(data))
            with self.assertRaises(ValueError, msg=mutation):
                release.inspect_archive(self.meta, self.target, archive, "v0.3.0", self.commit)

    def test_extracted_binaries_and_smokes_run_outside_repository_with_temporary_home(self):
        scripts = self.root / "tests"
        scripts.mkdir()
        for name in ["terminal_smoke.py", "provider_hosts.py"]:
            (scripts / name).write_text(
                "import os,sys; from pathlib import Path\n"
                "assert Path.cwd() != Path(__file__).resolve().parents[1]\n"
                "assert Path(os.environ['HOME']).is_dir()\n"
                "assert Path(os.environ['XDG_CONFIG_HOME']).parent == Path(os.environ['HOME'])\n"
                "assert Path(sys.argv[1]).is_file()\n"
                "Path(__file__).with_suffix('.ok').touch()\n")
        release.check(self.root, self.meta, self.target, self.package(), "v0.3.0", self.commit)
        self.assertTrue((scripts / "terminal_smoke.ok").is_file())
        self.assertTrue((scripts / "provider_hosts.ok").is_file())
        binary = self.meta["target_directory"] / self.target / "release/forge-deck"
        binary.write_text("#!/bin/sh\nprintf 'forge-deck 9.0.0\\n'\n")
        with self.assertRaises(ValueError):
            release.check(self.root, self.meta, self.target, self.package(), "v0.3.0", self.commit)

    def test_atlas_has_one_binary_and_only_its_public_docs(self):
        self.meta.update(name="tui-atlas", version="0.2.0", bins=["tui-atlas"])
        for name in release.assets(self.meta):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(name)
        binary = self.meta["target_directory"] / self.target / "release/tui-atlas"
        binary.write_text("#!/bin/sh\nprintf 'tui-atlas 0.2.0\\n'\n")
        binary.chmod(0o755)
        archive = self.package(tag="v0.2.0")
        release.inspect_archive(self.meta, self.target, archive, "v0.2.0", self.commit)
        with tarfile.open(archive) as bundle:
            self.assertFalse(any("adr/" in item.name or "gitlab-deck" in item.name for item in bundle))

    def test_checksums_require_both_archives_matching_version_and_tag_commit(self):
        with patch.object(release, "tag_commit", return_value=self.commit):
            self.package()
            with self.assertRaises(ValueError):
                release.checksums(self.root, self.meta, "v0.3.0", self.commit, self.output)
            second = self.package(release.TARGETS[1])
            result = release.checksums(self.root, self.meta, "v0.3.0", self.commit, self.output)
            lines = result.read_text().splitlines()
            self.assertEqual(len(lines), 2)
            self.assertIn(hashlib.sha256(second.read_bytes()).hexdigest() + "  " + second.name, lines)
            with self.assertRaises(ValueError):
                release.checksums(self.root, self.meta, "v9.0.0", self.commit, self.output)
            (self.output / "unexpected.tar.gz").write_bytes(b"unexpected")
            with self.assertRaises(ValueError):
                release.checksums(self.root, self.meta, "v0.3.0", self.commit, self.output)
        with patch.object(release, "tag_commit", return_value="b" * 40):
            with self.assertRaises(ValueError):
                release.checksums(self.root, self.meta, "v0.3.0", self.commit, self.output)


if __name__ == "__main__":
    unittest.main()
