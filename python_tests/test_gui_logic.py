import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from gui_logic import (
    build_conversion_command,
    detect_paths,
    find_converter_executable,
    validate_conversion_paths,
)


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

    def test_rejects_an_existing_output_directory(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            database = root / "nd.db3"
            route_segments = root / "RTE_SEG.csv"
            reference = root / "reference"
            output = root / "existing-output"
            database.touch()
            route_segments.touch()
            reference.mkdir()
            output.mkdir()

            with self.assertRaisesRegex(ValueError, "输出目录已存在"):
                validate_conversion_paths(
                    database, route_segments, reference, output
                )

    def test_prefers_converter_next_to_gui(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            app_dir = Path(temp_dir)
            bundled = app_dir / "fenix_to_tfdi.exe"
            cargo_release = app_dir / "target" / "release" / "fenix_to_tfdi.exe"
            cargo_release.parent.mkdir(parents=True)
            bundled.touch()
            cargo_release.touch()

            executable = find_converter_executable(app_dir)

        self.assertEqual(executable, bundled)

    def test_rejects_missing_fenix_database(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)

            with self.assertRaisesRegex(ValueError, "Fenix nd.db3 不存在"):
                validate_conversion_paths(
                    root / "missing.db3",
                    root / "RTE_SEG.csv",
                    root / "reference",
                    root / "output",
                )

    def test_rejects_missing_route_segments(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            database = root / "nd.db3"
            database.touch()

            with self.assertRaisesRegex(ValueError, "RTE_SEG.csv 不存在"):
                validate_conversion_paths(
                    database,
                    root / "missing.csv",
                    root / "reference",
                    root / "output",
                )


if __name__ == "__main__":
    unittest.main()
