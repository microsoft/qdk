"""Cirq implementation of the Phase Lookup (Phaseup) procedure from Gidney (2025).
See https://arxiv.org/abs/2505.15917

This module defines custom Cirq gates composing the Phaseup step: building and
uncomputing power-product (monomial) registers and applying classically
controlled multi-target phase operations (see PhaseupGate and SqrtPhaseupGate).
"""

from collections.abc import Sequence

import cirq
from sympy import Expr, Xor


class AndGate(cirq.Gate):
    """Three-qubit logical AND gate implemented via a single Toffoli (CCNOT).
    Decomposes directly to a single Cirq CCNOT (Toffoli) gate.
    Will be removed in favor of existing AND gate
    """

    def __init__(self) -> None:
        """Initialize AND gate"""
        super().__init__()

    def num_qubits(self) -> int:
        """AND gate uses 3 qubits: two controls and one target"""
        return 3

    def _decompose_(self, qubits: Sequence[cirq.Qid]) -> cirq.OP_TREE:
        """Delegate to CCNOT (Toffoli) gate"""
        yield cirq.CCNOT.on(*qubits)

    def _circuit_diagram_info_(
        self, args: cirq.CircuitDiagramInfoArgs
    ) -> cirq.CircuitDiagramInfo:
        """Custom diagram labels for the gate"""
        return ["●", "●", "X"]

    def __repr__(self) -> str:
        """display string: AND"""
        return "AND"


class MultitargetZGate(cirq.Gate):
    """
    Multitarget classically controlled Z gate.

    Applies a Z phase to each target qubit i conditioned on the corresponding
    classical Boolean expression classical_control_vars[i]. Functionally, this
    is equivalent to a collection of single-qubit Z gates, each with its own
    classical control.

    Args:
        n_qubits: Number of target qubits (and thus the number of independent
            classical controls).
        classical_control_vars: A sequence of SymPy Boolean expressions of the
            length n_qubits. Entry at index i controls whether a Z is applied
            to the target qubit at index i.

    Notes:
        - This gate decomposes into n_qubits operations of the form
          Z(q[i]).with_classical_controls(classical_control_vars[i]).
        - The circuit diagram displays each target wire as "Z^(<expr>)" to
          indicate the associated classical control expression.
        - No gates are applied if n_qubits = 0.

    Raises:
        ValueError: If n_qubits < 0 or if the number of classical control
            variables does not match n_qubits.
    """

    def __init__(self, n_qubits: int, classical_control_vars: Sequence[Expr]) -> None:
        """Initialize multitarget Z gate"""
        super().__init__()
        if n_qubits < 0:
            raise ValueError(
                f"MultitargetZGate: n_qubits must be >= 0. It is {n_qubits}."
            )
        if len(classical_control_vars) != n_qubits:
            raise ValueError(
                "MultitargetZGate: Number of classical control vars must be "
                f"{n_qubits}. It is {len(classical_control_vars)}."
            )
        self.n_qubits = n_qubits
        self.classical_control_vars = classical_control_vars

    def _num_qubits_(self) -> int:
        """
        multitarget Z gate uses n_qubits qubits:
        they are targets for each classicaly controlled Z gate.
        """
        # number of qubits passed is automatically checked by cirq against this value.
        return self.n_qubits

    def _decompose_(self, qubits: Sequence[cirq.Qid]) -> cirq.OP_TREE:
        """
        Decompose into n_qubits single-qubit classically controlled Z gates.
        No gates are applied if n_qubits == 0.
        """
        for i in range(self.n_qubits):
            yield cirq.Z.on(qubits[i]).with_classical_controls(
                self.classical_control_vars[i]
            )

    def _circuit_diagram_info_(
        self, args: cirq.CircuitDiagramInfoArgs
    ) -> cirq.CircuitDiagramInfo:
        """Custom diagram labels for the gate"""
        symbols = [f"Z^({n})" for n in self.classical_control_vars]
        return cirq.CircuitDiagramInfo(wire_symbols=tuple(symbols))

    def __repr__(self) -> str:
        """display string: MultitargetZ<n_qubits>"""
        return f"MultitargetZ{self.n_qubits}"


