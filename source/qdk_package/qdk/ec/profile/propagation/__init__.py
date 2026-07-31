"""Exact, noiseless semantic propagation over qodec programs."""

from __future__ import annotations

import importlib
from typing import Any

from qodec.circuits import Program

_EXPORTS = {
    "ChannelSimulation": ("qdk.ec.profile.check_discovery", "ChannelSimulation"),
    "ProgramSimulation": ("qdk.ec.profile.check_discovery", "ProgramSimulation"),
    "simulate_channel": ("qdk.ec.profile.check_discovery", "simulate_channel"),
    "simulate_program": ("qdk.ec.profile.check_discovery", "simulate_program"),
    "ConditionalChoiResult": (".conditional", "ConditionalChoiResult"),
    "conditional_choi_state": (".conditional", "conditional_choi_state"),
    "FrameGroup": (".frames", "FrameGroup"),
    "PauliFrame": (".frames", "PauliFrame"),
    "evolution_of": (".stabilizer", "evolution_of"),
    "frame_group_of": (".stabilizer", "frame_group_of"),
    "stabilizer_group_of": (".stabilizer", "stabilizer_group_of"),
}

__all__ = ["Program", *_EXPORTS]


def __getattr__(name: str) -> Any:
    try:
        module_name, symbol = _EXPORTS[name]
    except KeyError as error:
        raise AttributeError(
            f"module {__name__!r} has no attribute {name!r}"
        ) from error
    module = (
        importlib.import_module(module_name, __name__)
        if module_name.startswith(".")
        else importlib.import_module(module_name)
    )
    value = getattr(module, symbol)
    globals()[name] = value
    return value


def __dir__() -> list[str]:
    return sorted(__all__)
