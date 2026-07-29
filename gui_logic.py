"""GUI 使用的可测试业务逻辑。"""

from dataclasses import dataclass
import os
from pathlib import Path


@dataclass(frozen=True)
class DetectedPaths:
    database: Path | None
    route_segments: Path | None
    reference: Path | None


def detect_paths(search_root: Path | str) -> DetectedPaths:
    """检测转换所需输入和 TFDI 官方模板。"""
    root = Path(search_root)
    database = _first_file((root / "nd.db3", root.parent / "nd.db3"))
    route_segments = _first_file(
        (
            root / "2607" / "RTE_SEG.csv",
            root / "RTE_SEG.csv",
            root.parent / "2607" / "RTE_SEG.csv",
            root.parent / "RTE_SEG.csv",
        )
    )

    appdata = os.environ.get("APPDATA")
    reference = None
    if appdata:
        candidate = (
            Path(appdata)
            / "Microsoft Flight Simulator 2024"
            / "WASM"
            / "MSFS2024"
            / "tfdidesign-aircraft-md11"
            / "work"
            / "Nav-Primary"
        )
        if candidate.is_dir():
            reference = candidate

    return DetectedPaths(database, route_segments, reference)


def build_conversion_command(
    executable: Path | str,
    database: Path | str,
    route_segments: Path | str,
    reference: Path | str,
    output: Path | str,
) -> list[str]:
    """构造显式输入、隔离输出的转换命令。"""
    return [
        str(executable),
        "--db",
        str(database),
        "--rte-seg",
        str(route_segments),
        "--reference",
        str(reference),
        "--output",
        str(output),
    ]


def validate_conversion_paths(
    database: Path | str,
    route_segments: Path | str,
    reference: Path | str,
    output: Path | str,
) -> None:
    """验证 GUI 转换路径不会覆盖已有目录。"""
    del reference
    if not Path(database).is_file():
        raise ValueError(f"Fenix nd.db3 不存在：\n{database}")
    if not Path(route_segments).is_file():
        raise ValueError(f"RTE_SEG.csv 不存在：\n{route_segments}")
    if Path(output).exists():
        raise ValueError(f"输出目录已存在，请选择新的目录：\n{output}")


def find_converter_executable(app_dir: Path | str) -> Path | None:
    """查找随 GUI 分发或由 Cargo 构建的转换程序。"""
    root = Path(app_dir)
    candidates = (
        root / "fenix_to_tfdi.exe",
        root / "target" / "release" / "fenix_to_tfdi.exe",
    )
    return _first_file(candidates)


def _first_file(candidates: tuple[Path, ...]) -> Path | None:
    return next((path for path in candidates if path.is_file()), None)
