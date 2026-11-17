# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from collections.abc import Callable, Sequence
from contextlib import ExitStack
from dataclasses import dataclass, field
import os
from pathlib import Path
import shlex
import subprocess
import sys
from tempfile import TemporaryDirectory
from typing import TypeVar

from qdk.qre._application import Application
from qdk.qre._qre import Trace, trace_fds

_Result = TypeVar("_Result")


@dataclass
class QspApplication(Application[None]):
    """Quantum program written in compiled language using QSP function calls.

    Supports C and C++ in a QDK source checkout on Linux and macOS.

    Args:
        source_path: Main source file.
        extra_sources: Additional source files to compile and link.
        args: Command-line arguments passed to the executable.
        compiler_args: Extra compiler/linker arguments, appended after the archive
            and system libraries. Use these for include paths, defines, and libraries.
    """

    source_path: str | Path
    extra_sources: Sequence[str | Path] = ()
    args: Sequence[str] = ()
    compiler_args: Sequence[str] = ()
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
        """Compile sources against the release interop archive and return its executable.

        Requires ``cargo build --release -p qspnlib`` in the checkout.
        A ``.c`` main source uses ``CC`` (default ``cc``) and C11; other sources
        use ``CXX`` (default ``c++``) and C++17. Relative source paths are resolved
        from the current working directory. Compiler failures propagate as
        ``subprocess.CalledProcessError``, with diagnostics on stderr.
        """
        if sys.platform not in {"linux", "darwin"}:
            raise NotImplementedError("QSP compilation supports Linux and macOS only.")
        source = Path(self.source_path).resolve(strict=True)
        extra_sources = [Path(path).resolve(strict=True) for path in self.extra_sources]
        repository = Path(__file__).resolve().parents[5]
        include = repository / "source" / "qspnlib" / "include"
        library = repository / "target" / "release" / "libqspnlib.a"
        if not library.is_file() or not (include / "qspnlib.h").is_file():
            raise FileNotFoundError(
                "QSP compilation requires a QDK source checkout and the release "
                "interop archive. Run `cargo build --release -p qspnlib` "
                "from the repository root."
            )
        executable = source.with_name(f"{source.stem}.exe")
        libraries = ["-ldl", "-lpthread", "-lm"] if sys.platform == "linux" else []
        is_c = source.suffix == ".c"
        compiler = os.environ.get("CC", "cc") if is_c else os.environ.get("CXX", "c++")
        subprocess.run(
            [
                *shlex.split(compiler),
                "-std=c11" if is_c else "-std=c++17",
                "-O2",
                "-I",
                str(include),
                str(source),
                *(str(path) for path in extra_sources),
                str(library),
                *libraries,
                *self.compiler_args,
                "-o",
                str(executable),
            ],
            check=True,
        )
        return executable

    def _run_qsp_application(self, serve: Callable[[int, int], _Result]) -> _Result:
        """Compile and run the program, serving dedicated pipes and checking its exit.

        Args:
            serve: Synchronous callback receiving the instruction-read descriptor
                and result-write descriptor, in that order. It must read QSP
                batches and write and flush their replies until instruction EOF.
                Descriptors are borrowed and must not be closed by the callback.

        Returns:
            The callback's result after the program exits successfully.

        On Linux, the calling thread and child share one CPU during execution;
        the calling thread's original affinity is restored afterward.
        Callback errors propagate after the child process is stopped.
        """
        if os.name != "posix":
            raise NotImplementedError("QSP execution requires POSIX file descriptors.")
        compiled_exe = self.compile()
        with ExitStack() as pipes:
            if sys.platform == "linux":
                original_affinity = os.sched_getaffinity(0)
                # Avoid cross-core overhead in synchronous pipe exchanges; the child inherits affinity.
                os.sched_setaffinity(0, {min(original_affinity)})
                pipes.callback(os.sched_setaffinity, 0, original_affinity)
            instruction_read, instruction_write = os.pipe()
            instructions = pipes.enter_context(
                os.fdopen(instruction_read, "rb", buffering=0)
            )
            child_instructions = pipes.enter_context(
                os.fdopen(instruction_write, "wb", buffering=0)
            )
            result_read, result_write = os.pipe()
            child_results = pipes.enter_context(
                os.fdopen(result_read, "rb", buffering=0)
            )
            results = pipes.enter_context(os.fdopen(result_write, "wb", buffering=0))
            with subprocess.Popen(
                [str(compiled_exe), *self.args],
                pass_fds=(child_instructions.fileno(), child_results.fileno()),
                env={
                    **os.environ,
                    "QDK_INSTRUCTION_FD": str(child_instructions.fileno()),
                    "QDK_RESULT_FD": str(child_results.fileno()),
                },
            ) as program:
                child_instructions.close()
                child_results.close()
                try:
                    result = serve(instructions.fileno(), results.fileno())
                    returncode = program.wait()
                    if returncode != 0:
                        raise subprocess.CalledProcessError(returncode, program.args)
                    return result
                finally:
                    if program.poll() is None:
                        program.terminate()
                        try:
                            program.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            program.kill()
                            program.wait()

    def get_trace(self, parameters: None = None) -> Trace:
        """Run the program and collect its gates in a fresh trace.

        Measurements return random outcomes; only the executed path is traced.
        Compilation, protocol, and subprocess failures propagate to the caller.
        """
        return self._run_qsp_application(trace_fds)