class MultitargetCZGate(cirq.Gate):
    """
    Multitarget classically controlled CZ gate.

    This gate acts on one control qubit and n_targets target qubits. For each
    target index i, it applies a CZ between the shared control qubit and target
    qubit at index i, further conditioned on the corresponding classical Boolean
    expression classical_control_vars[i]. It should be implementable efficiently
    by the error-correction layer.

    Args:
        n_targets: Number of quantum target qubits. Total qubits required by
            the gate is n_targets + 1 (one control plus n_targets targets).
        classical_control_vars: A sequence of SymPy Boolean expressions with
            length n_targets. Entry at index i controls whether CZ(control, target_i)
            is applied.

    Notes:
        - Decomposes into per-target operations of the form
          CZ(control, targets[i]).with_classical_controls(classical_control_vars[i]).
        - The circuit diagram shows the first wire as a quantum control "●" and
          each target wire labeled as "Z^(<expr>)" to indicate its classical
          control expression.
        - No gates are applied if n_qubits = 0.

    Raises:
        ValueError: If n_targets < 0 or if the number of classical control
            variables does not equal n_targets.
    """

    def __init__(self, n_targets: int, classical_control_vars: Sequence[Expr]) -> None:
        """Initialize multitarget CZ gate"""
        super().__init__()
        if n_targets < 0:
            raise ValueError(
                f"MultitargetCZGate: n_targets must be >= 0. It is {n_targets}."
            )
        if len(classical_control_vars) != n_targets:
            raise ValueError(
                "MultitargetCZGate: Number of classical control vars must be "
                f"{n_targets}. It is {len(classical_control_vars)}."
            )
        self.n_targets = n_targets
        self.n_qubits = n_targets + 1  # One control bit
        self.classical_control_vars = classical_control_vars

    def _num_qubits_(self) -> int:
        """
        multitarget Z gate uses n_targets+1 qubits: One control
        and n_targets targets for each classicaly controlled CZ gate.
        """
        # number of qubits passed is automatically checked by cirq against this value.
        return self.n_qubits

    def _decompose_(self, qubits: Sequence[cirq.Qid]) -> cirq.OP_TREE:
        """
        Decompose into n_qubits two-qubit classically controlled CZ gates.
        No gates applied if n_targets = 0
        """
        control, *targets = qubits
        for i in range(self.n_targets):
            yield cirq.CZ.on(control, targets[i]).with_classical_controls(
                self.classical_control_vars[i]
            )

    def _circuit_diagram_info_(
        self, args: cirq.CircuitDiagramInfoArgs
    ) -> cirq.CircuitDiagramInfo:
        """Custom diagram labels for the gate"""
        symbols = ["●"] + [f"Z^({n})" for n in self.classical_control_vars]
        return cirq.CircuitDiagramInfo(wire_symbols=tuple(symbols))

    def __repr__(self) -> str:
        """display string: MultitargetCZ<n_targets>"""
        return f"MultitargetCZ{self.n_targets}"


class BarrierGate(cirq.Gate):
    """A no-op gate used to enforce a moment boundary.

    Cirq's moment packing ignores classical control dependencies when deciding
    whether operations can be placed in the same moment; this barrier gate
    introduces an artificial quantum dependency by touching both a measured
    qubit and a qubit used in the subsequent classically-controlled operation.
    It decomposes to no gates.
    """

    def __init__(self, n_qubits: int) -> None:
        """Initialize barrier gate"""
        super().__init__()
        self._n = n_qubits

    def _num_qubits_(self) -> int:
        """Barrier gate uses n_qubits qubits"""
        return self._n

    def _decompose_(self, qubits: Sequence[cirq.Qid]) -> cirq.OP_TREE:
        """Decomposes to no gates."""
        return []

    def _circuit_diagram_info_(
        self, args: cirq.CircuitDiagramInfoArgs
    ) -> cirq.CircuitDiagramInfo:
        """Custom diagram labels for the gate"""
        return cirq.CircuitDiagramInfo(wire_symbols=("│",) * self._n)

    def __repr__(self) -> str:
        """display string: Barrier<n_qubits>"""
        return f"Barrier{self._n}"


