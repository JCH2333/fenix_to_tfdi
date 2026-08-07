import json
import hashlib
import tempfile
import unittest
import zipfile
from pathlib import Path

from build_update_package import build_package
from gui import read_update_result
from update_manager import GITHUB_API_URL, UpdateError, check_for_update, validate_update_package
from version import __version__


class FakeResponse:
    def __init__(self, payload: bytes):
        self.payload = payload
        self.offset = 0
        self.headers = {"Content-Length": str(len(payload))}

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return False

    def read(self, size=-1):
        if self.offset >= len(self.payload):
            return b""
        if size < 0:
            size = len(self.payload) - self.offset
        chunk = self.payload[self.offset:self.offset + size]
        self.offset += len(chunk)
        return chunk


class UpdateCheckTests(unittest.TestCase):
    def test_finds_verified_tfdi_update_package(self):
        payload = {
            "tag_name": "v0.2.1",
            "name": "v0.2.1",
            "assets": [{
                "name": "fenix_to_tfdi-v0.2.1.zip",
                "browser_download_url": (
                    "https://github.com/JCH2333/fenix_to_tfdi/releases/"
                    "download/v0.2.1/fenix_to_tfdi-v0.2.1.zip"
                ),
                "digest": "sha256:" + "a" * 64,
                "size": 1234,
            }],
        }

        result = check_for_update(
            "0.2.0",
            opener=lambda request, timeout: FakeResponse(
                json.dumps(payload).encode("utf-8")
            ),
        )

        self.assertEqual(GITHUB_API_URL.rsplit("/", 1)[0],
                         "https://api.github.com/repos/JCH2333/fenix_to_tfdi/releases")
        self.assertTrue(result.update_available)
        self.assertEqual(result.release.version, "0.2.1")
        self.assertEqual(result.release.asset_sha256, "a" * 64)


class UpdatePackageTests(unittest.TestCase):
    def test_rejects_payload_not_listed_in_manifest(self):
        files = {
            "fenix_to_tfdi.exe": b"converter",
            "gui.py": b"gui",
            "gui_logic.py": b"logic",
            "run_gui.bat": b"launcher",
            "update_manager.py": b"updater",
            "version.py": b'__version__ = "0.2.1"\n',
        }
        manifest = {
            "version": "0.2.1",
            "files": {
                name: hashlib.sha256(content).hexdigest()
                for name, content in files.items()
            },
        }
        with tempfile.TemporaryDirectory() as temp_dir:
            package_path = Path(temp_dir) / "update.zip"
            with zipfile.ZipFile(package_path, "w") as package:
                for name, content in files.items():
                    package.writestr(name, content)
                package.writestr("extra.exe", b"unverified")
                package.writestr("update-manifest.json", json.dumps(manifest))

            with self.assertRaisesRegex(UpdateError, "未列入清单"):
                validate_update_package(package_path, "0.2.1")

    def test_builds_a_valid_package_from_a_supplied_converter(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            converter = root / "fenix_to_tfdi.exe"
            converter.write_bytes(b"converter binary")

            package = build_package(root / "packages", converter, __version__)

            self.assertEqual(package.name, f"fenix_to_tfdi-v{__version__}.zip")
            self.assertTrue(package.is_file())
            validate_update_package(package, __version__)


class UpdateResultTests(unittest.TestCase):
    def test_reads_and_removes_installer_result_file(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            result_path = Path(temp_dir) / "result.json"
            result_path.write_text(
                json.dumps({"success": True, "version": "0.2.1"}),
                encoding="utf-8",
            )

            result = read_update_result(["--update-result", str(result_path)])

        self.assertEqual(result, {"success": True, "version": "0.2.1"})
        self.assertFalse(result_path.exists())


if __name__ == "__main__":
    unittest.main()
