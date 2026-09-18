"""Develop and test quantum error-correction schemes described by qodecs.

The ``qodec`` package owns the data model and persistence. This module derives
facts by exact simulation, builds a qodec from a code, and audits complete
qodecs. Its public API is intentionally flat and small.

There is no qodec-wide profile. A qodec is a stack of lowering layers, so its
action depends on the program lowered through it. Use :class:`GadgetProfile` to
ask what one gadget or circuit does, and :func:`audit` to ask whether a complete
qodec is internally consistent.

Install the optional dependencies with ``pip install "qdk[ec]"``.
"""

from __future__ import annotations

from importlib import import_module
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    # Imported eagerly only for type checkers and editors; at runtime the names
    # below are resolved lazily, so `import qdk.ec` does not pull in paulimer,
    # highspy and binar for a one-line call.
    from ._analysis.channel_action import ChannelAction
    from ._analysis.propagation.pauli import Pauli
    from ._audit._auditor import audit
    from ._audit._diagnostic import Diagnostic
    from ._audit._report import Report
    from ._code_profile import CodeProfile
    from ._distance_result import Distance
    from ._completion import derive
    from ._faults import FaultEffect, FaultEvent
    from ._profile import GadgetProfile
    from ._build import build_qodec

__all__ = [
    "ChannelAction",
    "CodeProfile",
    "Diagnostic",
    "Distance",
    "FaultEffect",
    "FaultEvent",
    "GadgetProfile",
    "Pauli",
    "Report",
    "audit",
    "build_qodec",
    "derive",
]

_EXPORTS = {
    "ChannelAction": ("._analysis.channel_action", "ChannelAction"),
    "CodeProfile": ("._code_profile", "CodeProfile"),
    "Diagnostic": ("._audit._diagnostic", "Diagnostic"),
    "Distance": ("._distance_result", "Distance"),
    "FaultEffect": ("._faults", "FaultEffect"),
    "FaultEvent": ("._faults", "FaultEvent"),
    "GadgetProfile": ("._profile", "GadgetProfile"),
    "Pauli": ("._analysis.propagation.pauli", "Pauli"),
    "Report": ("._audit._report", "Report"),
    "audit": ("._audit._auditor", "audit"),
    "build_qodec": ("._build", "build_qodec"),
    "derive": ("._completion", "derive"),
}


def __getattr__(name: str) -> Any:
    try:
        module_name, attribute = _EXPORTS[name]
    except KeyError as error:
        raise AttributeError(
            f"module {__name__!r} has no attribute {name!r}"
        ) from error
    try:
        value = getattr(import_module(module_name, __name__), attribute)
    except ModuleNotFoundError as error:
        if error.name in {"binar", "highspy", "more_itertools", "paulimer", "qodec"}:
            raise ModuleNotFoundError(
                f"qdk.ec requires optional dependencies; install them with "
                f"'pip install \"qdk[ec]\"' (missing {error.name!r})"
            ) from error
        raise
    globals()[name] = value
    return value


def __dir__() -> list[str]:
    return sorted(__all__)
