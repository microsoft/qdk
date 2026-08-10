"""Target-conditioned evaluations and backend-bound views onto a qodec.

Exports are loaded lazily so importing the target contracts does not require
optional backend dependencies such as stim, QDK, or deq.
"""

from __future__ import annotations

import importlib
from typing import TYPE_CHECKING, Any

_EXPORTS = {
    "Target": (".base", "Target"),
    "Sampler": (".base", "Sampler"),
    "ComposableTarget": (".base", "ComposableTarget"),
    "CompositeTarget": (".base", "CompositeTarget"),
    "CompositeSampler": (".base", "CompositeSampler"),
    "Batch": (".results", "Batch"),
    "Readouts": (".results", "Readouts"),
    "SoftBatch": (".results", "SoftBatch"),
    "SoftView": (".results", "SoftView"),
    "HeraldedBatch": (".results", "HeraldedBatch"),
    "HeraldedView": (".results", "HeraldedView"),
    "TargetModel": (".model", "TargetModel"),
    "DepolarizingTargetModel": (".model", "DepolarizingTargetModel"),
    "depolarizing": (".model", "depolarizing"),
    "GadgetDistanceData": (".distance", "GadgetDistanceData"),
    "circuit_distance_of": (".distance", "circuit_distance_of"),
    "gadget_distance_bounds_of": (".distance", "gadget_distance_bounds_of"),
    "gadget_distance_of": (".distance", "gadget_distance_of"),
    "build_dem": (".dem", "build_dem"),
    "detector_error_model_of": (".dem", "detector_error_model_of"),
    "StimEmitter": (".stim", "StimEmitter"),
    "StimSampler": (".stim", "StimSampler"),
    "QdkSampler": (".qdk_sim", "QdkSampler"),
    "preselect_on_flags": (".qdk_sim", "preselect_on_flags"),
    "PaulimerSampler": (".paulimer", "PaulimerSampler"),
    "encodable_gates_of": (".qir", "encodable_gates_of"),
    "encode_qir": (".qir", "encode_qir"),
    "run_qir_encoded": (".qir", "run_qir_encoded"),
    "DeqLerTarget": (".deq", "DeqLerTarget"),
    "DeqOptions": (".deq", "DeqOptions"),
    "LerResult": (".deq", "LerResult"),
    "NoiseModel": (".deq", "NoiseModel"),
    "SI1000": (".deq", "SI1000"),
    "Biased": (".deq", "Biased"),
    "RecursiveTarget": (".recursive", "RecursiveTarget"),
    "AssumeViolation": (".universal", "AssumeViolation"),
    "UniversalSampler": (".universal", "UniversalSampler"),
    "UnsupportedFeatureWarning": (".universal", "UnsupportedFeatureWarning"),
}

__all__ = list(_EXPORTS)


def __getattr__(name: str) -> Any:
    try:
        module_name, symbol = _EXPORTS[name]
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
    from .base import (
        ComposableTarget as ComposableTarget,
        CompositeSampler as CompositeSampler,
        CompositeTarget as CompositeTarget,
        Sampler as Sampler,
        Target as Target,
    )
    from .deq import (
        Biased as Biased,
        DeqLerTarget as DeqLerTarget,
        DeqOptions as DeqOptions,
        LerResult as LerResult,
        NoiseModel as NoiseModel,
        SI1000 as SI1000,
    )
    from .dem import (
        build_dem as build_dem,
        detector_error_model_of as detector_error_model_of,
    )
    from .distance import (
        GadgetDistanceData as GadgetDistanceData,
        circuit_distance_of as circuit_distance_of,
        gadget_distance_bounds_of as gadget_distance_bounds_of,
        gadget_distance_of as gadget_distance_of,
    )
    from .model import (
        DepolarizingTargetModel as DepolarizingTargetModel,
        TargetModel as TargetModel,
        depolarizing as depolarizing,
    )
    from .paulimer import PaulimerSampler as PaulimerSampler
    from .qir import (
        encodable_gates_of as encodable_gates_of,
        encode_qir as encode_qir,
        run_qir_encoded as run_qir_encoded,
    )
    from .qdk_sim import (
        QdkSampler as QdkSampler,
        preselect_on_flags as preselect_on_flags,
    )
    from .recursive import RecursiveTarget as RecursiveTarget
    from .results import (
        Batch as Batch,
        HeraldedBatch as HeraldedBatch,
        HeraldedView as HeraldedView,
        Readouts as Readouts,
        SoftBatch as SoftBatch,
        SoftView as SoftView,
    )
    from .stim import StimEmitter as StimEmitter, StimSampler as StimSampler
    from .universal import (
        AssumeViolation as AssumeViolation,
        UniversalSampler as UniversalSampler,
        UnsupportedFeatureWarning as UnsupportedFeatureWarning,
    )
