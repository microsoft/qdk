"""Bounded distance values and lazy, replayable failure witnesses."""

from __future__ import annotations

from collections.abc import Callable, Iterator
from copy import deepcopy
from dataclasses import dataclass
from typing import Generic, TypeVar

Factor = TypeVar("Factor")
WitnessFactor = TypeVar("WitnessFactor")


@dataclass(frozen=True, init=False, repr=False, eq=False, slots=True)
class Distance(Generic[Factor]):
    """Certified bounds with witnesses establishing the finite upper bound.

    Both bounds use None for positive infinity. Equal bounds are exact,
    including (None, None), which proves no allowed logical failure exists.
    Comparisons with integers or other distances use only these bounds and
    raise ValueError when unresolved. Comparisons never run a search.

    Obtain results from CodeProfile or GadgetProfile. The search problem is
    snapshotted; consuming witnesses does not refine the bounds.
    """

    _lower_bound: int | None
    _upper_bound: int | None
    _witness: Distance.Witness[Factor] | None
    _alternatives: Callable[[], Iterator[Distance.Witness[Factor]]]

    def __new__(cls) -> Distance[Factor]:
        raise TypeError("Distance is returned by profile distance searches")

    @classmethod
    def _create(
        cls,
        lower_bound: int | None,
        upper_bound: int | None,
        *,
        witness: Distance.Witness[Factor] | None = None,
        alternatives: Callable[[], Iterator[Distance.Witness[Factor]]] = lambda: iter(
            ()
        ),
    ) -> Distance[Factor]:
        for bound in (lower_bound, upper_bound):
            if bound is not None and (
                not isinstance(bound, int) or isinstance(bound, bool) or bound < 0
            ):
                raise ValueError("distance bounds must be nonnegative integers or None")
        if upper_bound is not None and (
            lower_bound is None or lower_bound > upper_bound
        ):
            raise ValueError("distance lower bound exceeds its upper bound")
        if (upper_bound is None) != (witness is None):
            raise ValueError("a finite upper bound requires a witness")
        if witness is not None and len(witness._factors) != upper_bound:
            raise ValueError("witness factor count differs from the upper bound")
        result = object.__new__(cls)
        object.__setattr__(result, "_lower_bound", lower_bound)
        object.__setattr__(result, "_upper_bound", upper_bound)
        object.__setattr__(result, "_witness", witness)
        object.__setattr__(result, "_alternatives", alternatives)
        return result

    @property
    def lower_bound(self) -> int | None:
        """Certified lower bound; None means proven infinite distance."""
        return self._lower_bound

    @property
    def upper_bound(self) -> int | None:
        """Witness factor count, or None when no finite bound is established."""
        return self._upper_bound

    @property
    def is_exact(self) -> bool:
        return self._lower_bound == self._upper_bound

    @property
    def value(self) -> int | None:
        """Exact distance, with None for infinity; raise ValueError if unresolved."""
        if not self.is_exact:
            raise ValueError(f"distance is not exact: {self}")
        return self._lower_bound

    @property
    def witness(self) -> Distance.Witness[Factor]:
        """Return a retained witness without searching, or raise LookupError."""
        if self._witness is None:
            raise LookupError("no finite-distance witness is available")
        return self._witness

    @property
    def witnesses(self) -> Iterator[Distance.Witness[Factor]]:
        """A fresh iterator, yielding the retained witness before lazy alternatives.

        Every selection has the reported upper-bound cost. Selections differ
        by original allowed-factor positions, not their order or product.
        Exhaustion means all such selections were enumerated; failures raise.
        An infinite upper bound gives an empty iterator. Enumeration can be
        combinatorially expensive and never changes the result's bounds.
        """
        if self._witness is not None:
            yield self._witness
            yield from self._alternatives()

    def __str__(self) -> str:
        lower = "\u221e" if self._lower_bound is None else str(self._lower_bound)
        upper = "\u221e" if self._upper_bound is None else str(self._upper_bound)
        return lower if self.is_exact else f"[{lower}, {upper}]"

    def __repr__(self) -> str:
        return (
            f"Distance(lower_bound={self._lower_bound!r}, "
            f"upper_bound={self._upper_bound!r}, witness={self._witness!r})"
        )

    def __format__(self, format_spec: str) -> str:
        return format(str(self), format_spec)

    def __bool__(self) -> bool:
        raise TypeError("use is_exact or an explicit distance comparison")

    def _compare(self, other: object, operation: str) -> bool:
        if isinstance(other, Distance):
            other_lower, other_upper = other.lower_bound, other.upper_bound
        elif isinstance(other, int) and not isinstance(other, bool):
            other_lower = other_upper = other
        else:
            return NotImplemented
        lower, upper = self._lower_bound, self._upper_bound
        if operation == "eq":
            if self.is_exact and other_lower == other_upper:
                return lower == other_lower
            if _less(upper, other_lower) or _less(other_upper, lower):
                return False
        elif operation == "lt":
            if _less(upper, other_lower):
                return True
            if not _less(lower, other_upper):
                return False
        elif operation == "le":
            if not _less(other_lower, upper):
                return True
            if _less(other_upper, lower):
                return False
        raise ValueError(f"comparison is unresolved for {self} and {other}")

    def __eq__(self, other: object) -> bool:
        return self._compare(other, "eq")

    def __ne__(self, other: object) -> bool:
        equal = self._compare(other, "eq")
        return NotImplemented if equal is NotImplemented else not equal

    def __lt__(self, other: object) -> bool:
        return self._compare(other, "lt")

    def __le__(self, other: object) -> bool:
        return self._compare(other, "le")

    def __gt__(self, other: object) -> bool:
        less_equal = self._compare(other, "le")
        return NotImplemented if less_equal is NotImplemented else not less_equal

    def __ge__(self, other: object) -> bool:
        less = self._compare(other, "lt")
        return NotImplemented if less is NotImplemented else not less

    @dataclass(frozen=True, init=False, repr=False, eq=False, slots=True)
    class Witness(Generic[WitnessFactor]):
        """A selection of allowed factors establishing the distance's upper bound.

        Factors are ordered for multiplication. Equality and hashing use that
        ordered tuple, not just the product. Neither proves minimality.
        Returned factors and products are copies of the retained snapshot.
        ``str`` joins the readable factors with semicolons, without repeating
        the product; an empty selection is displayed as ``1``.
        """

        _factors: tuple[WitnessFactor, ...]
        _product: WitnessFactor
        _copy: Callable[[WitnessFactor], WitnessFactor]

        def __new__(cls) -> Distance.Witness[WitnessFactor]:
            raise TypeError("Distance.Witness is returned by distance results")

        @classmethod
        def _create(
            cls,
            factors: tuple[WitnessFactor, ...],
            *,
            product: Callable[[tuple[WitnessFactor, ...]], WitnessFactor],
            copy: Callable[[WitnessFactor], WitnessFactor] = deepcopy,
        ) -> Distance.Witness[WitnessFactor]:
            snapshot = tuple(copy(factor) for factor in factors)
            witness = object.__new__(cls)
            object.__setattr__(witness, "_factors", snapshot)
            object.__setattr__(
                witness, "_product", product(tuple(copy(factor) for factor in snapshot))
            )
            object.__setattr__(witness, "_copy", copy)
            return witness

        @property
        def factors(self) -> tuple[WitnessFactor, ...]:
            return tuple(self._copy(factor) for factor in self._factors)

        @property
        def product(self) -> WitnessFactor:
            return self._copy(self._product)

        def __str__(self) -> str:
            return "; ".join(map(str, self._factors)) or "1"

        def __repr__(self) -> str:
            return f"Distance.Witness(factors={self._factors!r})"

        def __eq__(self, other: object) -> bool:
            if not isinstance(other, Distance.Witness):
                return NotImplemented
            return self._factors == other._factors

        def __ne__(self, other: object) -> bool:
            equal = self.__eq__(other)
            return NotImplemented if equal is NotImplemented else not equal

        def __hash__(self) -> int:
            return hash(self._factors)


def _less(left: int | None, right: int | None) -> bool:
    return left is not None and (right is None or left < right)


__all__ = ["Distance"]
