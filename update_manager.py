"""GitHub Release 检查、验证下载和安全自更新。"""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from typing import Callable
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen
import zipfile

from version import __version__


REPOSITORY = "JCH2333/fenix_to_tfdi"
GITHUB_API_URL = f"https://api.github.com/repos/{REPOSITORY}/releases/latest"
GITHUB_RELEASE_URL = f"https://github.com/{REPOSITORY}/releases"
MIRROR_PREFIXES = ("https://gh-proxy.com/", "https://ghfast.top/")
USER_AGENT = f"fenix-to-tfdi/{__version__}"
MAX_DOWNLOAD_SIZE = 100 * 1024 * 1024
MAX_EXTRACTED_SIZE = 150 * 1024 * 1024
MAX_PACKAGE_FILES = 100
MANIFEST_NAME = "update-manifest.json"
REQUIRED_PROGRAM_FILES = {
    "fenix_to_tfdi.exe",
    "gui.py",
    "gui_logic.py",
    "run_gui.bat",
    "update_manager.py",
    "version.py",
}
VERSION_RE = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)$", re.IGNORECASE)


@dataclass(frozen=True)
class ReleaseInfo:
    version: str
    tag_name: str
    name: str
    page_url: str
    asset_name: str | None = None
    asset_url: str | None = None
    asset_sha256: str | None = None
    asset_size: int | None = None


@dataclass(frozen=True)
class UpdateCheckResult:
    current_version: str
    update_available: bool
    release: ReleaseInfo | None = None
    error: str | None = None


class UpdateError(RuntimeError):
    """更新包无法安全使用。"""


def parse_version(value: str) -> tuple[int, int, int]:
    match = VERSION_RE.fullmatch((value or "").strip())
    if not match:
        raise ValueError(f"不支持的版本号: {value!r}")
    return tuple(int(part) for part in match.groups())


def is_newer_version(latest: str, current: str) -> bool:
    return parse_version(latest) > parse_version(current)


def _request_urls(url: str) -> tuple[str, ...]:
    return (url, *(prefix + url for prefix in MIRROR_PREFIXES))


def _open(request_url: str, opener, timeout: float):
    return opener(
        Request(
            request_url,
            headers={
                "Accept": "application/vnd.github+json",
                "User-Agent": USER_AGENT,
                "X-GitHub-Api-Version": "2022-11-28",
            },
        ),
        timeout=timeout,
    )


def _read_limited(response, limit: int) -> bytes:
    declared = response.headers.get("Content-Length") if response.headers else None
    if declared:
        try:
            if int(declared) > limit:
                raise UpdateError("远程文件超过允许大小")
        except ValueError:
            pass

    chunks: list[bytes] = []
    received = 0
    while True:
        chunk = response.read(min(1024 * 1024, limit - received + 1))
        if not chunk:
            break
        received += len(chunk)
        if received > limit:
            raise UpdateError("远程文件超过允许大小")
        chunks.append(chunk)
    return b"".join(chunks)


def _fetch_latest_release(opener, timeout: float) -> dict:
    failures: list[str] = []
    for request_url in _request_urls(GITHUB_API_URL):
        try:
            with _open(request_url, opener, timeout) as response:
                payload = json.loads(_read_limited(response, 2 * 1024 * 1024))
            if not isinstance(payload, dict):
                raise UpdateError("更新接口返回格式无效")
            return payload
        except (HTTPError, URLError, TimeoutError, OSError, ValueError,
                json.JSONDecodeError, UpdateError) as error:
            failures.append(str(error))
    raise UpdateError("GitHub 和国内镜像均无法访问") from RuntimeError("; ".join(failures))


def _release_from_payload(payload: dict, current_version: str) -> ReleaseInfo:
    tag_name = str(payload.get("tag_name") or "").strip()
    version = ".".join(str(part) for part in parse_version(tag_name))
    name = str(payload.get("name") or tag_name)
    page_url = f"{GITHUB_RELEASE_URL}/tag/{tag_name}"
    if not is_newer_version(version, current_version):
        return ReleaseInfo(version, tag_name, name, page_url)

    asset_name = f"fenix_to_tfdi-v{version}.zip"
    asset = next(
        (
            candidate for candidate in payload.get("assets") or ()
            if isinstance(candidate, dict) and candidate.get("name") == asset_name
        ),
        None,
    )
    if not asset:
        raise UpdateError(f"最新版本缺少一键更新包: {asset_name}")

    digest = str(asset.get("digest") or "")
    if not digest.lower().startswith("sha256:"):
        raise UpdateError("一键更新包缺少 GitHub SHA-256 校验值")
    sha256 = digest.split(":", 1)[1].lower()
    if not re.fullmatch(r"[0-9a-f]{64}", sha256):
        raise UpdateError("一键更新包 SHA-256 校验值无效")

    asset_url = str(asset.get("browser_download_url") or "")
    expected_url_prefix = f"https://github.com/{REPOSITORY}/releases/download/"
    if not asset_url.startswith(expected_url_prefix):
        raise UpdateError("一键更新包下载地址无效")
    asset_size = int(asset.get("size") or 0)
    if asset_size <= 0 or asset_size > MAX_DOWNLOAD_SIZE:
        raise UpdateError("一键更新包大小无效")

    return ReleaseInfo(
        version, tag_name, name, page_url, asset_name, asset_url, sha256, asset_size
    )