class DoPowerProductGate(cirq.Gate):
    """
    Computes all non-empty power products for n_variables source qubits
    and 2^n_variables-n_variables-1 aux qubits.

    Boolean variable terminology is used for easier understanding (Each source
    qubit is a Boolean variable, each power product is a monomial, etc.) So,
    given n variables, the gate acts on 2^n - 1 qubits, one per non-empty
    monomial. It uses AND gates to build higher-degree monomials from lower-degree ones.

    Qubit layout expectation:
        Internally, decomposition prepends and empty placeholder at index 0
        (representing the empty monomial ≡1). With that implicit placeholder,
        the expected ordering is:
        - At index 2^i sits the single-variable monomial x_i (0-based i)
            that corresponds to an existing source qubit
        - For i >= 1 and 1 <= j < 2^i, index 2^i + j the gate computes x_i AND (monomial
            encoded by index j) on auxiliary qubit.
        The helper DoPowerProductGate.rearrange_qubits(...) returns a sequence
        matching this convention (with the leading None placeholder) given source
        and auxiliary qubits. The gate itself should be applied to that sequence
        starting from index 1.

    Args:
        n_variables: Number of Boolean variables n.

    Notes:
        - Decomposes into a sequence of AND gates that compute the
            required monomials in-place on the provided qubit register.
        - The circuit diagram writes "PP" on the first wire and "." on the
            remaining wires of the monomial register.

    Raises:
        ValueError: If n_variables < 1.
    """

    def __init__(self, n_variables: int) -> None:
        """Initialize Power Product Gate"""
        super().__init__()
        if n_variables < 1:
            raise ValueError(
                f"Number of source qubits n must be >= 1. It is {n_variables}."
            )
        self.n_variables = n_variables
        self.n_minterms = 1 << n_variables  # 2**n for all combinations
        self.n_monomials = self.n_minterms - 1  # No monomial for empty set of variable.

    def _num_qubits_(self) -> int:
        """
        Do Power Product gate uses 2**n_variables-1 qubits: n_variables source
        qubits and 2**n_variables-n_variables-1 auxiliary qubits.
        """
        # number of qubits passed is automatically checked by cirq against this value.
        # One qubit is used for each monomial (power product).
        # No qubit is used for monomial ≡1.
        return self.n_monomials

    def _decompose_(self, qubits: Sequence[cirq.Qid]) -> cirq.OP_TREE:
        """
        Decompose into multiple AND gates to build power products on auxiliary qubits.
        """
        # Can't pass None as a first qubit, so we pass a shorter tuple
        # and reconstruct the complete tuple here.
        qubits = [None] + [*qubits]
        # This should be checked by Cirq. Assert just in case
        assert len(qubits) == self.n_minterms

        and_gate = AndGate()
        for i in range(1, self.n_variables):
            # Assume arrangement of qubits where single-variable monomials
            # are at indexes 2^i
            var_index = 1 << i
            for j in range(1, var_index):
                yield and_gate.on(qubits[j], qubits[var_index], qubits[var_index + j])

    def _circuit_diagram_info_(
        self, args: cirq.CircuitDiagramInfoArgs
    ) -> cirq.CircuitDiagramInfo:
        """Custom diagram labels for the gate"""
        return cirq.CircuitDiagramInfo(
            wire_symbols=("PP",) + (".",) * (self.n_monomials - 1)
        )

    def __repr__(self) -> str:
        """display string: PP<n_variables>"""
        return f"PP{self.n_variables}"

    @classmethod
    def rearrange_qubits(
        cls, qubits: Sequence[cirq.Qid], aux_qubits: Sequence[cirq.Qid]
    ) -> Sequence[cirq.Qid | None]:
        """
        Rearrange source and auxiliary qubits into the power-product order.

        Constructs the register layout expected by DoPowerProductGate,
        phasing procedure, and UndoPowerProductGate. The returned sequence
        has length 2**n where n = len(qubits).

        Entry 0 is a placeholder None for the empty monomial (≡ 1).
        In general, index k holds a monomial corresponding to the bit
        representation of k. Bit i of the number k tells if a variable
        x_i is present in the monomial.

        Args:
            qubits: Source variable qubits x_0..x_{n-1}.
            aux_qubits: Auxiliary qubits used to hold higher-degree monomials;
                must have length 2**n - n - 1.

        Returns:
            A sequence of length 2**n where entry 0 is None and entries
            1..(2**n-1) are QIDs laid out as described above.
        """
        n = len(qubits)
        n_aux = (1 << n) - n - 1
        if len(aux_qubits) != n_aux:
            raise ValueError(
                "Number of auxiliary qubits should be "
                f"{n_aux}. It is {len(aux_qubits)}."
            )

        # The first element corresponds to an empty set and is never used.
        power_products: list[cirq.Qid] = [None]
        # Index to take next free qubit from aux_qubits array.
        next_available: int = 0
        # Consider every index in the input qubit register
        for qubit_index in range(len(qubits)):
            # First, add the set that consists of only one qubit at index qubit_index.
            power_products.extend([qubits[qubit_index]])
            # Then add enough aux qubits for the monomials that include this qubit
            # as the last one.
            count_new = len(power_products) - 2  # Number of existing non-empty sets.
            if count_new > 0:
                power_products.extend(
                    aux_qubits[next_available : next_available + count_new]
                )
                next_available += count_new

        assert next_available == len(aux_qubits), (
            "ConstructPowerProducts: All auxiliary qubits should be used."
        )
        return power_products


