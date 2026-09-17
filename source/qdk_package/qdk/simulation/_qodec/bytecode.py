from pyqir import Module

from ..._adaptive_pass import AdaptiveProfilePass, AdaptiveProgram, Bytecode


def compile(qir_mod: Module) -> AdaptiveProgram:
    adaptive_pass = AdaptiveProfilePass(Bytecode.Bit64)
    return adaptive_pass.run(qir_mod)
