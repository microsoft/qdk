"""Conversion between qodec objects and deq artifacts."""

from __future__ import annotations

import importlib
from typing import TYPE_CHECKING, Any

_EXPORTS = {
    "from_deq": (".qodec_builder", "from_deq"),
    "to_deq": (".source_emitter", "to_deq_source"),
    "to_deq_source": (".source_emitter", "to_deq_source"),
    "to_jit_library": (".library", "to_jit_library"),
    "to_stim_source": (".library", "to_stim_source"),
}

__all__ = list(_EXPORTS)


def __getattr__(name: str) -> Any:
    try:
        module_name, symbol = _EXPORTS[name]
    except KeyError as error:
        raise AttributeError(
            f"module {__name__!r} has no attribute {name!r}"
        ) from error
    module = importlib.import_module(module_name, __package__)
    value = getattr(module, symbol)
    globals()[name] = value
    return value


def __dir__() -> list[str]:
    return sorted(__all__)


if TYPE_CHECKING:
    from .library import (
        to_jit_library as to_jit_library,
        to_stim_source as to_stim_source,
    )
    from .qodec_builder import from_deq as from_deq
    from .source_emitter import to_deq_source as to_deq_source

    to_deq = to_deq_source
