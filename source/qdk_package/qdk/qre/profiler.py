# API entry point for Quantum Execution Profiler.
# TODO: finish this class.
import pyqir
from qdk import qsharp
from qdk._adaptive_pass import AdaptiveProfilePass, Bytecode
from qdk._interpreter import _get_context_or_default
from qdk.estimator._estimator import LogicalCounts
from qdk.qre._application import Application
from qdk.qre._qre import Profiler, ProfilerResult
from qdk.qre.application._qsharp import QSharpApplication


class QREProfiler:
    """Entry point for QRE Profiling. Wrapper around Rust profilers."""

    def __init__(self, profiler: Profiler):
        self.profiler = profiler

    def _profile_qir(self, qir: str) -> ProfilerResult:
        """Profiles QIR from string."""
        module = pyqir.Module.from_ir(pyqir.Context(), qir)
        program = AdaptiveProfilePass(Bytecode.Bit64).run(module)
        return self.profiler.profile_adaptive_program(program.as_dict())

    def _profile_qsharp_application_using_interpreter(
        self, app: QSharpApplication
    ) -> ProfilerResult:
        """Profiles Q# application using Q# interpreter."""
        assert not isinstance(app.entry_expr, LogicalCounts)
        qir = qsharp.compile(app.entry_expr, *app.args)
        return self._profile_qir(str(qir))

    def _profile_qsharp_application_using_qir(
        self, app: QSharpApplication
    ) -> ProfilerResult:
        """Profiles Q# application by compiling it to QIR."""
        context = _get_context_or_default(app.entry_expr)
        if isinstance(app.entry_expr, str):
            return self.profiler.profile_qsharp_expr(
                context._interpreter, app.entry_expr
            )
        else:
            return self.profiler.profile_qsharp(
                context._interpreter,
                getattr(app.entry_expr, "__global_callable"),
                context._python_args_to_interpreter_args(app.args),
            )

    def profile(self, app: Application, use_qir: bool = True) -> ProfilerResult:
        """Profiles QRE Application."""
        if isinstance(app, QSharpApplication):
            if use_qir:
                return self._profile_qsharp_application_using_qir(app)
            else:
                return self._profile_qsharp_application_using_interpreter(app)
        else:
            raise TypeError("Unsupported application.")

    def profile_qir_file(self, qir_file: str) -> ProfilerResult:
        """Profiles QIR program from given file."""
        with open(qir_file, "r") as f:
            qir = f.read()
        return self._profile_qir(qir)