def check_for_update(
    current_version: str = __version__, *, opener=None, timeout: float = 6.0
) -> UpdateCheckResult:
    """检查是否存在带有验证更新包的新稳定版本。"""
    opener = opener or urlopen
    try:
        parse_version(current_version)
        release = _release_from_payload(
            _fetch_latest_release(opener, timeout), current_version
        )
        return UpdateCheckResult(
            current_version, is_newer_version(release.version, current_version), release
        )
    except (ValueError, TypeError, UpdateError) as error:
        return UpdateCheckResult(current_version, False, error=str(error))


def download_update(
    release: ReleaseInfo,
    *,
    opener=None,
    timeout: float = 30.0,
    progress_callback: Callable[[int, int], None] | None = None,
) -> Path:
    """下载并同时校验 GitHub SHA-256 的更新 ZIP。"""
    if not release.asset_url or not release.asset_sha256 or not release.asset_size:
        raise UpdateError("Release 没有可用的一键更新包")
    opener = opener or urlopen
    failures: list[str] = []
    for request_url in _request_urls(release.asset_url):
        package_path: Path | None = None
        try:
            with _open(request_url, opener, timeout) as response:
                handle = tempfile.NamedTemporaryFile(
                    prefix="fenix_to_tfdi_update_", suffix=".zip", delete=False
                )
                package_path = Path(handle.name)
                received = 0
                digest = hashlib.sha256()
                with handle:
                    while chunk := response.read(1024 * 1024):
                        received += len(chunk)
                        if received > MAX_DOWNLOAD_SIZE:
                            raise UpdateError("更新包超过允许大小")
                        handle.write(chunk)
                        digest.update(chunk)
                        if progress_callback:
                            progress_callback(received, release.asset_size)
            if received != release.asset_size:
                raise UpdateError(f"更新包大小不一致: {received}/{release.asset_size}")
            if digest.hexdigest().lower() != release.asset_sha256.lower():
                raise UpdateError("更新包 SHA-256 校验失败")
            validate_update_package(package_path, release.version)
            return package_path
        except (HTTPError, URLError, TimeoutError, OSError, ValueError,
                UpdateError) as error:
            failures.append(str(error))
            if package_path:
                package_path.unlink(missing_ok=True)
    raise UpdateError("GitHub 和国内镜像均无法下载有效更新包") from RuntimeError("; ".join(failures))


def _safe_member_path(name: str) -> PurePosixPath:
    path = PurePosixPath(name)
    if path.is_absolute() or not path.parts or any(
        part in ("", ".", "..") for part in path.parts
    ):
        raise UpdateError(f"更新包包含不安全路径: {name}")
    if path.parts[0] in {".git", "backups", "diagnostics", "output", "target"}:
        raise UpdateError(f"更新包包含禁止路径: {name}")
    return path


def validate_update_package(package_path: Path | str, expected_version: str) -> dict[str, str]:
    """验证 ZIP 路径、清单、版本和每个程序文件的哈希。"""
    try:
        package = zipfile.ZipFile(package_path)
    except (OSError, zipfile.BadZipFile) as error:
        raise UpdateError("更新包不是有效的 ZIP 文件") from error

    with package:
        infos = package.infolist()
        if len(infos) > MAX_PACKAGE_FILES:
            raise UpdateError("更新包文件数量超过限制")
        if sum(info.file_size for info in infos) > MAX_EXTRACTED_SIZE:
            raise UpdateError("更新包解压后超过允许大小")
        names: set[str] = set()
        for info in infos:
            name = _safe_member_path(info.filename).as_posix()
            if name in names:
                raise UpdateError(f"更新包包含重复文件: {name}")
            names.add(name)
            if ((info.external_attr >> 16) & 0o170000) == 0o120000:
                raise UpdateError("更新包不能包含符号链接")
        try:
            manifest = json.loads(package.read(MANIFEST_NAME).decode("utf-8"))
        except (KeyError, UnicodeDecodeError, json.JSONDecodeError) as error:
            raise UpdateError("更新包清单缺失或无效") from error
        if not isinstance(manifest, dict) or not isinstance(manifest.get("files"), dict):
            raise UpdateError("更新包清单格式无效")
        if manifest.get("version") != expected_version:
            raise UpdateError("更新包版本与 Release 不一致")
        files = manifest["files"]
        if not REQUIRED_PROGRAM_FILES.issubset(files):
            raise UpdateError("更新包缺少必要程序文件")

        validated: dict[str, str] = {}
        for name, expected_hash in files.items():
            safe_name = _safe_member_path(str(name)).as_posix()
            if safe_name == MANIFEST_NAME or safe_name not in names:
                raise UpdateError(f"更新包清单引用了缺失文件: {safe_name}")
            if not re.fullmatch(r"[0-9a-fA-F]{64}", str(expected_hash)):
                raise UpdateError(f"更新包文件校验值无效: {safe_name}")
            actual_hash = hashlib.sha256(package.read(safe_name)).hexdigest()
            if actual_hash.lower() != str(expected_hash).lower():
                raise UpdateError(f"更新包文件校验失败: {safe_name}")
            validated[safe_name] = actual_hash

        version_source = package.read("version.py").decode("utf-8")
        match = re.search(r'^__version__\s*=\s*["\']([^"\']+)["\']', version_source, re.MULTILINE)
        if not match or match.group(1) != expected_version:
            raise UpdateError("更新包内部程序版本不一致")
        return validated