class UndoPowerProductGate(cirq.Gate):
    """
    Uncomputes (cleans up) the power-product / monomial register created by
    DoPowerProductGate.

    Expects the same 2^n - 1 qubit ordering: indices correspond to monomials
    per DoPowerProductGate.rearrange_qubits. (No qubit is used for the the empty
    monomial ≡1, it is pre-pended internally). Walking backward over
    source variables, it erases monomials that end in the current variable by
    (1) measuring those monomial qubits to obtain classical control bits and
    (2) applying a classically-controlled multitarget CZ to toggle phases so
    the entanglement / information can be safely discarded.

    A no-op BarrierGate is inserted to force a moment boundary between the
    measurement layer and the classically-controlled operations due to Cirq's
    current moment packing heuristics ignoring measurement->classical-control
    dependencies.

    Args:
        uniquifier: String used to namespace measurement keys so multiple
            undo operations in the same circuit do not collide.
        n_variables: Number of original source Boolean variables (n). The gate
            operates on 2^n - 1 qubits (all non-empty monomials).

    Notes:
        - If n_variables == 1 there is nothing to uncompute.
        - Each iteration halves the window (h) until all higher-degree
          monomials have been removed.
        - Measurement keys are of the form f"{uniquifier}_<index>" where
          <index> is the monomial register index being cleared.

    Raises:
        ValueError: If n_variables < 1.
    """

    def __init__(self, uniquifier: str, n_variables: int) -> None:
        """Initialize Undo Power Product Gate"""
        super().__init__()
        if n_variables < 1:
            raise ValueError(f"Number of qubits must be >= 1. It is {n_variables}")
        self.uniquifier = uniquifier
        self.n_variables = n_variables
        self.n_minterms = 1 << n_variables  # 2**n for all combinations
        # No monomial for the empty set of variables.
        self.n_monomials = self.n_minterms - 1

    def _num_qubits_(self) -> int:
        """
        Undo Power Product gate uses 2**n_variables-1 qubits: n_variables source
        qubits and 2**n_variables-n_variables-1 auxiliary qubits.
        """
        # number of qubits passed is automatically checked by cirq against this value.
        # One qubit for each monomial (power product)
        return self.n_monomials

    def _decompose_(self, qubits: Sequence[cirq.Qid]) -> cirq.OP_TREE:
        """Decomposes into measurements and phase-corrections."""
        if self.n_minterms <= 1:
            # Nothing to undo (at least with no 'wandering').
            # This was one of the source qubits.
            return

        # We can't pass None as a first qubit, so we pass a shorter tuple
        # and pre-pend it with None.
        products = [None] + [*qubits]
        # At index h a source qubit is located. To the right are all power products
        # ending in it. We are going backwards over all original qubits.
        h = self.n_minterms // 2
        # If h is 1 we have nothing else to undo.
        while h > 1:
            # Go over all sets that end in original qubit currently at index h.
            # NOTE: k is at least 1. entry at index 0 is never used.
            # NOTE: The order of targets in a multi-target CZ gate doesn't matter.
            classical_controls = []
            for i in range(h + 1, 2 * h):
                key = cirq.MeasurementKey(f"{self.uniquifier}_{i}")
                yield cirq.H.on(products[i])
                yield cirq.measure(products[i], key=key.name)
                classical_controls.append(key)

            # Force a moment boundary between the measurements above and the
            # following classically-controlled CZs by inserting a barrier that
            # overlaps one measured qubit and the upcoming control qubit.
            if h + 1 < 2 * h:
                yield BarrierGate(2).on(products[h + 1], products[h])

            mt_cz_gate = MultitargetCZGate(h - 1, classical_controls)
            yield mt_cz_gate.on(products[h], *products[1:h])

            # Done with qubit at index h. Go to next original qubit.
            h = h // 2

    def _circuit_diagram_info_(
        self, args: cirq.CircuitDiagramInfoArgs
    ) -> cirq.CircuitDiagramInfo:
        """Custom diagram labels for the gate"""
        return cirq.CircuitDiagramInfo(
            wire_symbols=("UPP",) + (".",) * (self.n_monomials - 1)
        )

    def __repr__(self) -> str:
        """display string: UPP<n_variables>_<uniquifier>"""
        return f"UPP{self.n_variables}_{self.uniquifier}"


