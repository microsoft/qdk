"""Observable/flag split over a gadget's positional ``readouts`` list.

``Gadget.readouts`` is one positional list: the implemented instruction's
``observe`` outcomes first (the observables), then its ``flags:`` flags. The
boundary between the two is fixed by the instruction, not by the gadget, so
these helpers read it off ``gadget.implements`` rather than guessing.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping

import qodec

from ._references import outcome_indices, readout_atoms


def observe_count(gadget: qodec.Gadget) -> int:
    """Number of ``observe`` outcome bits the gadget's instruction declares."""
    return sum(
        len(action.observables)
        for action in gadget.implements.action
        if isinstance(action, qodec.actions.Observe)
    )


def readout_equation(entry: qodec.Readout) -> list[str]:
    """The flat atom-string list of one ``gadget.readouts`` entry.

    An entry is either a bare parity equation or a single-key
    ``{name: equation}`` mapping; both reduce to the same flat atom list.
    """
    if isinstance(entry, Mapping):
        (equation,) = entry.values()
        return [str(atom) for atom in equation]
    return [str(atom) for atom in entry]


def observable_names(gadget: qodec.Gadget) -> list[str]:
    """Positional names of the gadget's *bound* observables (``"0"``, ``"1"``, ...).

    A gadget that declares fewer readouts than its instruction has observe
    outcomes binds only the leading ones; the rest are reported missing by the
    auditor.
    """
    return [
        str(position)
        for position in range(min(observe_count(gadget), len(gadget.readouts)))
    ]


def observables_as_xor_map(gadget: qodec.Gadget) -> dict[str, list[int]]:
    """Gadget observables: positional name → measurement-record XOR.

    The trailing flag entries are deliberately excluded: a flag is a
    decoder-blind side-channel bit, not a logical observable.
    """
    return {
        name: outcome_indices(readout_equation(gadget.readouts[int(name)]))
        for name in observable_names(gadget)
    }


def set_gadget_readouts(
    gadget: qodec.Gadget, named_xor: Mapping[str, Iterable[int]]
) -> None:
    """Set the observable entries of ``gadget.readouts`` from an XOR map.

    ``named_xor`` is a position-keyed observable-XOR map (decimal-string keys
    ``"0"``, ``"1"``, ...); each becomes one ``circuit.readouts[...]`` parity
    equation, in positional order. Non-positional (flag-named) keys are ignored.

    Any pre-authored trailing flag entries are preserved: flags carry no Pauli
    expectation, so they are authored by hand rather than discovered, and
    re-deriving the observables must not drop them.
    """
    positional: dict[int, list[str]] = {}
    for name, indices in named_xor.items():
        if str(name).isdigit():
            positional[int(name)] = readout_atoms(indices)
    observables = [positional[index] for index in sorted(positional)]
    flags = list(gadget.readouts)[observe_count(gadget) :]
    gadget.readouts = observables + flags


__all__ = [
    "observable_names",
    "observables_as_xor_map",
    "observe_count",
    "readout_equation",
    "set_gadget_readouts",
]
