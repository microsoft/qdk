from ..simulation import run_qir
from .._native import NoiseConfig, compile_stim_to_qir
from typing import List, Literal, Optional, Tuple


def compile(src: str, noise: Optional[NoiseConfig]) -> Tuple[str, NoiseConfig]:
    return compile_stim_to_qir(src, noise)


def run(
    src: str,
    shots: Optional[int] = 1,
    noise: Optional[NoiseConfig] = None,
    seed: Optional[int] = None,
    type: Optional[Literal["clifford", "cpu", "gpu"]] = None,
) -> List:
    qir, noise = compile(src, noise)
    return run_qir(qir, shots, noise, seed, type)
