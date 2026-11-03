# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import contextlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "circuit_capi_build", Path(__file__).resolve().parents[1] / "build.py"
)
assert SPEC is not None and SPEC.loader is not None
build = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(build)


class HeaderTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.generated = self.root / "generated.h"
        self.reference = self.root / "includes" / "qdk_circuit.h"
        self.generated.write_bytes(b"/* generated */\n")

    def test_matching_header_is_not_changed(self) -> None:
        build.update_header(self.generated, self.reference)
        before = self.reference.stat().st_mtime_ns

        build.check_header(self.generated, self.reference)

        self.assertEqual(self.reference.stat().st_mtime_ns, before)

    def test_missing_header_fails_without_creating_it(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "Missing tracked header"):
            build.check_header(self.generated, self.reference)

        self.assertFalse(self.reference.exists())

    def test_stale_header_fails_without_changing_it(self) -> None:
        self.reference.parent.mkdir()
        self.reference.write_bytes(b"stale\n")
        before = self.reference.stat().st_mtime_ns

        with self.assertRaisesRegex(RuntimeError, "Stale tracked header"):
            build.check_header(self.generated, self.reference)

        self.assertEqual(self.reference.read_bytes(), b"stale\n")
        self.assertEqual(self.reference.stat().st_mtime_ns, before)

    def test_explicit_update_copies_exact_bytes(self) -> None:
        build.update_header(self.generated, self.reference)

        self.assertEqual(self.reference.read_bytes(), self.generated.read_bytes())

    def test_update_conflicts_are_rejected_during_argument_parsing(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as check:
                build.parse_args(["--update-header", "--check-header"])
            with self.assertRaises(SystemExit) as native:
                build.parse_args(["--update-header", "--test"])
            with self.assertRaises(SystemExit) as fortran:
                build.parse_args(["--update-header", "--test-fortran"])

        self.assertEqual(
            (check.exception.code, native.exception.code, fortran.exception.code),
            (2, 2, 2),
        )

    def test_explicit_missing_fortran_compiler_fails_without_fallback(self) -> None:
        missing = self.root / "missing compiler"

        with self.assertRaisesRegex(RuntimeError, "Required tool is missing"):
            build.fortran_compiler(f'"{missing}"')


class SdkStagingTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.header = self.root / "qdk_circuit.h"
        self.library = self.root / "libqdk_circuit_capi.so"
        self.header.write_bytes(b"/* generated header */\n")
        self.library.write_bytes(b"native library")
        self.sdk = self.root / "SDK with spaces"

    def test_header_and_library_are_staged_together(self) -> None:
        include, library = build.stage_sdk(self.sdk, self.header, self.library)

        self.assertEqual(
            (include / self.header.name).read_bytes(), self.header.read_bytes()
        )
        self.assertEqual(library, self.sdk / "lib" / self.library.name)
        self.assertEqual(library.read_bytes(), self.library.read_bytes())

    def test_go_module_and_license_are_staged(self) -> None:
        build.stage_sdk(self.sdk, self.header, self.library)
        source = build.SOURCE / "bindings" / "go"
        staged = self.sdk / "bindings" / "go"

        self.assertEqual(
            (staged / "go.mod").read_bytes(), (source / "go.mod").read_bytes()
        )
        self.assertEqual(
            (staged / "shim.go").read_bytes(), (source / "shim.go").read_bytes()
        )
        self.assertEqual(
            (staged / "LICENSE.txt").read_bytes(),
            (build.ROOT / "LICENSE.txt").read_bytes(),
        )

    def test_rebuilding_refreshes_the_staged_library(self) -> None:
        build.stage_sdk(self.sdk, self.header, self.library)
        self.library.write_bytes(b"rebuilt library")

        _, library = build.stage_sdk(self.sdk, self.header, self.library)

        self.assertEqual(library.read_bytes(), b"rebuilt library")

    def test_fortran_sources_and_license_are_staged(self) -> None:
        build.stage_sdk(self.sdk, self.header, self.library)
        source = build.SOURCE / "bindings" / "fortran"
        staged = self.sdk / "bindings" / "fortran"

        self.assertEqual(
            (staged / "qdk_circuit.f90").read_bytes(),
            (source / "qdk_circuit.f90").read_bytes(),
        )
        self.assertEqual(
            (staged / "bridge.c").read_bytes(), (source / "bridge.c").read_bytes()
        )
        self.assertEqual(
            (staged / "LICENSE.txt").read_bytes(),
            (build.ROOT / "LICENSE.txt").read_bytes(),
        )

    def test_compiler_specific_fortran_products_are_not_staged(self) -> None:
        source = self.root / "source"
        fortran = source / "bindings" / "fortran"
        go = source / "bindings" / "go"
        fortran.mkdir(parents=True)
        go.mkdir()
        (go / "go.mod").write_text("module example\n", encoding="utf-8")
        (fortran / "qdk_circuit.f90").write_text(
            "module qdk_circuit\nend module\n", encoding="utf-8"
        )
        (fortran / "qdk_circuit.mod").write_bytes(b"compiler-specific module")
        (fortran / "qdk_circuit.smod").write_bytes(b"compiler-specific submodule")
        (fortran / "bridge.o").write_bytes(b"compiler-specific object")

        with patch.object(build, "SOURCE", source):
            build.stage_sdk(self.sdk, self.header, self.library)

        self.assertEqual(
            sorted(path.name for path in (self.sdk / "bindings" / "fortran").iterdir()),
            ["LICENSE.txt", "qdk_circuit.f90"],
        )
        self.assertTrue((self.sdk / "bindings" / "go" / "go.mod").is_file())


if __name__ == "__main__":
    unittest.main()