class PhaseupGate(cirq.Gate):
    """
    Implements the phase lookup procedure: inverts the phases of the basis vector
    coefficients for which the corresponding Boolean value in the data sequence
    are true.

    Given n logical variables (qubits) and a length-2^n list of SymPy Boolean
    expressions (data) representing the function's minterm coefficients, the gate
    performs a fast Boole-Möbius transform over GF(2) to obtain algebraic
    normal form (ANF) coefficients, then applies classically-controlled Z
    phases for every non-empty monomial term. Temporary higher-degree monomial
    qubits are synthesized (via DoPowerProductGate) and uncomputed
    (via UndoPowerProductGate) around the phase application.

    Args:
        uniquifier: String used to namespace internal measurement / control keys.
        n_variables: Number of logical input variables (n >= 1).
        data: Sequence of length 2**n of SymPy expressions giving minterm coefficients.

    Attributes (counts):
        n_minterms  = 2**n (the size of classical data)
        n_monomials = 2**n - 1 (excludes the empty product)
        n_aux_qubits = n_monomials - n (number of higher order monomials)

    Decomposition steps:
        1. Reorder variable + aux qubits into monomial layout.
        2. Compute monomials with DoPowerProductGate.
        3. Compute ANF coefficients via walsh_hadamard_boolean.
        4. Apply MultitargetZGate over all monomials (skipping the empty one).
        5. Uncompute monomials with UndoPowerProductGate.

    Raises:
        ValueError: If n_variables < 1 or len(data) != 2**n.
    """

    def __init__(self, uniquifier: str, n_variables: int, data: Sequence[Expr]) -> None:
        """Initialize Phaseup Gate"""
        super().__init__()

        self.uniquifier = uniquifier
        if n_variables < 1:
            raise ValueError(
                "PhaseupGate: Qubit register length must be at least 1. It is "
                f"{n_variables}"
            )
        self.n_variables = n_variables
        self.data = data

        self.n_minterms = 1 << self.n_variables  # 2^n for all combinations
        self.n_monomials = self.n_minterms - 1  # No monomial for empty set of variable.
        # One aux qubit for each higher degree monomial
        self.n_aux = self.n_monomials - self.n_variables

        if len(data) != self.n_minterms:
            raise ValueError(
                "PhaseupGate: The length of data array should be "
                f"{self.n_minterms}. It is {len(data)}"
            )

    def get_aux_count(self) -> int:
        """The number of aux qubits needed to perform Phaseup operation"""
        return self.n_aux

    def _num_qubits_(self) -> int:
        """
        Phaseup gate uses 2**n_variables-1 qubits: n_variables source
        qubits and 2**n_variables-n_variables-1 auxiliary qubits.
        """
        # number of qubits passed is automatically checked by cirq against this value.
        return self.n_monomials  # One qubit for each monomial

    def _decompose_(self, qubits: Sequence[cirq.Qid]) -> cirq.OP_TREE:
        """Decomposes into monomial construction, phasing, and uncomputation."""
        # assume first n_variables qubits are original qubits and the rest are aux
        variables = qubits[: self.n_variables]
        aux = qubits[self.n_variables :]

        # Rearrange qubits in the order of monomials
        monomials = DoPowerProductGate.rearrange_qubits(variables, aux)

        # Transform classical data from minterm coefficients to polynomial coefficients
        control_variables = PhaseupGate.boole_mobius_transform(self.data)

        # create required monomials on aux qubits
        pp_gate = DoPowerProductGate(self.n_variables)
        yield pp_gate.on(*monomials[1:])

        # Apply Z phasing.
        mt_z_gate = MultitargetZGate(self.n_monomials, control_variables[1:])
        yield mt_z_gate.on(*monomials[1:])

        # Uncompute higher-order monomials on aux qubits
        upp_gate = UndoPowerProductGate(self.uniquifier, self.n_variables)
        yield upp_gate.on(*monomials[1:])

    def _circuit_diagram_info_(
        self, args: cirq.CircuitDiagramInfoArgs
    ) -> cirq.CircuitDiagramInfo:
        """Custom diagram labels for the gate"""
        return cirq.CircuitDiagramInfo(
            wire_symbols=("?ZZ",) * self.n_variables
            + ("?&ZZ",) * (self.n_monomials - self.n_variables)
        )

    def __repr__(self) -> str:
        """display string: Phaseup<n_variables>_<uniquifier>"""
        return f"Phaseup{self.n_variables}_{self.uniquifier}"

    @classmethod
    def boole_mobius_transform(cls, vars_list: Sequence[Expr]) -> list[Expr]:
        """
        Perform symbolic Boole-Möbius transform over GF(2) to compute ANF coefficients.
        See https://wikipedia.org/wiki/Boolean_function#Derived_functions

        Parameters:
            vars_list (List[Expr]): List of SymPy Boolean expressions
            representing minterm coefficients.

        Returns:
            list[Expr]: List of SymPy Boolean expressions representing ANF coefficients.
        """
        n: int = len(vars_list)
        if n & (n - 1) != 0:
            raise ValueError(
                "boole_mobius_transform: Length of vars_list must be a power of 2. "
                f"It is {n}."
            )

        coeffs: list[Expr] = list(vars_list[:])
        step: int = 1

        while step < n:
            for i in range(0, n, 2 * step):
                for j in range(step):
                    a: Expr = coeffs[i + j]
                    b: Expr = coeffs[i + j + step]
                    # XOR is the addition in the GF(2) field
                    coeffs[i + j + step] = Xor(a, b, evaluate=False)
            step *= 2

        return coeffs


