import json
import unittest

from update_manager import GITHUB_API_URL, check_for_update


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


if __name__ == "__main__":
    unittest.main()
