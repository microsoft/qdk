# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Majorana fermion representations for many-body systems."""

from collections.abc import Sequence


class Majorana:
    """Single-mode Majorana operator tied to an explicit site index.

    ``Majorana`` stores a Majorana operator identifier and the site it acts on.
    The identifier can be provided either as an integer code or a label:

    - ``12`` / ``"G"``
    - ``13`` / ``"G'"``

    Example:

    .. code-block:: python
        >>> g = Majorana("G'", site=2)
        >>> g.op
        13
        >>> g.site
        2
    """

    _VALID_INTS = {12, 13}
    _STR_TO_INT = {
        "G": 12,
        "G'": 13,
    }
    _INT_TO_LABEL = {12: "G", 13: "G'"}

    def __init__(self, value: int | str, site: int = 0) -> None:
        """Initialize a Majorana operator.

        Args:
            value: An integer 12-13 or one of 'G', 'G''.
            site: The index of the site this operator acts on. Defaults to 0.

        Raises:
            ValueError: If ``value`` is not a valid integer/string Majorana identifier.
        """
        if isinstance(value, int):
            if value not in self._VALID_INTS:
                raise ValueError(f"Integer value must be 12 or 13, got {value}.")
            self._op = value
        elif isinstance(value, str):
            key = value.upper()
            if key not in self._STR_TO_INT:
                raise ValueError(f"String value must be one of 'G', \"G'\", got '{value}'.")
            self._op = self._STR_TO_INT[key]
        else:
            raise ValueError(f"Expected int or str, got {type(value).__name__}.")
        self.site: int = site

    @property
    def op(self) -> int:
        """Integer encoding of this Majorana term."""
        return self._op

    def __str__(self) -> str:
        return f"{self._INT_TO_LABEL[self._op]}({self.site})"

    def __repr__(self) -> str:
        return f"Majorana('{self._INT_TO_LABEL[self._op]}', site={self.site})"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Majorana):
            return NotImplemented
        return self._op == other._op and self.site == other.site

    def __hash__(self) -> int:
        return hash((self._op, self.site))


def MajoranaFermion(site: int) -> Majorana:
    """Create a Majorana fermion on the given site."""
    return Majorana("G", site)


def MajoranaDualFermion(site: int) -> Majorana:
    """Create a dual Majorana fermion on the given site."""
    return Majorana("G'", site)


