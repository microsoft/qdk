"""Deq interchange and decoded execution."""

from __future__ import annotations

import importlib
from typing import TYPE_CHECKING, Any

_EXPORTS = {
    "Biased": (".target", "Biased"),
    "DeqLerTarget": (".target", "DeqLerTarget"),
    "DeqOptions": (".options", "DeqOptions"),
    "LerResult": (".target", "LerResult"),
    "NoiseModel": (".target", "NoiseModel"),
    "SI1000": (".target", "SI1000"),
    "from_deq": (".interchange", "from_deq"),
    "to_deq": (".interchange", "to_deq"),
    "to_deq_source": (".interchange", "to_deq_source"),
    "to_jit_library": (".interchange", "to_jit_library"),
    "to_stim_source": (".interchange", "to_stim_source"),
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
    from .interchange import (
        from_deq as from_deq,
        to_deq as to_deq,
        to_deq_source as to_deq_source,
        to_jit_library as to_jit_library,
        to_stim_source as to_stim_source,
    )
    from .options import DeqOptions as DeqOptions
    from .target import (
        Biased as Biased,
        DeqLerTarget as DeqLerTarget,
        LerResult as LerResult,
        NoiseModel as NoiseModel,
        SI1000 as SI1000,
    )

__all__ = [
    "Biased",
    "DeqLerTarget",
    "DeqOptions",
    "LerResult",
    "NoiseModel",
    "SI1000",
    "from_deq",
    "to_deq",
    "to_deq_source",
    "to_jit_library",
    "to_stim_source",
]
