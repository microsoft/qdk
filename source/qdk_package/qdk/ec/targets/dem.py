"""Target-conditioned detector error model construction."""

from __future__ import annotations

from collections.abc import Mapping

import qodec
from qodec.circuits import Program


def detector_error_model_of(
    codec: qodec.Qodec,
    program: Program,
    target_model: Mapping[str, float],
    *,
    decompose_errors: bool = False,
) -> object:
    """Build a Stim DEM under the target model's gate-noise assumptions."""
    from .stim import StimEmitter

    return StimEmitter(codec, noise=dict(target_model)).build_dem(
        program, decompose_errors=decompose_errors
    )


build_dem = detector_error_model_of

__all__ = ["build_dem", "detector_error_model_of"]
