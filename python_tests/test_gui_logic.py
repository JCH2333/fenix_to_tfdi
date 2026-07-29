import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from gui_logic import build_conversion_command, detect_paths


class PathDetectionTests(unittest.TestCase):
    def test_detects_converter_inputs_and_tfdi_wasm_template(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            workspace = root / "workspace"
            database = workspace / "nd.db3"
            route_segments = workspace / "2607" / "RTE_SEG.csv"
            template = (
                root
                / "AppData"
                / "Microsoft Flight Simulator 2024"
                / "WASM"
                / "MSFS2024"
                / "tfdidesign-aircraft-md11"
                / "work"
                / "Nav-Primary"
            )
            route_segments.parent.mkdir(parents=True)
            template.mkdir(parents=True)
            database.touch()
            route_segments.touch()

            with patch.dict(
                os.environ,
                {"APPDATA": str(root / "AppData")},
                clear=False,
            ):
                detected = detect_paths(workspace)

        self.assertEqual(detected.database, database)
        self.assertEqual(detected.route_segments, route_segments)
        self.assertEqual(detected.reference, template)


class ConversionCommandTests(unittest.TestCase):
    def test_builds_explicit_isolated_conversion_command(self):
        command = build_conversion_command(
            Path("fenix_to_tfdi.exe"),
            Path("input/nd.db3"),
            Path("input/RTE_SEG.csv"),
            Path("official/Nav-Primary"),
            Path("output/2607-test"),
        )

        self.assertEqual(
            command,
            [
                "fenix_to_tfdi.exe",
                "--db",
                os.fspath(Path("input/nd.db3")),
                "--rte-seg",
                os.fspath(Path("input/RTE_SEG.csv")),
                "--reference",
                os.fspath(Path("official/Nav-Primary")),
                "--output",
                os.fspath(Path("output/2607-test")),
            ],
        )


if __name__ == "__main__":
    unittest.main()
