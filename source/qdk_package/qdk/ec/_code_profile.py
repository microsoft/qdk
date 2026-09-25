"""Snapshot-based algebraic analysis of qodec code definitions."""

from __future__ import annotations

from typing import Sequence, TYPE_CHECKING

import qodec as qc
from paulimer import CliffordUnitary

from ._analysis.code_algebra import (
    _are_equivalent,
    _validate_error_support,
    subsystem_code_of,
)
from ._analysis.propagation.pauli import Pauli
from ._distance_result import Distance

if TYPE_CHECKING:
    from ._analysis.distance_solvers import BoundsSolver as _BoundsSolver
    from ._analysis.distance_solvers import ExactSolver as _ExactSolver


class CodeProfile:
    """Algebraic analysis of a qodec code's operators.

    The operators are snapshotted at construction. Later changes to the code
    do not affect this profile. Derived groups are computed on first access
    and cached; distance searches run when requested. The code's name,
    description, and persistence remain with qodec.

    Operator collections are tuples of independent Pauli copies. Equality
    compares the ordered operator snapshot, not code names or metadata.
    Profiles are not hashable; use is_equivalent_to for algebraic equivalence.
    X and Z name logical or gauge axes, not the physical Pauli factors.
    """

    def __init__(self, code: qc.Code) -> None:
        self._algebra = subsystem_code_of(code)

    @property
    def stabilizers(self) -> tuple[Pauli, ...]:
        """Physical generators in the snapshotted code.stabilizers order."""
        return tuple(pauli.copy() for pauli in self._algebra.stabilizers)

    @property
    def x(self) -> tuple[Pauli, ...]:
        """Physical logical-X representatives, indexed by logical qubit as in code.x."""
        return tuple(pauli.copy() for pauli in self._algebra.logical_basis[0::2])

    @property
    def z(self) -> tuple[Pauli, ...]:
        """Physical logical-Z representatives, indexed by logical qubit as in code.z."""
        return tuple(pauli.copy() for pauli in self._algebra.logical_basis[1::2])

    @property
    def gauge_x(self) -> tuple[Pauli, ...]:
        """Physical gauge-X generators, paired with gauge_z at the same index.

        Indexes identify gauge qubits, not physical labels. The derived basis
        is fixed within the snapshot but is not canonical.
        """
        return tuple(pauli.copy() for pauli in self._algebra.gauge_basis[0::2])

    @property
    def gauge_z(self) -> tuple[Pauli, ...]:
        """Physical gauge-Z generators, paired with gauge_x at the same index.

        Indexes identify gauge qubits, not physical labels. The derived basis
        is fixed within the snapshot but is not canonical.
        """
        return tuple(pauli.copy() for pauli in self._algebra.gauge_basis[1::2])

    @property
    def support(self) -> frozenset[int]:
        """Physical qubit labels in the analyzed operators, not a dense array extent."""
        return self._algebra.support

    @property
    def length(self) -> int:
        """Number of physical qubits in support, not the largest label plus one."""
        return self._algebra.length

    @property
    def logical_qubit_count(self) -> int:
        """Number of logical X/Z pairs, not the number of basis generators."""
        return self._algebra.logical_qubit_count

    def syndrome_of(self, error: Pauli) -> frozenset[int]:
        """Zero-based positions in ``stabilizers`` that anticommute with a physical error.

        Raise ValueError when the error acts outside support.
        """
        _validate_error_support(error, self.support)
        return self._algebra.syndrome_of(error)

    def logical_effect_of(
        self, error: Pauli, *, including_phase: bool = True
    ) -> Pauli:
        """Map a physical error to a Pauli on zero-based logical-qubit indexes.

        By default, require a signed logical action: the error must differ from
        its logical representative only by a stabilizer and scalar phase.
        Nonzero syndrome or a nontrivial gauge component raises ValueError.

        With including_phase=False, return the phase-free component defined by
        commutation with the logical basis, even for detectable or gauge errors.
        This does not imply a logical failure or assume any recovery.
        Errors outside support always raise ValueError.
        """
        if including_phase:
            return self._algebra.logical_action_of(error, require_phase=True)
        return self._algebra.unsigned_logical_action_of(error)

    def is_logical(self, error: Pauli) -> bool:
        """Whether a physical error has zero syndrome and a nonidentity logical effect.

        Identity, stabilizers, gauge-only changes, and detectable errors return
        False. Global phase is ignored. This is stricter than merely preserving
        the code space. Errors outside support raise ValueError.
        """
        _validate_error_support(error, self.support)
        return self._algebra.is_non_trivial_logical_error(error)

    def distance(
        self,
        *,
        errors: "str | Sequence[Pauli]" = "XYZ",
        logical_observable: Pauli | None = None,
        upper_bound: int | None = None,
        solver: "_ExactSolver | None" = None,
    ) -> Distance[Pauli]:
        """Return the minimum number of allowed errors and their witness factors.

        The default counts each single-qubit X, Y, or Z error once, giving
        ordinary Pauli-weight distance. A string restricts the allowed
        single-qubit errors; a sequence supplies explicit errors, including
        correlated multi-qubit Paulis, each counted once. The witness remains
        a selection of factors, accessible through result.witness.factors;
        result.witness.product is their combined Pauli. Select solver="highs"
        (the default), "enumeration", or "mwpf". HiGHS is included in qdk[ec];
        MWPF requires a separate pip install mwpf.
        A cutoff or an unresolved bound gap raises RuntimeError. A
        result with both bounds None means no allowed logical error exists.

        logical_observable restricts failure to flipping that nonidentity,
        Hermitian Pauli on zero-based logical-qubit indexes; for example Z_0
        selects logical X_0 or Y_0 errors. None permits any logical failure.
        Out-of-support errors and invalid observables raise ValueError.
        upper_bound is a search cutoff, not a certified bound; MWPF ignores it.
        """
        from ._analysis.distance_solvers import HighsSolverOptions
        from ._distance import CodeDistanceData, _pauli_product, distance_result_of

        data = CodeDistanceData.of(
            self._algebra, errors, logical_observable=logical_observable
        )
        return distance_result_of(
            data.odd_cycles,
            data.errors,
            solver=HighsSolverOptions() if solver is None else solver,
            upper_bound=upper_bound,
            exact=True,
            product=_pauli_product,
            copy=Pauli.copy,
        )

    def distance_bounds(
        self,
        *,
        errors: "str | Sequence[Pauli]" = "XYZ",
        logical_observable: Pauli | None = None,
        upper_bound: int | None = None,
        solver: "_BoundsSolver | None" = None,
    ) -> Distance[Pauli]:
        """Return lower and upper distance bounds and witness factors.

        Uses the same error counting as :meth:`distance`: X, Y, and Z each
        cost one by default; an explicit error sequence can include correlated
        Paulis. result.witness.factors retains the selection establishing the
        upper bound; result.witness.product is its combined Pauli. With no
        finite upper bound, result.witness raises LookupError.
        Select solver="highs" (the default), "enumeration", or "mwpf".
        HiGHS is included in qdk[ec]; MWPF requires a separate pip install mwpf.
        None represents infinity in either
        bound; both bounds None proves impossibility. Enumeration and HiGHS use
        upper_bound as a search cutoff; MWPF ignores it. Backend failures,
        invalid witnesses, or unavailable bound certificates
        raise RuntimeError rather than returning a partial or uncertified bound.
        logical_observable has the same logical-qubit indexes, validation, and
        failure meaning as in distance. Errors outside support raise ValueError.
        """
        from ._analysis.distance_solvers import HighsSolverOptions
        from ._distance import CodeDistanceData, _pauli_product, distance_result_of

        data = CodeDistanceData.of(
            self._algebra, errors, logical_observable=logical_observable
        )
        return distance_result_of(
            data.odd_cycles,
            data.errors,
            solver=HighsSolverOptions() if solver is None else solver,
            upper_bound=upper_bound,
            product=_pauli_product,
            copy=Pauli.copy,
        )

    def encoding_clifford(
        self, *, supported_by: Sequence[int] | None = None
    ) -> CliffordUnitary:
        """Return the encoding Clifford with physical support mapped to dense indices.

        ``supported_by`` lists every physical qubit label in ``support`` once;
        its order assigns positions 0, 1, ... in the returned Clifford. The
        default is increasing label order. Repeated, incomplete, or different
        support raises ``ValueError``.
        """
        if supported_by is not None and (
            len(supported_by) != len(self.support)
            or frozenset(supported_by) != self.support
        ):
            raise ValueError("supported_by must list every physical support label once")
        return self._algebra.encoding_clifford(supported_by=supported_by)

    def representative_of(self, pauli: Pauli) -> Pauli:
        """Expand a Pauli on zero-based logical-qubit indexes to physical support.

        Use the chosen logical basis and preserve phase. Invalid logical-qubit
        indexes raise ValueError.
        """
        return self._algebra.representative_of(pauli)

    def is_equivalent_to(
        self,
        other: "CodeProfile",
        *,
        including_signs: bool = True,
        strict_basis: bool = True,
    ) -> bool:
        """Compare stabilizer and gauge groups and the chosen logical basis.

        By default signs and logical-basis order matter. Set strict_basis=False
        to compare logical groups instead, allowing another basis.
        """
        return not self.why_not_equivalent_to(
            other,
            including_signs=including_signs,
            strict_basis=strict_basis,
        )

    def why_not_equivalent_to(
        self,
        other: "CodeProfile",
        *,
        including_signs: bool = True,
        strict_basis: bool = True,
    ) -> str:
        """Return the first difference, or "" exactly when is_equivalent_to is True."""
        if self.support != other.support:
            return f"Code supports differ: {self.support!r} vs {other.support!r}."
        for name, left_group, right_group in (
            ("Stabilizer", self._algebra.stabilizer, other._algebra.stabilizer),
            ("Gauge", self._algebra.gauge, other._algebra.gauge),
        ):
            if not _are_equivalent(
                left_group,
                right_group,
                including_signs=including_signs,
            ):
                return f"{name} groups differ."
        if strict_basis:
            left, right = self._algebra.logical_basis, other._algebra.logical_basis
            if not including_signs:
                left, right = tuple(map(abs, left)), tuple(map(abs, right))
            if left != right:
                return "Logical bases differ."
        elif not _are_equivalent(
            self._algebra.logical,
            other._algebra.logical,
            including_signs=including_signs,
        ):
            return "Logical groups differ."
        return ""

    def __eq__(self, other: object) -> bool:
        return isinstance(other, CodeProfile) and self._algebra == other._algebra


__all__ = ["CodeProfile"]
