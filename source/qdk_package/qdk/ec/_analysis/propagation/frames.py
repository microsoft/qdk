"""Outcome-conditioned Pauli frame result types."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Iterable, Mapping, Sequence

from paulimer import PauliGroup

from .groups import rank_extension_of, restriction_indicator_basis_of
from .pauli import Pauli, identity


@dataclass(frozen=True, repr=False)
class PauliFrame:
    """A Pauli and the measurement outcomes that condition its sign."""

    pauli: Pauli
    frame: frozenset[int] = frozenset()

    def __mul__(self, other: object) -> "PauliFrame":
        if isinstance(other, PauliFrame):
            return PauliFrame(self.pauli * other.pauli, self.frame ^ other.frame)
        if isinstance(other, Pauli):
            return PauliFrame(self.pauli * other, self.frame)
        if isinstance(other, (int, float, complex)):
            return PauliFrame(self.pauli * identity(other), self.frame)
        return NotImplemented

    def __abs__(self) -> "PauliFrame":
        return PauliFrame(abs(self.pauli), self.frame)

    def __str__(self) -> str:
        if not self.frame:
            return str(self.pauli)
        outcomes = ",".join(str(index) for index in sorted(self.frame))
        return f"{self.pauli}^{{{outcomes}}}"

    def __repr__(self) -> str:
        return self.__str__()


@dataclass(frozen=True)
class FrameGroup:
    """An ordered set of frame-aware Pauli generators."""

    generators: tuple[PauliFrame, ...]

    def __init__(self, generators: Iterable[PauliFrame]) -> None:
        object.__setattr__(self, "generators", tuple(generators))

    @property
    def unframed(self) -> PauliGroup:
        return PauliGroup([framed.pauli for framed in self.generators])

    def __or__(self, other: "FrameGroup") -> "FrameGroup":
        return FrameGroup(self.generators + other.generators)

    def _element(self, indicator: Sequence[int]) -> PauliFrame:
        element = PauliFrame(Pauli.identity())
        for bit, framed in zip(indicator, self.generators):
            if bit:
                element = element * framed
        return element

    def subgroup(self, indicators: Iterable[Sequence[int]]) -> "FrameGroup":
        return FrameGroup(self._element(indicator) for indicator in indicators)

    def partition(
        self, *, over: Iterable[int]
    ) -> tuple["FrameGroup", "FrameGroup", "FrameGroup"]:
        operators = self.unframed
        over_set = set(over)
        support = set(operators.support)
        primary = list(restriction_indicator_basis_of(operators, supported_by=over_set))
        complementary = list(
            restriction_indicator_basis_of(operators, supported_by=support - over_set)
        )
        identity_indicator = [0] * len(self.generators)
        extension = rank_extension_of(primary + complementary + [identity_indicator])
        return (
            self.subgroup(primary),
            self.subgroup(complementary),
            self.subgroup(extension),
        )

    def standardized(self) -> "FrameGroup":
        return _carry_frames(
            self.generators,
            lambda tagged: PauliGroup(tagged).standard_generators,
        )

    def __mod__(self, modulus: "FrameGroup") -> "FrameGroup":
        combined = self.generators + modulus.generators
        offset = len(self.generators)

        def reduce(tagged: list[Pauli]) -> Sequence[Pauli]:
            left = PauliGroup(tagged[:offset])
            right = PauliGroup(tagged[offset:])
            return (left % right).generators

        return _carry_frames(combined, reduce)

    def relabel(self, mapping: Mapping[int, int]) -> "FrameGroup":
        def remap(pauli: Pauli) -> Pauli:
            return Pauli(
                {mapping.get(qubit, qubit): pauli[qubit] for qubit in pauli.support}
            ) * identity(pauli.phase)

        return FrameGroup(
            PauliFrame(remap(framed.pauli), framed.frame) for framed in self.generators
        )

    def restrict_to(self, support: Iterable[int]) -> "FrameGroup":
        support_set = frozenset(support)

        def restrict(pauli: Pauli) -> Pauli:
            kept = {qubit: pauli[qubit] for qubit in set(pauli.support) & support_set}
            return Pauli(kept) * identity(pauli.phase)

        return FrameGroup(
            PauliFrame(restrict(framed.pauli), framed.frame)
            for framed in self.generators
        )

    def complex_conjugated(self) -> "FrameGroup":
        def conjugate(pauli: Pauli) -> Pauli:
            y_count = sum(1 for qubit in pauli.support if pauli[qubit] == "Y")
            return pauli * identity(-1) if y_count % 2 else pauli

        return FrameGroup(
            PauliFrame(conjugate(framed.pauli), framed.frame)
            for framed in self.generators
        )

    def factorization_of(self, target: Pauli) -> list[PauliFrame] | None:
        factors = self.unframed.factorization_of(target)
        if factors is None:
            return None
        frame_of = {framed.pauli: framed.frame for framed in self.generators}
        return [PauliFrame(factor, frame_of[factor]) for factor in factors]

    def frame_of(self, target: Pauli) -> frozenset[int]:
        factors = self.factorization_of(target)
        if factors is None:
            raise ValueError(f"{target!r} is not in this group")
        frame: frozenset[int] = frozenset()
        for factored in factors:
            frame ^= factored.frame
        return frame


def _carry_frames(
    framed: Sequence[PauliFrame],
    transform: Callable[[list[Pauli]], Sequence[Pauli]],
) -> FrameGroup:
    operators = [item.pauli for item in framed]
    base = (
        max(
            (qubit for operator in operators for qubit in operator.support),
            default=-1,
        )
        + 1
    )
    tagged = [
        operator * Pauli({base + index: "Z"})
        for index, operator in enumerate(operators)
    ]
    recovered: list[PauliFrame] = []
    for result in transform(tagged):
        sources = [qubit - base for qubit in result.support if qubit >= base]
        clean = Pauli(
            {qubit: result[qubit] for qubit in result.support if qubit < base}
        ) * identity(result.phase)
        frame: frozenset[int] = frozenset()
        for index in sources:
            frame ^= framed[index].frame
        recovered.append(PauliFrame(clean, frame))
    return FrameGroup(recovered)


__all__ = ["FrameGroup", "PauliFrame"]
