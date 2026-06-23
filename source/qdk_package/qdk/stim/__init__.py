"""EXPERIMENTAL: Stim-to-QIR compilation and simulation.

This module is experimental and its API may change in a future release.
"""

from ..simulation import run_qir
from .._native import NoiseConfig, StimError, compile_stim_to_qir
from typing import List, Literal, Optional, Tuple


def compile(src: str, noise: Optional[NoiseConfig] = None) -> Tuple[str, NoiseConfig]:
    """
    EXPERIMENTAL:

    Compile a Stim program to QIR.
    """
    return compile_stim_to_qir(src, noise)


def run(
    src: str,
    shots: Optional[int] = 1,
    noise: Optional[NoiseConfig] = None,
    seed: Optional[int] = None,
    type: Optional[Literal["clifford", "cpu", "gpu"]] = None,
) -> List:
    """
    EXPERIMENTAL:

    Compile and simulate a Stim program.
    """
    qir, noise = compile(src, noise)
    return run_qir(qir, shots, noise, seed, type)
