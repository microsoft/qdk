"""Target-conditioned evaluations and backend-bound views onto a qodec.

Everything here is imported normally except the exports whose module needs an
optional backend: ``stim``, ``qdk_sim`` and ``recursive`` (all stim), and the
``deq`` symbols. Those stay behind :func:`__getattr__` so importing the target
contracts does not require a simulator or decoder toolchain to be installed.
"""

from __future__ import annotations

import importlib
from typing import TYPE_CHECKING, Any

from .base import (
    ComposableTarget,
    CompositeSampler,
    CompositeTarget,
    Sampler,
    Target,
)
from .dem import build_dem, detector_error_model_of
from .distance import (
    GadgetDistanceData,
    circuit_distance_of,
    gadget_distance_bounds_of,
    gadget_distance_of,
)
from .model import DepolarizingTargetModel, TargetModel, depolarizing
from .paulimer import PaulimerSampler
from .qir import encodable_gates_of, encode_qir, run_qir_encoded
from .results import (
    AnnotatedBatch,
    Batch,
    Readouts,
    leaks_of,
    probabilities_of,
)
from .universal import AssumeViolation, UniversalSampler, UnsupportedFeatureWarning

#: Exports whose module needs an optional backend, so cannot be imported eagerly.
_LAZY_EXPORTS = {
    "StimEmitter": (".stim", "StimEmitter"),
    "StimSampler": (".stim", "StimSampler"),
    "QdkSampler": (".qdk_sim", "QdkSampler"),
    "preselect_on_flags": (".qdk_sim", "preselect_on_flags"),
    "RecursiveTarget": (".recursive", "RecursiveTarget"),
    "Biased": (".deq", "Biased"),
    "DeqLerTarget": (".deq", "DeqLerTarget"),
    "DeqOptions": (".deq", "DeqOptions"),
    "LerResult": (".deq", "LerResult"),
    "NoiseModel": (".deq", "NoiseModel"),
    "SI1000": (".deq", "SI1000"),
}

__all__ = [
    "AnnotatedBatch",
    "AssumeViolation",
    "Batch",
    "ComposableTarget",
    "CompositeSampler",
    "CompositeTarget",
    "DepolarizingTargetModel",
    "GadgetDistanceData",
    "PaulimerSampler",
    "Readouts",
    "Sampler",
    "Target",
    "TargetModel",
    "UniversalSampler",
    "UnsupportedFeatureWarning",
    "build_dem",
    "circuit_distance_of",
    "depolarizing",
    "detector_error_model_of",
    "encodable_gates_of",
    "encode_qir",
    "gadget_distance_bounds_of",
    "gadget_distance_of",
    "leaks_of",
    "probabilities_of",
    "run_qir_encoded",
    *_LAZY_EXPORTS,
]


def __getattr__(name: str) -> Any:
    try:
        module_name, symbol = _LAZY_EXPORTS[name]
    except KeyError as error:
        raise AttributeError(
            f"module {__name__!r} has no attribute {name!r}"
        ) from error
    module = importlib.import_module(module_name, __name__)
    value = getattr(module, symbol)
    globals()[name] = value
    return value


def __dir__() -> list[str]:
    return sorted(__all__)


if TYPE_CHECKING:
    from .deq import (
        Biased as Biased,
        DeqLerTarget as DeqLerTarget,
        DeqOptions as DeqOptions,
        LerResult as LerResult,
        NoiseModel as NoiseModel,
        SI1000 as SI1000,
    )
    from .qdk_sim import (
        QdkSampler as QdkSampler,
        preselect_on_flags as preselect_on_flags,
    )
    from .recursive import RecursiveTarget as RecursiveTarget
    from .stim import StimEmitter as StimEmitter, StimSampler as StimSampler
