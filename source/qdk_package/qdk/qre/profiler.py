# API entry point for Quantum Execution Profiler.
# TODO: finish this class.
import pyqir
from qdk._adaptive_pass import AdaptiveProfilePass, Bytecode
from qdk.qre._qre import Profiler, ProfilerResult


class QREProfiler:
    def __init__(self):
        pass

    # Temporary function to profile QIR directly (from file).
    @staticmethod
    def profile_qir(qir_file: str, profiler: Profiler) -> ProfilerResult:
        with open(qir_file, "r") as f:
            qir = f.read()
        module = pyqir.Module.from_ir(pyqir.Context(), qir)
        program = AdaptiveProfilePass(Bytecode.Bit64).run(module)
        return profiler.profile_adaptive_program(program.as_dict())