def apply_update_package(package_path: Path | str, install_dir: Path | str, expected_version: str) -> Path:
    """替换经验证的程序文件，保留备份，并在失败时回滚。"""
    install_dir = Path(install_dir).resolve()
    files = validate_update_package(package_path, expected_version)
    backup_dir = install_dir / "backups" / f"program_update_{time.strftime('%Y%m%d_%H%M%S')}"
    backup_dir.mkdir(parents=True, exist_ok=False)
    replaced: list[tuple[Path, Path]] = []
    created: list[Path] = []
    try:
        with zipfile.ZipFile(package_path) as package:
            for name in sorted(files):
                relative = Path(*PurePosixPath(name).parts)
                destination = (install_dir / relative).resolve()
                if install_dir not in destination.parents:
                    raise UpdateError(f"安装目标越界: {name}")
                destination.parent.mkdir(parents=True, exist_ok=True)
                if destination.exists():
                    backup = backup_dir / relative
                    backup.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(destination, backup)
                    replaced.append((destination, backup))
                else:
                    created.append(destination)
                temporary = destination.with_name(f"{destination.name}.update-new")
                with package.open(name) as source, open(temporary, "wb") as target:
                    shutil.copyfileobj(source, target)
                os.replace(temporary, destination)
    except Exception:
        for destination in reversed(created):
            destination.unlink(missing_ok=True)
        for destination, backup in reversed(replaced):
            shutil.copy2(backup, destination)
        raise
    return backup_dir


def _wait_for_parent(parent_pid: int, timeout_seconds: int = 60) -> None:
    if parent_pid <= 0:
        return
    if os.name == "nt":
        handle = ctypes.windll.kernel32.OpenProcess(0x00100000, False, parent_pid)
        if handle:
            try:
                ctypes.windll.kernel32.WaitForSingleObject(handle, timeout_seconds * 1000)
            finally:
                ctypes.windll.kernel32.CloseHandle(handle)
        return
    deadline = time.time() + timeout_seconds
    while time.time() < deadline:
        try:
            os.kill(parent_pid, 0)
        except OSError:
            return
        time.sleep(0.2)


def _write_result(success: bool, message: str, version: str) -> Path:
    fd, path = tempfile.mkstemp(prefix="fenix_to_tfdi_update_result_", suffix=".json")
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        json.dump({"success": success, "message": message, "version": version}, handle, ensure_ascii=False)
    return Path(path)


def _start_gui(install_dir: Path, result_path: Path) -> None:
    creationflags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
    subprocess.Popen(
        [sys.executable, str(install_dir / "gui.py"), "--update-result", str(result_path)],
        cwd=install_dir,
        creationflags=creationflags,
        close_fds=True,
    )


def launch_update_installer(package_path: Path | str, install_dir: Path | str, release: ReleaseInfo, parent_pid: int) -> None:
    """启动独立安装器，调用方随后关闭 GUI 释放文件锁。"""
    creationflags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
    subprocess.Popen(
        [
            sys.executable, str(Path(__file__).resolve()), "--apply",
            str(Path(package_path).resolve()), str(Path(install_dir).resolve()),
            release.version, str(parent_pid),
        ],
        cwd=Path(install_dir),
        creationflags=creationflags,
        close_fds=True,
    )


def _installer_main(package_path: Path, install_dir: Path, version: str, parent_pid: int) -> int:
    _wait_for_parent(parent_pid)
    try:
        backup = apply_update_package(package_path, install_dir, version)
        result = _write_result(True, f"已更新到 v{version}\n备份位置: {backup}", version)
        exit_code = 0
    except Exception as error:
        result = _write_result(False, f"自动更新失败: {error}", version)
        exit_code = 1
    finally:
        Path(package_path).unlink(missing_ok=True)
    _start_gui(install_dir, result)
    return exit_code


if __name__ == "__main__":
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("package", nargs="?")
    parser.add_argument("install_dir", nargs="?")
    parser.add_argument("version", nargs="?")
    parser.add_argument("parent_pid", nargs="?", type=int)
    args = parser.parse_args()
    if not args.apply or not all((args.package, args.install_dir, args.version, args.parent_pid)):
        raise SystemExit(2)
    raise SystemExit(_installer_main(Path(args.package), Path(args.install_dir), args.version, args.parent_pid))
