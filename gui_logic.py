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


def _first_file(candidates: tuple[Path, ...]) -> Path | None:
    return next((path for path in candidates if path.is_file()), None)
