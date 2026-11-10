# API entry point for Quantum Execution Profiler.
# TODO: finish this class.
from collections.abc import Callable

import pyqir

from qdk import code, openqasm, qsharp
from qdk._adaptive_pass import AdaptiveProfilePass, Bytecode
from qdk._interpreter import _get_context_or_default
from qdk.estimator._estimator import LogicalCounts
from qdk.qre._application import Application
from qdk.qre._qre import Profiler, ProfilerResult
from qdk.qre.application._openqasm import (
    OpenQASMApplication,
    _find_unique_openqasm_name,
)
from qdk.qre.application._qsharp import QSharpApplication


class QREProfiler:
    """Entry point for QRE Profiling. Wrapper around Rust profilers."""

    def __init__(self, profiler: Profiler):
        """Wraps a backend profiler."""
        self.profiler = profiler

    def _profile_qir(self, qir: str) -> ProfilerResult:
        """Profiles QIR from string."""
        module = pyqir.Module.from_ir(pyqir.Context(), qir)
        program = AdaptiveProfilePass(Bytecode.Bit64).run(module)
        return self.profiler.profile_adaptive_program(program.as_dict())

    def _profile_qsharp_entry(
        self,
        entry_expr: str | Callable | LogicalCounts,
        args: tuple,
        use_qir: bool,
    ) -> ProfilerResult:
        """Profiles an expression or imported callable in its owning context."""
        if isinstance(entry_expr, LogicalCounts):
            raise TypeError("Profiling requires executable code, not LogicalCounts.")
        if use_qir:
            return self._profile_qir(str(qsharp.compile(entry_expr, *args)))
        context = _get_context_or_default(entry_expr)
        if isinstance(entry_expr, str):
            return self.profiler.profile_qsharp_expr(
                context._interpreter, entry_expr
            )
        return self.profiler.profile_qsharp(
            context._interpreter,
            getattr(entry_expr, "__global_callable"),
            context._python_args_to_interpreter_args(args),
        )

    def _profile_openqasm_application(
        self, app: OpenQASMApplication, use_qir: bool
    ) -> ProfilerResult:
        """Profiles OpenQASM source or a callable, preserving classical arguments."""
        program = app.program
        if isinstance(program, str):
            if use_qir and not app.args:
                return self._profile_qir(str(openqasm.compile(program)))
            name = _find_unique_openqasm_name()
            openqasm.import_openqasm(
                program, name=name, program_type=openqasm.ProgramType.File
            )
            program = getattr(code.qasm_import, name)
        return self._profile_qsharp_entry(program, app.args, use_qir)

    def profile(self, app: Application, use_qir: bool = True) -> ProfilerResult:
        """Profiles Q# or OpenQASM through QIR by default, or the interpreter."""
        if isinstance(app, QSharpApplication):
            return self._profile_qsharp_entry(app.entry_expr, app.args, use_qir)
        if isinstance(app, OpenQASMApplication):
            return self._profile_openqasm_application(app, use_qir)
        raise TypeError("Unsupported application.")

    def profile_qir_file(self, qir_file: str) -> ProfilerResult:
        """Profiles QIR program from given file."""
        with open(qir_file, "r") as f:
            qir = f.read()
        return self._profile_qir(qir)
