# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from dataclasses import dataclass, field
import os
from pathlib import Path
import shlex
import subprocess
import sys
from tempfile import TemporaryDirectory

from qdk.qre._application import Application
from qdk.qre._qre import Trace


@dataclass
class QspApplication(Application[None]):
    """Quantum program written in compiled language using QSP function calls.

    Currently supports C++ in a QDK source checkout on Linux and macOS.
    """

    source_path: str | Path  # C++ source.
    _source_directory: TemporaryDirectory[str] | None = field(
        default=None, init=False, repr=False, compare=False
    )

    @staticmethod
    def for_cpp_code(cpp_code: str) -> QspApplication:
        """Save UTF-8 C++ source in a temporary directory owned by the application.

        Source and compiled executable remain available for the application's
        lifetime and are removed when its temporary directory is cleaned up.
        """
        directory = TemporaryDirectory(prefix="qdk-qsp-")
        source = Path(directory.name) / "program.cpp"
        source.write_text(cpp_code, encoding="utf-8")
        application = QspApplication(source)
        application._source_directory = directory
        return application

    def compile(self) -> Path:
        """Compile C++ against the release interop archive and return its executable.

        Requires ``cargo build --release -p qep_interop`` in the checkout.
        Uses ``CXX`` when set, otherwise ``c++``. Compiler failures propagate as
        ``subprocess.CalledProcessError``, with diagnostics on stderr.
        """
        if sys.platform not in {"linux", "darwin"}:
            raise NotImplementedError("QSP compilation supports Linux and macOS only.")
        source = Path(self.source_path).resolve(strict=True)
        repository = Path(__file__).resolve().parents[5]
        include = repository / "source" / "qep_interop" / "include"
        library = repository / "target" / "release" / "libqep_interop.a"
        if not library.is_file() or not (include / "qep_interop.h").is_file():
            raise FileNotFoundError(
                "QSP compilation requires a QDK source checkout and the release "
                "interop archive. Run `cargo build --release -p qep_interop` "
                "from the repository root."
            )
        executable = source.with_name(f"{source.stem}_qsp")
        libraries = ["-ldl", "-lpthread", "-lm"] if sys.platform == "linux" else []
        subprocess.run(
            [
                *shlex.split(os.environ.get("CXX", "c++")),
                "-std=c++17",
                "-O2",
                "-I",
                str(include),
                str(source),
                str(library),
                *libraries,
                "-o",
                str(executable),
            ],
            check=True,
        )
        return executable

    def get_trace(self, parameters: None = None) -> Trace:
        """Reject trace generation; compiled QSP applications use live profiling."""
        raise NotImplementedError("Use QREProfiler.profile() for QSP applications.")
