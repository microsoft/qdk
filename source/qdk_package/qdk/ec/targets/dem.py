"""Target-conditioned detector error model construction."""

from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING

import qodec as qc
from qodec.circuits import Program

if TYPE_CHECKING:
    import stim


def detector_error_model_of(
    qodec: qc.Qodec,
    program: Program,
    target_model: Mapping[str, float],
    *,
    decompose_errors: bool = False,
) -> "stim.DetectorErrorModel":
    """Build a Stim DEM under the target model's gate-noise assumptions."""
    from .stim import StimEmitter

    return StimEmitter(qodec, noise=dict(target_model)).build_dem(
        program, decompose_errors=decompose_errors
    )


build_dem = detector_error_model_of

__all__ = ["build_dem", "detector_error_model_of"]