class MajoranaString:
    """Ordered product of single-site ``Majorana`` terms with a coefficient.

    ``MajoranaString`` stores:

    - an ordered tuple of :class:`Majorana` objects (including each term's site), and
    - a complex scalar coefficient.

    Construction options:

    - pass a sequence of :class:`Majorana` objects to ``MajoranaString(...)``
    - use :meth:`from_sites` to pair site indices with Majorana labels or codes

    Example:

    .. code-block:: python
        >>> ms = MajoranaString([MajoranaFermion(0), MajoranaDualFermion(1)], coefficient=-1j)
        >>> ms.sites
        (0, 1)
        >>> ms2 = MajoranaString.from_sites((0, 1), ["G", "G'"], coefficient=-1j)
        >>> ms == ms2
        True
    """

    def __init__(self, majoranas: Sequence[Majorana], coefficient: complex = 1.0) -> None:
        """Initialize a MajoranaString from a sequence of Majorana operators.

        Args:
            majoranas: A sequence of :class:`Majorana` instances, each with its
                own site index.
            coefficient: Complex coefficient multiplying the Majorana string.

        Raises:
            TypeError: If any element is not a Majorana instance.
        """
        for majorana in majoranas:
            if not isinstance(majorana, Majorana):
                raise TypeError(
                    f"Expected Majorana instance, got {type(majorana).__name__}. "
                    "Use MajoranaString.from_sites() for int/str values."
                )
        self._majoranas: tuple[Majorana, ...] = tuple(majoranas)
        self._coefficient: complex = coefficient

    @classmethod
    def from_sites(
        cls,
        sites: tuple[int, ...],
        values: Sequence[int | str],
        coefficient: complex = 1.0,
    ) -> "MajoranaString":
        """Create a MajoranaString from site indices and Majorana labels.

        Args:
            sites: Tuple of site indices.
            values: Sequence of Majorana identifiers (integers 12-13 or strings
                like 'G' and "G'").
            coefficient: Complex coefficient multiplying the Majorana string.

        Returns:
            A new MajoranaString instance.

        Raises:
            ValueError: If sites and values have different lengths, or if
                any value is not a valid Majorana identifier.
        """
        if len(sites) != len(values):
            raise ValueError(
                f"Length mismatch: {len(sites)} sites vs {len(values)} values."
            )
        majoranas = [Majorana(value, site) for site, value in zip(sites, values)]
        return cls(majoranas, coefficient=coefficient)

    @property
    def sites(self) -> tuple[int, ...]:
        """Tuple of site indices in the same order as the stored Majorana terms."""
        return tuple(majorana.site for majorana in self._majoranas)

    @property
    def coefficient(self) -> complex:
        """Complex coefficient multiplying this Majorana string."""
        return self._coefficient

    @property
    def majoranas(self) -> tuple[str, ...]:
        """Tuple of canonical Majorana labels in stored order."""
        return tuple(Majorana._INT_TO_LABEL[majorana.op] for majorana in self._majoranas)

    def __iter__(self):
        """Iterate over Majorana terms in stored order."""
        return iter(self._majoranas)

    def __len__(self) -> int:
        return len(self._majoranas)

    def __getitem__(self, index: int) -> Majorana:
        return self._majoranas[index]

    def __mul__(self, scalar: complex) -> "MajoranaString":
        """Scale the coefficient of this MajoranaString by a complex scalar."""
        return MajoranaString(self._majoranas, coefficient=self._coefficient * scalar)

    def normalize(self) -> None:
        """Normalize this Majorana string in place.

        The normalization performs two operations using adjacent swaps only:

        - reorder terms into ascending site order with the convention ``G[j] < G'[j]``,
        - cancel adjacent equal Majorana terms using ``G[j]G[j] = I`` and
          ``G'[j]G'[j] = I``.

        Every swap of adjacent distinct Majorana terms flips the sign of the
        coefficient.
        """

        def order_key(majorana: Majorana) -> tuple[int, int]:
            return (majorana.site, 0 if majorana.op == 12 else 1)

        majoranas = list(self._majoranas)
        coefficient = self._coefficient
        index = 0

        while index < len(majoranas) - 1:
            if majoranas[index] == majoranas[index + 1]:
                del majoranas[index : index + 2]
                index = max(index - 1, 0)
                continue

            if order_key(majoranas[index]) > order_key(majoranas[index + 1]):
                majoranas[index], majoranas[index + 1] = (
                    majoranas[index + 1],
                    majoranas[index],
                )
                coefficient *= -1
                index = max(index - 1, 0)
                continue

            index += 1

        self._majoranas = tuple(majoranas)
        self._coefficient = coefficient

    def __str__(self) -> str:
        return f"{self._coefficient} * {''.join(map(str, self._majoranas))}"

    def __repr__(self) -> str:
        ops = list(self.majoranas)
        return (
            f"MajoranaString(sites={self.sites}, ops={ops}, "
            f"coefficient={self._coefficient})"
        )

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, MajoranaString):
            return NotImplemented
        return self._majoranas == other._majoranas and self._coefficient == other._coefficient

    def __hash__(self) -> int:
        return hash((self._majoranas, self._coefficient))


def vertex_operator(j: int) -> MajoranaString:
    """Create the vertex operator :math:`i G_j G'_j`.

    Args:
        j: Site index of both Majorana operators.

    Returns:
        A Majorana string representing ``i * G[j] * G'[j]``.
    """
    return MajoranaString([MajoranaFermion(j), MajoranaDualFermion(j)]) * 1j


def edge_operator(j: int, k: int) -> MajoranaString:
    """Create the edge operator :math:`i G_j G_k`.

    Args:
        j: Site index of the first Majorana operator.
        k: Site index of the second Majorana operator.

    Returns:
        A Majorana string representing ``i * G[j] * G[k]``.
    """
    return MajoranaString([MajoranaFermion(j), MajoranaFermion(k)]) * 1j
