# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Pauli operator representation for quantum spin systems."""

from collections.abc import Sequence

try:
    import cirq
except Exception as ex:
    raise ImportError(
        "qsharp.magnets.models requires the cirq extras. Install with 'pip install \"qsharp[cirq]\"'."
    ) from ex


class Pauli:
    """A single-qubit Pauli operator (I, X, Y, or Z) acting on a specific qubit.

    Can be constructed from an integer (0–3) or a string ('I', 'X', 'Y', 'Z'),
    along with the index of the qubit it acts on.

    Mapping:
        0 / 'I' → Identity
        1 / 'X' → Pauli-X
        2 / 'Z' → Pauli-Z
        3 / 'Y' → Pauli-Y

    Attributes:
        qubit: The qubit index this operator acts on.

    Example:

    .. code-block:: python
        >>> p = Pauli('X', 0)
        >>> p.op
        1
        >>> p.qubit
        0
    """

    _VALID_INTS = {0, 1, 2, 3}
    _STR_TO_INT = {"I": 0, "X": 1, "Z": 2, "Y": 3}

    def __init__(self, value: int | str, qubit: int = 0) -> None:
        """Initialize a Pauli operator.

        Args:
            value: An integer 0–3 or one of 'I', 'X', 'Y', 'Z' (case-insensitive).
            qubit: The index of the qubit this operator acts on. Defaults to 0.

        Raises:
            ValueError: If the value is not a recognized Pauli identifier.
        """
        if isinstance(value, int):
            if value not in self._VALID_INTS:
                raise ValueError(f"Integer value must be 0–3, got {value}.")
            self._op = value
        elif isinstance(value, str):
            key = value.upper()
            if key not in self._STR_TO_INT:
                raise ValueError(
                    f"String value must be one of 'I', 'X', 'Y', 'Z', got '{value}'."
                )
            self._op = self._STR_TO_INT[key]
        else:
            raise ValueError(f"Expected int or str, got {type(value).__name__}.")
        self.qubit: int = qubit

    @property
    def op(self) -> int:
        """Return the integer representation of this Pauli operator.

        Returns:
            0 for I, 1 for X, 2 for Z, 3 for Y.
        """
        return self._op

    def __repr__(self) -> str:
        labels = {0: "I", 1: "X", 2: "Z", 3: "Y"}
        return f"Pauli('{labels[self._op]}', qubit={self.qubit})"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Pauli):
            return NotImplemented
        return self._op == other._op and self.qubit == other.qubit

    def __hash__(self) -> int:
        return hash((self._op, self.qubit))

    @property
    def cirq(self):
        """Return the corresponding Cirq Pauli operator.

        Returns:
            ``cirq.I``, ``cirq.X``, ``cirq.Z``, or ``cirq.Y``.
        """
        _INT_TO_CIRQ = (cirq.I, cirq.X, cirq.Z, cirq.Y)
        return _INT_TO_CIRQ[self._op].on(cirq.LineQubit(self.qubit))


def PauliX(qubit: int) -> Pauli:
    """Create a Pauli-X operator on the given qubit."""
    return Pauli("X", qubit)


def PauliY(qubit: int) -> Pauli:
    """Create a Pauli-Y operator on the given qubit."""
    return Pauli("Y", qubit)


def PauliZ(qubit: int) -> Pauli:
    """Create a Pauli-Z operator on the given qubit."""
    return Pauli("Z", qubit)


class PauliString:
    """A multi-qubit Pauli operator acting on specific qubits.

    Stores a tuple of :class:`Pauli` objects, each carrying its own qubit index.
    Can be constructed from a sequence of ``Pauli`` instances (default), or via
    the :meth:`from_qubits` class method which takes qubit indices and Pauli
    labels separately.

    Attributes:
        _paulis: Tuple of Pauli objects defining the operator on each qubit.

    Example:

    .. code-block:: python
        >>> ps = PauliString([PauliX(0), PauliZ(1)])
        >>> ps.qubits
        (0, 1)
        >>> list(ps)
        [Pauli(X, qubit=0), Pauli(Z, qubit=1)]
        >>> ps2 = PauliString.from_qubits((0, 1), "XZ")
        >>> ps == ps2
        True
    """

    def __init__(self, paulis: Sequence[Pauli]) -> None:
        """Initialize a PauliString from a sequence of Pauli operators.

        Args:
            paulis: A sequence of :class:`Pauli` instances, each with its
                own qubit index.

        Raises:
            TypeError: If any element is not a Pauli instance.
        """
        for p in paulis:
            if not isinstance(p, Pauli):
                raise TypeError(
                    f"Expected Pauli instance, got {type(p).__name__}. "
                    "Use PauliString.from_qubits() for int/str values."
                )
        self._paulis: tuple[Pauli, ...] = tuple(paulis)

    @classmethod
    def from_qubits(
        cls,
        qubits: tuple[int, ...],
        values: Sequence[int | str] | str,
    ) -> "PauliString":
        """Create a PauliString from qubit indices and Pauli labels.

        Args:
            qubits: Tuple of qubit indices.
            values: Sequence of Pauli identifiers (integers 0–3 or strings
                'I', 'X', 'Y', 'Z'). A plain string like ``"XZI"`` is also
                accepted and treated as individual characters.

        Returns:
            A new PauliString instance.

        Raises:
            ValueError: If qubits and values have different lengths, or if
                any value is not a valid Pauli identifier.
        """
        if len(qubits) != len(values):
            raise ValueError(
                f"Length mismatch: {len(qubits)} qubits vs {len(values)} values."
            )
        paulis = [Pauli(v, q) for q, v in zip(qubits, values)]
        return cls(paulis)

    @property
    def qubits(self) -> tuple[int, ...]:
        """Return the tuple of qubit indices.

        Returns:
            Tuple of qubit indices, one per Pauli operator.
        """
        return tuple(p.qubit for p in self._paulis)

    def __iter__(self):
        """Iterate over the Pauli operators in this PauliString.

        Yields:
            :class:`Pauli` instances in order.
        """
        return iter(self._paulis)

    def __len__(self) -> int:
        return len(self._paulis)

    def __getitem__(self, index: int) -> Pauli:
        return self._paulis[index]

    def __repr__(self) -> str:
        labels = {0: "I", 1: "X", 2: "Z", 3: "Y"}
        s = "".join(labels[p.op] for p in self._paulis)
        return f"PauliString(qubits={self.qubits}, ops='{s}')"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, PauliString):
            return NotImplemented
        return self._paulis == other._paulis

    def __hash__(self) -> int:
        return hash(self._paulis)

    @property
    def cirq(self):
        """Return the corresponding Cirq ``PauliString``.

        Constructs a ``cirq.PauliString`` by applying each single-qubit
        Pauli to its corresponding ``cirq.LineQubit``.

        Returns:
            A ``cirq.PauliString`` acting on ``cirq.LineQubit`` instances.
        """
        _INT_TO_CIRQ = (cirq.I, cirq.X, cirq.Z, cirq.Y)
        return cirq.PauliString(
            {cirq.LineQubit(p.qubit): _INT_TO_CIRQ[p.op] for p in self._paulis}
        )
