# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Fermionic operator representations for many-body systems."""

from collections.abc import Sequence


class Fermion:
    """Single-mode fermionic operator tied to an explicit site index.

    ``Fermion`` stores a fermionic operator identifier and the site it acts on.
    The identifier can be provided either as an integer code or a label:

    - ``10`` / ``"A^"`` / ``"CREATE"`` / ``"CREATION"``
    - ``11`` / ``"A"`` / ``"ANNIHILATE"`` / ``"ANNIHILATION"``

    The annihilation operator ``A`` is treated as primitive, and the creation
    operator ``A^`` denotes its Hermitian conjugate.

    Example:

    .. code-block:: python
        >>> f = Fermion("A^", site=2)
        >>> f.op
        10
        >>> f.site
        2
    """

    _VALID_INTS = {10, 11}
    _STR_TO_INT = {
        "A^": 10,
        "CREATE": 10,
        "CREATION": 10,
        "A": 11,
        "ANNIHILATE": 11,
        "ANNIHILATION": 11,
    }
    _INT_TO_LABEL = {10: "A^", 11: "A"}

    def __init__(self, value: int | str, site: int = 0) -> None:
        """Initialize a fermionic operator.

        Args:
            value: An integer 10-11 or a creation/annihilation label.
            site: The index of the site this operator acts on. Defaults to 0.

        Raises:
            ValueError: If ``value`` is not a valid integer/string fermion identifier.
        """
        if isinstance(value, int):
            if value not in self._VALID_INTS:
                raise ValueError(f"Integer value must be 10 or 11, got {value}.")
            self._op = value
        elif isinstance(value, str):
            key = value.upper()
            if key not in self._STR_TO_INT:
                raise ValueError(
                    "String value must be one of 'A^', 'CREATE', 'CREATION', "
                    f"'A', 'ANNIHILATE', 'ANNIHILATION', got '{value}'."
                )
            self._op = self._STR_TO_INT[key]
        else:
            raise ValueError(f"Expected int or str, got {type(value).__name__}.")
        self.site: int = site

    @property
    def op(self) -> int:
        """Integer encoding of this fermionic term."""
        return self._op

    def __str__(self) -> str:
        return f"{self._INT_TO_LABEL[self._op]}({self.site})"

    def __repr__(self) -> str:
        return f"Fermion('{self._INT_TO_LABEL[self._op]}', site={self.site})"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Fermion):
            return NotImplemented
        return self._op == other._op and self.site == other.site

    def __hash__(self) -> int:
        return hash((self._op, self.site))


def FermionCreation(site: int) -> Fermion:
    """Create a fermionic creation operator on the given site."""
    return Fermion("A^", site)


def FermionAnnihilation(site: int) -> Fermion:
    """Create a fermionic annihilation operator on the given site."""
    return Fermion("A", site)


class FermionString:
    """Ordered product of single-site ``Fermion`` terms with a coefficient.

    ``FermionString`` stores:

    - an ordered tuple of :class:`Fermion` objects (including each term's site), and
    - a complex scalar coefficient.

    Construction options:

    - pass a sequence of :class:`Fermion` objects to ``FermionString(...)``
    - use :meth:`from_sites` to pair site indices with fermion labels or codes

    Example:

    .. code-block:: python
        >>> fs = FermionString([FermionCreation(0), FermionAnnihilation(1)], coefficient=-1j)
        >>> fs.sites
        (0, 1)
        >>> fs2 = FermionString.from_sites((0, 1), ["A^", "A"], coefficient=-1j)
        >>> fs == fs2
        True
    """

    def __init__(self, fermions: Sequence[Fermion], coefficient: complex = 1.0) -> None:
        """Initialize a FermionString from a sequence of Fermion operators.

        Args:
            fermions: A sequence of :class:`Fermion` instances, each with its
                own site index.
            coefficient: Complex coefficient multiplying the fermion string.

        Raises:
            TypeError: If any element is not a Fermion instance.
        """
        for fermion in fermions:
            if not isinstance(fermion, Fermion):
                raise TypeError(
                    f"Expected Fermion instance, got {type(fermion).__name__}. "
                    "Use FermionString.from_sites() for int/str values."
                )
        self._fermions: tuple[Fermion, ...] = tuple(fermions)
        self._coefficient: complex = coefficient

    @classmethod
    def from_sites(
        cls,
        sites: tuple[int, ...],
        values: Sequence[int | str],
        coefficient: complex = 1.0,
    ) -> "FermionString":
        """Create a FermionString from site indices and fermion labels.

        Args:
            sites: Tuple of site indices.
            values: Sequence of fermion identifiers (integers 10-11 or strings
                like 'A^' and 'A').
            coefficient: Complex coefficient multiplying the fermion string.

        Returns:
            A new FermionString instance.

        Raises:
            ValueError: If sites and values have different lengths, or if
                any value is not a valid fermion identifier.
        """
        if len(sites) != len(values):
            raise ValueError(
                f"Length mismatch: {len(sites)} sites vs {len(values)} values."
            )
        fermions = [Fermion(value, site) for site, value in zip(sites, values)]
        return cls(fermions, coefficient=coefficient)

    @property
    def sites(self) -> tuple[int, ...]:
        """Tuple of site indices in the same order as the stored Fermion terms."""
        return tuple(fermion.site for fermion in self._fermions)

    @property
    def coefficient(self) -> complex:
        """Complex coefficient multiplying this fermion string."""
        return self._coefficient

    @property
    def fermions(self) -> tuple[str, ...]:
        """Tuple of canonical fermion labels in stored order."""
        return tuple(Fermion._INT_TO_LABEL[fermion.op] for fermion in self._fermions)

    def __iter__(self):
        """Iterate over Fermion terms in stored order."""
        return iter(self._fermions)

    def __len__(self) -> int:
        return len(self._fermions)

    def __getitem__(self, index: int) -> Fermion:
        return self._fermions[index]

    def __mul__(self, scalar: complex) -> "FermionString":
        """Scale the coefficient of this FermionString by a complex scalar."""
        return FermionString(self._fermions, coefficient=self._coefficient * scalar)

    def hermitian_conjugate(self) -> "FermionString":
        """Return the Hermitian conjugate of this fermion string.

        Returns:
            A new ``FermionString`` with reversed operator order, each
            annihilation/creation operator swapped with its conjugate, and the
            coefficient complex-conjugated.
        """
        conjugated_ops = [
            Fermion(10 if fermion.op == 11 else 11, fermion.site)
            for fermion in reversed(self._fermions)
        ]
        return FermionString(conjugated_ops, coefficient=self._coefficient.conjugate())

    def __str__(self) -> str:
        return f"{self._coefficient} * {''.join(map(str, self._fermions))}"

    def __repr__(self) -> str:
        ops = list(self.fermions)
        return (
            f"FermionString(sites={self.sites}, ops={ops}, "
            f"coefficient={self._coefficient})"
        )

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, FermionString):
            return NotImplemented
        return self._fermions == other._fermions and self._coefficient == other._coefficient

    def __hash__(self) -> int:
        return hash((self._fermions, self._coefficient))


def hopping_term(j: int, k: int) -> FermionString:
    """Create the hopping term :math:`A^_j A_k`.

    Args:
        j: Site index of the creation operator.
        k: Site index of the annihilation operator.

    Returns:
        A fermion string representing ``A^[j] * A[k]``.

    Note:
        Setting ``j = k`` yields the number operator on that site.
    """
    return FermionString([FermionCreation(j), FermionAnnihilation(k)])
