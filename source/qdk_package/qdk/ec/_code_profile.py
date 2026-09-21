"""Snapshot-based algebraic analysis of qodec code definitions."""

from __future__ import annotations

from typing import Sequence, TYPE_CHECKING

import qodec as qc
from paulimer import CliffordUnitary, PauliGroup

from ._analysis.code_algebra import subsystem_code_of
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
    """

    def __init__(self, code: qc.Code) -> None:
        self._algebra = subsystem_code_of(code)

    @property
    def stabilizer(self) -> PauliGroup:
        return self._algebra.stabilizer

    @property
    def stabilizers(self) -> Sequence[Pauli]:
        return self._algebra.stabilizers

    @property
    def anti_stabilizer(self) -> PauliGroup:
        return self._algebra.anti_stabilizer

    @property
    def anti_stabilizers(self) -> Sequence[Pauli]:
        return self._algebra.anti_stabilizers

    @property
    def gauge(self) -> PauliGroup:
        """The gauge group as declared, or derived when none was declared."""
        return self._algebra.gauge

    @property
    def gauge_basis(self) -> tuple[Pauli, ...]:
        return self._algebra.gauge_basis

    @property
    def logical(self) -> PauliGroup:
        return self._algebra.logical

    @property
    def logical_basis(self) -> Sequence[Pauli]:
        return self._algebra.logical_basis

    @property
    def support(self) -> frozenset[int]:
        return self._algebra.support

    @property
    def length(self) -> int:
        return self._algebra.length

    @property
    def logical_qubit_count(self) -> int:
        return self._algebra.logical_qubit_count

    def syndrome_of(self, error: Pauli) -> frozenset[int]:
        """Zero-based positions in ``stabilizers`` that anticommute with ``error``."""

        return self._algebra.syndrome_of(error)

    def logical_effect_of(self, error: Pauli) -> Pauli:
        """Return the logical Pauli induced by ``error``."""
        return self._algebra.logical_effect_of(error)

    def distance(
        self,
        *,
        errors: "str | Sequence[Pauli]" = "XYZ",
        coset_representative: Pauli | None = None,
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
        """
        from ._analysis.distance_solvers import HighsSolverOptions
        from ._distance import CodeDistanceData, _pauli_product, distance_result_of

        data = CodeDistanceData.of(self._algebra, errors)
        return distance_result_of(
            data.odd_cycles,
            data.errors,
            solver=HighsSolverOptions() if solver is None else solver,
            upper_bound=upper_bound,
            coset_indicator=data.parity_indicator(coset_representative),
            exact=True,
            product=_pauli_product,
            copy=Pauli.copy,
        )

    def distance_bounds(
        self,
        *,
        errors: "str | Sequence[Pauli]" = "XYZ",
        coset_representative: Pauli | None = None,
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
        """
        from ._analysis.distance_solvers import HighsSolverOptions
        from ._distance import CodeDistanceData, _pauli_product, distance_result_of

        data = CodeDistanceData.of(self._algebra, errors)
        return distance_result_of(
            data.odd_cycles,
            data.errors,
            solver=HighsSolverOptions() if solver is None else solver,
            upper_bound=upper_bound,
            coset_indicator=data.parity_indicator(coset_representative),
            product=_pauli_product,
            copy=Pauli.copy,
        )

    def encoding_clifford(
        self, *, supported_by: Sequence[int] | None = None
    ) -> CliffordUnitary:
        """Return the encoding Clifford with physical support mapped to dense indices.

        ``supported_by`` lists every physical qubit label in ``support`` once;
        its order assigns positions 0, 1, ... in the returned Clifford. The
        default is increasing label order. Incomplete or different support
        raises ``ValueError``.
        """
        return self._algebra.encoding_clifford(supported_by=supported_by)

    def is_trivial_error(self, error: Pauli) -> bool:
        return self._algebra.is_trivial_error(error)

    def is_trivial_logical_error(self, error: Pauli) -> bool:
        return self._algebra.is_trivial_logical_error(error)

    def is_logical_error(self, error: Pauli) -> bool:
        return self._algebra.is_logical_error(error)

    def is_non_trivial_logical_error(self, error: Pauli) -> bool:
        return self._algebra.is_non_trivial_logical_error(error)

    def logical_action_of(self, error: Pauli) -> Pauli:
        return self._algebra.logical_action_of(error)

    def representative_of(self, pauli: Pauli) -> Pauli:
        return self._algebra.representative_of(pauli)

    def unsigned_logical_action_of(self, error: Pauli) -> Pauli:
        return self._algebra.unsigned_logical_action_of(error)

    def is_equivalent_to(
        self,
        other: "CodeProfile",
        *,
        including_signs: bool = False,
        strict_basis: bool = True,
    ) -> bool:
        return self._algebra.is_equivalent_to(
            other._algebra,
            including_signs=including_signs,
            strict_basis=strict_basis,
        )

    def why_not_equivalent_to(self, other: "CodeProfile") -> str:
        return self._algebra.why_not_equivalent_to(other._algebra)

    def __eq__(self, other: object) -> bool:
        return isinstance(other, CodeProfile) and self._algebra == other._algebra

    def __hash__(self) -> int:
        return hash(self._algebra)


__all__ = ["CodeProfile"]
