"""Deterministic completion of draft qodec gadgets."""

from __future__ import annotations

from collections.abc import Mapping, Sequence

import qodec

from ._qodec_compat import set_gadget_readouts
from .checks import profile_of


def _references(values: Sequence[object]) -> list[str]:
    return [str(value) for value in values]


def _readout(
    value: Sequence[object] | Mapping[str, Sequence[object]],
) -> list[str] | dict[str, list[str]]:
    if isinstance(value, Mapping):
        return {name: _references(equation) for name, equation in value.items()}
    return _references(value)


def complete_gadget(gadget: qodec.Gadget) -> qodec.Gadget:
    """Return a copy of ``gadget`` with discovered checks and readouts.

    Pauli-bearing instruction outputs are derived by exact simulation. Flag
    bindings cannot be inferred and are preserved from the draft. The input
    gadget and all objects it references are left unchanged.
    """
    discovered = profile_of(gadget)
    completed = qodec.Gadget(
        gadget.implements,
        gadget.circuit,
        inputs=list(gadget.inputs),
        outputs=list(gadget.outputs),
        checks=discovered.checks,
        readouts=[_readout(value) for value in gadget.readouts],
        parameters=dict(gadget.parameters),
        metadata=dict(gadget.metadata),
    )
    set_gadget_readouts(completed, discovered.observables)
    return completed


def complete_qodec(codec: qodec.Qodec) -> qodec.Qodec:
    """Return a copy of ``codec`` with every gadget completed.

    Applies :func:`complete_gadget` to each gadget of each layer, so the
    returned qodec carries the checks and observable bindings that exact
    simulation can derive. Layers whose gadgets all fail to complete are left
    untouched; a gadget whose circuit cannot be simulated is re-raised with its
    mnemonic attached so the offending draft is easy to find.

    The input qodec and every object it references are left unchanged.
    """
    layers = []
    for index, layer in enumerate(codec.layers):
        completed: list[qodec.Gadget] = []
        for mnemonic, gadget in layer.gadgets.items():
            try:
                completed.append(complete_gadget(gadget))
            except Exception as error:  # noqa: BLE001 - re-raised with context
                raise type(error)(
                    f"layer {index} gadget {mnemonic!r}: {error}"
                ) from error
        layers.append(qodec.Layer(layer.isa, gadgets=completed))
    return qodec.Qodec(
        layers,
        name=codec.name,
        description=codec.description,
        schema_version=codec.schema_version,
        metadata=dict(codec.metadata),
    )


__all__ = ["complete_gadget", "complete_qodec"]
