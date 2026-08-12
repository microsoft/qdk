"""Characteristics of :class:`qodec.Code` objects.

A code is a static object — a list of stabilizers and logical operators. These
functions read its structure: the syndrome an error produces
(:func:`syndrome_of`), the logical Pauli it induces (:func:`logical_effect_of`),
a basis for its unfixed gauge degrees of freedom (:func:`gauge_basis_of`), and a
Clifford circuit that encodes into it (:func:`encoding_clifford_of`).

Distance lives in :mod:`qdk.ec.distance`; comparing two codes lives in
:mod:`qdk.ec.equivalence`.
"""

from __future__ import annotations

from collections.abc import Sequence

import qodec
from paulimer import CliffordUnitary

from ._analysis.propagation.pauli import Pauli
from ._analysis.code_algebra import SubsystemCode
from ._analysis.code_algebra import encoding_clifford_of as _encoding_clifford_of


def _view(code: qodec.Code) -> SubsystemCode:
    # Transitional adapter until qodec exposes first-class gauge pairs.
    return SubsystemCode.from_qodec(code)


def syndrome_of(code: qodec.Code, error: Pauli) -> set[int]:
    """Return the stabilizer syndrome of ``error`` for ``code``."""
    return _view(code).syndrome_of(error)


def logical_effect_of(code: qodec.Code, error: Pauli) -> Pauli:
    """Return the logical Pauli induced by ``error`` on ``code``."""
    return _view(code).logical_action_of(error)


def gauge_basis_of(code: qodec.Code) -> tuple[Pauli, ...]:
    """Return a derived gauge basis for the code's unspecified degrees of freedom."""
    return tuple(_view(code).gauge_basis)


def codes_equivalent(
    left: qodec.Code,
    right: qodec.Code,
    *,
    including_signs: bool = False,
    strict_basis: bool = True,
) -> bool:
    """Whether two code definitions describe the same stabilizer code."""
    return _view(left).is_equivalent_to(
        _view(right),
        including_signs=including_signs,
        strict_basis=strict_basis,
    )


def encoding_clifford_of(
    code: qodec.Code,
    *,
    supported_by: Sequence[int] | None = None,
) -> CliffordUnitary:
    """Return a Clifford encoder for ``code``."""
    return _encoding_clifford_of(_view(code), supported_by=supported_by)


__all__ = [
    "codes_equivalent",
    "encoding_clifford_of",
    "gauge_basis_of",
    "logical_effect_of",
    "syndrome_of",
]
