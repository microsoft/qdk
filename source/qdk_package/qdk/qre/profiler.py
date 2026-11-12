# API entry point for Quantum Execution Profiler.
# TODO: finish this class.
from collections.abc import Callable

import pyqir

from qdk import code, openqasm, qsharp
from qdk._adaptive_pass import AdaptiveProfilePass, Bytecode
from qdk._interpreter import _get_context_or_default
from qdk.estimator._estimator import LogicalCounts
from qdk.qre._application import Application
from qdk.qre._qre import Profiler, ProfilerResult, SequentialCountingProfiler
from qdk.qre.application._openqasm import (
    OpenQASMApplication,
    _find_unique_openqasm_name,
)
from qdk.qre.application._qir import QIRApplication
from qdk.qre.application._qsharp import QSharpApplication


class QREProfiler:
    """Entry point for QRE Profiling. Wrapper around Rust profilers."""

    def __init__(self, profiler: Profiler):
        """Wraps a backend profiler."""
        self.profiler = profiler

    @staticmethod
    def counting() -> "QREProfiler":
        """Returns profiler that counts instructions."""
        gate_names = (  # noqa: SIM905
            "ccx cx cy cz rx rxx ry ryy rz rzz h s sadj sx t tadj x y z swap "
            "reset mz_store mz_store_reset read_result mz mz_reset"
        ).split()
        profiler = SequentialCountingProfiler(
            physical_qubits_per_logical=1,
            gate_time_sec={name: 0.0 for name in gate_names},
        )
        return QREProfiler(profiler)

    def _profile_qir(self, qir: str | bytes) -> ProfilerResult:
        """Profiles QIR supplied as LLVM text or binary bitcode."""
        context = pyqir.Context()
        if isinstance(qir, str):
            module = pyqir.Module.from_ir(context, qir)
        else:
            module = pyqir.Module.from_bitcode(context, qir)
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
            return self.profiler.profile_qsharp_expr(context._interpreter, entry_expr)
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
        """Profiles Q# or OpenQASM through QIR or the interpreter; QIR uses bytecode."""
        if isinstance(app, QSharpApplication):
            return self._profile_qsharp_entry(app.entry_expr, app.args, use_qir)
        if isinstance(app, OpenQASMApplication):
            return self._profile_openqasm_application(app, use_qir)
        if isinstance(app, QIRApplication):
            return self._profile_qir(app.input)
        raise TypeError("Unsupported application.")