class SqrtPhaseupGate(cirq.Gate):
    """
    Space-efficient ("square root" auxiliary qubit) variant of PhaseupGate.

    Instead of constructing all higher-degree monomials over the full set of n
    variables at once (which needs 2**n - 1 qubits), the register is split into
    two halves of sizes n1 = floor(n/2) and n2 = n - n1. Monomials are built
    separately for each half, requiring only (2**n1 - 1) + (2**n2 - 1)
    qubits (without the two empty sets) plus the original variables, which is
    asymptotically O(2**(n/2)) auxiliary qubits instead of O(2**n).

    After building half-monomials for each side using DoPowerProductGate,
    the Boolean function given by the minterm coefficients (data) is converted
    into algebraic normal form (ANF) coefficients (via boole_mobius_transform)
    and factored into a matrix with rows indexed by second-half minterms and
    columns by first-half minterms. Phases are then applied using:
        - MultitargetZGate on each half for column/row 0 terms.
        - MultitargetCZGate for mixed terms where a monomial includes factors
          from both halves.
    Finally, half-monomials are uncomputed.

    Args:
        uniquifier: String namespace used for any internal measurement keys.
        n_variables: Total number of logical input variables (n >= 2).
        data: Sequence of length 2**n of SymPy expressions giving minterm coefficients.

    Attributes (counts for each half):
        n_variables1 / n_variables2: Split sizes.
        n_minterms1 / n_minterms2: 2**n1, 2**n2.
        n_monomials1 / n_monomials2: Non-empty monomials per half.
        n_aux1 / n_aux2: Auxiliary qubits needed for higher-degree monomials.

    Raises:
        ValueError: If n_variables < 2 or len(data) != 2**n.
    """

    def __init__(self, uniquifier: str, n_variables: int, data: Sequence[Expr]) -> None:
        """Initialize SqrtPhaseup Gate"""
        super().__init__()

        self.uniquifier = uniquifier
        if n_variables < 2:
            raise ValueError(
                "SqrtPhaseupGate: Qubit register length must be at least 2. It is "
                f"{n_variables}"
            )
        self.n_variables = n_variables
        self.data = data

        # First half of the register:
        # Half of the variables (or less if n is odd)
        self.n_variables1 = self.n_variables >> 1
        # 2^(n/2) for all combinations
        self.n_minterms1 = 1 << self.n_variables1
        # No monomial for empty set of variables
        self.n_monomials1 = self.n_minterms1 - 1
        # One aux qubit for each higher degree monomial
        self.n_aux1 = self.n_monomials1 - self.n_variables1

        # Second half of the register:
        # The other half of the variables
        self.n_variables2 = self.n_variables - self.n_variables1
        # 2^(n/2) for all combinations
        self.n_minterms2 = 1 << self.n_variables2
        # No monomial for empty set of variables
        self.n_monomials2 = self.n_minterms2 - 1
        # One aux qubit for each higher degree monomial
        self.n_aux2 = self.n_monomials2 - self.n_variables2

        n_minterms = 1 << n_variables  # overall number of minterms, also data length
        assert self.n_minterms1 * self.n_minterms2 == n_minterms, (
            "Internal error. Unexpected register split."
        )
        assert (
            self.n_monomials1 + self.n_monomials2
            == self.n_variables + self.n_aux1 + self.n_aux2
        ), "Internal error. Unexpected aux qubit counts."
        if len(data) != n_minterms:
            raise ValueError(
                "SqrtPhaseupGate: The length of data array should be "
                f"{n_minterms}. It is {len(data)}"
            )

    def get_aux_count(self) -> int:
        """The number of aux qubits needed to perform SqrtPhaseup operation"""
        return self.n_aux1 + self.n_aux2

    def _num_qubits_(self) -> int:
        """
        Phaseup gate uses as many qubits as the total number of monomials in
        both parts of the split register.
        """
        # number of qubits passed is automatically checked by cirq against this value.
        return self.n_monomials1 + self.n_monomials2

    def _decompose_(self, qubits: Sequence[cirq.Qid]) -> cirq.OP_TREE:
        """
        Decomposes into monomial construction, phasing, and uncomputation for
        each half of the split register, plus cross-phasing.
        """
        # Split passed qubits into two halves of the register and aux qubits
        h1 = qubits[: self.n_variables1]  # First half of the register
        slice_index1 = self.n_variables1 + self.n_variables2
        h2 = qubits[self.n_variables1 : slice_index1]  # Second half of the register
        slice_index2 = slice_index1 + self.n_aux1
        aux1 = qubits[slice_index1:slice_index2]  # Aux qubits for the first half
        aux2 = qubits[slice_index2:]  # Aux qubits for the second half

        # Rearrange qubits in the order of monomials
        monomials1 = DoPowerProductGate.rearrange_qubits(h1, aux1)
        monomials2 = DoPowerProductGate.rearrange_qubits(h2, aux2)

        # Transform classical data from minterm coefficients to polynomial coefficients
        # Rearrange data in the form of a matrix. First index spans n_minterms2 chunks,
        # each of length n_minterms1.
        control_variables = PhaseupGate.boole_mobius_transform(self.data)
        controls_as_matrix = [
            list(control_variables[offset : offset + self.n_minterms1])
            for offset in range(0, len(control_variables), self.n_minterms1)
        ]
        assert controls_as_matrix is not None, (
            "Internal error: failed to rearrange classical data."
        )
        assert len(controls_as_matrix) == self.n_minterms2, (
            "Internal error: wrong number of rows."
        )
        assert len(controls_as_matrix[0]) == self.n_minterms1, (
            "Internal error: wrong number of columns."
        )

        # create required monomials on aux qubits for both halves
        pp1_gate = DoPowerProductGate(self.n_variables1)
        yield pp1_gate.on(*monomials1[1:])
        pp2_gate = DoPowerProductGate(self.n_variables2)
        yield pp2_gate.on(*monomials2[1:])

        # Phasing: Apply Z phasing for the first column of the matrix
        # (except the [0,0] element)
        mt_z_gate1 = MultitargetZGate(
            self.n_monomials2, [row[0] for row in controls_as_matrix][1:]
        )
        yield mt_z_gate1.on(*monomials2[1:])
        # Phasing: Apply Z phasing for the first row of the matrix
        # (except the [0,0] element)
        mt_z_gate2 = MultitargetZGate(self.n_monomials1, controls_as_matrix[0][1:])
        yield mt_z_gate2.on(*monomials1[1:])
        # Phasing: Apply CZ phasing for the rest of the matrix.
        for col in range(1, self.n_minterms1):
            # TODO: consider building transposed matrix above to use slices here
            mt_cz_gate = MultitargetCZGate(
                self.n_monomials2, [row[col] for row in controls_as_matrix][1:]
            )
            yield mt_cz_gate.on(monomials1[col], *monomials2[1:])

        # Uncompute higher-order monomials on aux qubits for both halves
        upp_gate1 = UndoPowerProductGate(self.uniquifier + "_u1", self.n_variables1)
        yield upp_gate1.on(*monomials1[1:])
        upp_gate2 = UndoPowerProductGate(self.uniquifier + "_u2", self.n_variables2)
        yield upp_gate2.on(*monomials2[1:])

    def _circuit_diagram_info_(
        self, args: cirq.CircuitDiagramInfoArgs
    ) -> cirq.CircuitDiagramInfo:
        """Custom diagram labels for the gate"""
        return cirq.CircuitDiagramInfo(
            wire_symbols=("?ZZ",) * self.n_variables
            + ("?&ZZ",) * (self.n_aux1 + self.n_aux2)
        )

    def __repr__(self) -> str:
        """display string: SqrtPhaseup<n_variables>_<uniquifier>"""
        return f"SqrtPhaseup{self.n_variables}_{self.uniquifier}"


