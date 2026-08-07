"""构建 GitHub Release 使用的一键更新 ZIP。"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import zipfile

from version import __version__


ROOT = Path(__file__).resolve().parent
PACKAGE_SOURCE_FILES = (
    "gui.py",
    "gui_logic.py",
    "run_gui.bat",
    "update_manager.py",
    "version.py",
)


def build_package(
    output_dir: Path | str,
    converter_executable: Path | str,
    version: str = __version__,
) -> Path:
    """创建仅包含可更新程序文件和哈希清单的 ZIP。"""
    if version != __version__:
        raise ValueError(
            f"更新包版本 {version} 与 version.py 的 {__version__} 不一致"
        )
    executable = Path(converter_executable)
    if not executable.is_file():
        raise FileNotFoundError(f"未找到已构建的 fenix_to_tfdi.exe: {executable}")

    files = {name: ROOT / name for name in PACKAGE_SOURCE_FILES}
    missing = [name for name, path in files.items() if not path.is_file()]
    if missing:
        raise FileNotFoundError(f"缺少更新包源文件: {', '.join(missing)}")
    files["fenix_to_tfdi.exe"] = executable

    output = Path(output_dir)
    output.mkdir(parents=True, exist_ok=True)
    target = output / f"fenix_to_tfdi-v{version}.zip"
    hashes = {
        name: hashlib.sha256(path.read_bytes()).hexdigest()
        for name, path in files.items()
    }
    manifest = json.dumps(
        {"version": version, "files": hashes},
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")

    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as package:
        for name in sorted(files):
            package.write(files[name], name)
        package.writestr("update-manifest.json", manifest)
    return target


def main() -> None:
    parser = argparse.ArgumentParser(description="构建 Fenix -> TFDI 一键更新包")
    parser.add_argument("--exe", required=True, type=Path, help="已构建的 fenix_to_tfdi.exe")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "external-test-packages")
    parser.add_argument("--version", default=__version__)
    args = parser.parse_args()
    target = build_package(args.output_dir, args.exe, args.version)
    print(target)
    print(f"SHA-256: {hashlib.sha256(target.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()
