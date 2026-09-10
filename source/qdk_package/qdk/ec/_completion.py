"""Deterministic completion of draft qodec gadgets."""

from __future__ import annotations

import qodec as qc

from ._readouts import as_readout, set_gadget_readouts
from ._references import as_references
from ._checks import profile_of


def complete_gadget(gadget: qc.Gadget) -> qc.Gadget:
    """Return a copy of ``gadget`` with discovered checks and readouts.

    Pauli-bearing instruction outputs are derived by exact simulation. Flag
    bindings cannot be inferred and are preserved from the draft. The input
    gadget and all objects it references are left unchanged.
    """
    discovered = profile_of(gadget)
    completed = qc.Gadget(
        gadget.implements,
        gadget.circuit,
        inputs=list(gadget.inputs),
        outputs=list(gadget.outputs),
        checks=[as_references(check) for check in discovered.checks],
        readouts=[as_readout(value) for value in gadget.readouts],
        parameters=dict(gadget.parameters),
        metadata=dict(gadget.metadata),
    )
    set_gadget_readouts(completed, discovered.readouts)
    return completed


def complete_qodec(qodec: qc.Qodec) -> qc.Qodec:
    """Return a copy of ``qodec`` with every gadget completed.

    Applies :func:`complete_gadget` to each gadget of each layer, so the
    returned qodec carries the checks and observable bindings that exact
    simulation can derive. Layers whose gadgets all fail to complete are left
    untouched; a gadget whose circuit cannot be simulated is re-raised with its
    mnemonic attached so the offending draft is easy to find.

    The input qodec and every object it references are left unchanged.
    """
    layers = []
    for index, layer in enumerate(qodec.layers):
        completed: list[qc.Gadget] = []
        for mnemonic, gadget in layer.gadgets.items():
            completed.append(_try_complete_gadget(gadget, index, mnemonic))
        layers.append(qc.Layer(layer.isa, gadgets=completed))
    return qc.Qodec(
        layers,
        name=qodec.name,
        description=qodec.description,
        schema_version=qodec.schema_version,
        metadata=dict(qodec.metadata),
    )


def derive(target: qc.Gadget | qc.Qodec) -> qc.Gadget | qc.Qodec:
    """Discover checks and readout bindings, returning a new artifact."""
    if isinstance(target, qc.Gadget):
        return complete_gadget(target)
    if isinstance(target, qc.Qodec):
        return complete_qodec(target)
    raise TypeError(
        f"expected qodec.Gadget or qodec.Qodec, got {type(target).__name__}"
    )


def _try_complete_gadget(gadget: qc.Gadget, index: int, mnemonic: str) -> qc.Gadget:
    """Enrich a gadget completion error with its location within a qodec."""
    try:
        return complete_gadget(gadget)
    except Exception as error:  # noqa: BLE001 - preserve the original as the cause
        raise RuntimeError(
            f"failed to derive layer {index} gadget {mnemonic!r}"
        ) from error


__all__ = ["derive"]