# Testing and drawing circuit

def keep_fn(op: cirq.Operation) -> bool:
    # Use structural pattern matching (Python 3.10+) to filter out gate types
    # that should be decomposed away. Return True for all other operations.
    g = getattr(op, "gate", None)
    match g:
        case UndoPowerProductGate():
            return False
        case DoPowerProductGate():
            return False
        case PhaseupGate():
            return False
        case SqrtPhaseupGate():
            return False
        case _:
            return True


n = 3
import sympy

controls = " ".join(f"x{i}" for i in range(1 << n))
data = list(sympy.symbols(controls))
PU = PhaseupGate("s", n, data)

qs = cirq.LineQubit.range(n)
nn = PU.get_aux_count()  # aux count
aux = cirq.LineQubit.range(n, n + nn)

circuit = cirq.Circuit()
op = PU.on(*(qs + aux))
op_d = cirq.decompose(op, keep=keep_fn)
circuit.append(op_d)

# Step 3: Print the circuit
print("Quantum Circuit:")
print(circuit)
print(circuit.to_qasm(version="3.0"))
# print(op_d)





    # Implementation via multi-controlled X

    # anc = qubit_manager.qalloc(1)[0]
    # yield from phase_lookup_via_multicontrolled_x(nrows, address, anc, conditions)
    # qubit_manager.qfree([anc])
