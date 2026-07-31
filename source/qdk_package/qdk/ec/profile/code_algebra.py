"""Algebraic view used to profile qodec code definitions."""

from __future__ import annotations

from functools import cached_property
from itertools import chain, product
from typing import Any, Callable, Iterable, Mapping, Optional, Sequence, TYPE_CHECKING

from binar import BitMatrix
from more_itertools import chunked, interleave, take
from paulimer import (
    CliffordUnitary,
    DensePauli,
    PauliGroup,
    centralizer_of,
    symplectic_form_of,
)

from .propagation.groups import is_stabilizer_group
from .propagation.pauli import Pauli, as_literals, characters_of, identity

if TYPE_CHECKING:
    import qodec


class SubsystemCode:  # pylint: disable=too-many-public-methods
    """Internal algebraic interpretation of a qodec code."""

    qodec_name: str | None = None
    qodec_description: str | None = None

    @staticmethod
    def standard_basis(over: Iterable[int] = ()) -> Sequence[Pauli]:
        basis = []
        for index in over:
            basis += [Pauli({index: "X"}), Pauli({index: "Z"})]
        return basis

    @classmethod
    def from_qodec(cls, code: "qodec.Code") -> "SubsystemCode":
        stabilizers = [Pauli(text) for text in code.stabilizers]
        logical_basis = [
            Pauli(str(text))
            for x_operator, z_operator in zip(list(code.x), list(code.z))
            for text in (x_operator, z_operator)
        ]
        gauges = [Pauli(text) for text in getattr(code, "gauges", [])]
        if gauges:
            instance = cls(stabilizers, logical_basis, gauge_basis=gauges)
        else:
            instance = cls(stabilizers, logical_basis)
        instance.qodec_name = code.name
        instance.qodec_description = code.description
        return instance

    def to_qodec(self, name: Optional[str] = None) -> "qodec.Code":
        import qodec

        resolved_name = self.qodec_name or name
        if not resolved_name:
            raise ValueError(
                "Cannot materialize qodec.Code without a name; pass one "
                "explicitly or construct the view from qodec.Code."
            )
        x_strings: list[str] = []
        z_strings: list[str] = []
        for x_operator, z_operator in zip(
            self.logical_basis[0::2], self.logical_basis[1::2]
        ):
            x_strings.append(_format_pauli(x_operator))
            z_strings.append(_format_pauli(z_operator))
        if list(self.gauge.generators):
            raise ValueError(
                "Cannot materialize a subsystem code with gauge operators as "
                "qodec.Code; qodec does not yet model gauge pairs."
            )
        return qodec.Code(
            name=resolved_name,
            description=self.qodec_description or "",
            stabilizers=[_format_pauli(stabilizer) for stabilizer in self.stabilizers],
            x=x_strings,
            z=z_strings,
        )

    def __init__(
        self,
        stabilizers: Sequence[Pauli],
        logical_basis: Sequence[Pauli],
        gauge_basis: Optional[Sequence[Pauli]] = None,
    ) -> None:
        _validate_stabilizers(stabilizers)
        _validate_basis(logical_basis, centralized=stabilizers, name="Logical")
        self._stabilizer = PauliGroup(stabilizers, all_commute=True)
        self._logical = PauliGroup(logical_basis)
        self._support = frozenset(self._stabilizer.support) | frozenset(
            self._logical.support
        )
        if gauge_basis is not None:
            _validate_basis(
                gauge_basis,
                centralized=tuple(stabilizers) + tuple(logical_basis),
                name="Gauge",
            )
            self._support |= frozenset(PauliGroup(gauge_basis).support)
            self.gauge = PauliGroup(gauge_basis)

    @property
    def stabilizer(self) -> PauliGroup:
        return self._stabilizer

    @property
    def stabilizers(self) -> Sequence[Pauli]:
        return self.stabilizer.generators

    @cached_property
    def anti_stabilizer(self) -> PauliGroup:
        return PauliGroup(_anti_stabilizers_of(self), all_commute=True)

    @property
    def anti_stabilizers(self) -> Sequence[Pauli]:
        return self.anti_stabilizer.generators

    @cached_property
    def gauge(self) -> PauliGroup:
        group = PauliGroup(
            logical_basis_of(self._stabilizer, supported_by=tuple(self.support))
        )
        mod_group = (group | self.stabilizer) % (self.stabilizer | self.logical)
        mod_group = PauliGroup(
            normalize(mod_group.generators, with_respect_to_basis=self.logical_basis)
        )
        return PauliGroup(
            abs(generator)
            for generator in symplectic_form_of(mod_group.generators)
            if generator.weight
        )

    @property
    def gauge_basis(self) -> Sequence[Pauli]:
        return self.gauge.generators

    @property
    def logical(self) -> PauliGroup:
        return self._logical

    @property
    def logical_basis(self) -> Sequence[Pauli]:
        return self.logical.generators

    @property
    def support(self) -> frozenset[int]:
        return self._support

    @property
    def length(self) -> int:
        return len(self.support)

    @property
    def logical_qubit_count(self) -> int:
        return len(self.logical_basis) // 2

    def syndrome_of(self, error: Pauli) -> set[int]:
        return {
            label
            for label, generator in enumerate(self.stabilizers)
            if not generator.commutes_with(error)
        }

    def is_trivial_error(self, error: Pauli) -> bool:
        return self.is_logical_error(error) and self.is_trivial_logical_error(error)

    def is_trivial_logical_error(self, error: Pauli) -> bool:
        return all(error.commutes_with(generator) for generator in self.logical_basis)

    def is_logical_error(self, error: Pauli) -> bool:
        return all(error.commutes_with(generator) for generator in self.stabilizers)

    def is_non_trivial_logical_error(self, error: Pauli) -> bool:
        return self.is_logical_error(error) and not self.is_trivial_logical_error(error)

    def logical_action_of(self, error: Pauli) -> Pauli:
        logical = self.unsigned_logical_action_of(error)
        representative = self.representative_of(logical)
        stabilizer = abs(error) * representative
        reduced = (PauliGroup([stabilizer]) % self._stabilizer).generators[0]
        if reduced.weight:
            return logical
        return logical * reduced * identity(error.phase)

    def representative_of(self, pauli: Pauli) -> Pauli:
        if not set(pauli.support) <= frozenset(range(self.logical_qubit_count)):
            raise ValueError(f"Pauli {pauli} has no logical representative.")
        representative = Pauli.identity()
        for index, character in characters_of(pauli).items():
            if character == "X":
                representative *= self.logical_basis[2 * index]
            elif character == "Z":
                representative *= self.logical_basis[2 * index + 1]
            elif character == "Y":
                representative *= (
                    self.logical_basis[2 * index]
                    * self.logical_basis[2 * index + 1]
                    * identity(1j)
                )
        return representative * identity(pauli.phase)

    def unsigned_logical_action_of(self, error: Pauli) -> Pauli:
        if not set(error.support) <= self.support:
            raise ValueError(f"Error {error} is not supported by {self.support}.")
        character_of = ("Y", "Z", "X", "I")
        commutations = map(error.commutes_with, self.logical_basis)
        indexes = [2 * x + z for x, z in chunked(commutations, 2)]
        return Pauli.from_string("".join(character_of[index] for index in indexes))

    def is_equivalent_to(
        self,
        other: "SubsystemCode",
        including_signs: bool = False,
        strict_basis: bool = True,
    ) -> bool:
        if self.support != other.support or not _are_equivalent(
            self.stabilizer,
            other.stabilizer,
            including_signs=including_signs,
        ):
            return False
        if strict_basis:
            return self.logical_basis == other.logical_basis
        return _are_equivalent(
            self.logical, other.logical, including_signs=including_signs
        )

    def relocated(self, by: Mapping[int, int]) -> "SubsystemCode":
        def remap(pauli: Pauli) -> Pauli:
            characters = {by.get(qubit, qubit): pauli[qubit] for qubit in pauli.support}
            return Pauli(characters) * identity(pauli.phase)

        return SubsystemCode(
            [remap(generator) for generator in self.stabilizers],
            [remap(generator) for generator in self.logical_basis],
            gauge_basis=[remap(generator) for generator in self.gauge_basis],
        )

    def __eq__(self, other: object) -> bool:
        return (
            isinstance(other, SubsystemCode)
            and self.support == other.support
            and self.stabilizers == other.stabilizers
            and self.logical_basis == other.logical_basis
            and self.gauge_basis == other.gauge_basis
        )

    def __hash__(self) -> int:
        return hash((self.stabilizers, self.logical_basis))


def anti_commutation_indicator_of(
    observable: Pauli, paulis: Sequence[Pauli]
) -> frozenset[int]:
    return frozenset(
        index
        for index, pauli in enumerate(paulis)
        if not pauli.commutes_with(observable)
    )


def logical_effect_indicators_of(
    code: SubsystemCode, errors: Sequence[Pauli]
) -> list[frozenset[int]]:
    return [
        anti_commutation_indicator_of(error, code.logical_basis) for error in errors
    ]


def syndrome_indicators_of(
    code: SubsystemCode, errors: Sequence[Pauli]
) -> list[frozenset[int]]:
    return [anti_commutation_indicator_of(error, code.stabilizers) for error in errors]


def one_qubit_errors_on_support(code: SubsystemCode, error_kinds: str) -> list[Pauli]:
    return [
        Pauli({qubit: pauli_label})
        for pauli_label in as_literals(error_kinds)
        for qubit in code.support
    ]


def encoding_clifford_of(
    code: SubsystemCode, *, supported_by: Optional[Sequence[int]] = None
) -> CliffordUnitary:
    if supported_by is None:
        supported_by = sorted(code.support)
    elif frozenset(supported_by) != code.support:
        raise ValueError(
            f"Specified support {supported_by} is incomplete (need {code.support})."
        )
    qubit_count = len(supported_by)
    index_of = {qubit: index for index, qubit in enumerate(supported_by)}
    images = []
    for image in clifford_images_of(code):
        remapped = Pauli(
            {index_of[qubit]: image[qubit] for qubit in image.support}
        ) * identity(image.phase)
        images.append(DensePauli.from_sparse(remapped, qubit_count))
    return CliffordUnitary.from_preimages(images).inverse()


def clifford_images_of(code: SubsystemCode) -> Sequence[Pauli]:
    stabilizer_images = interleave(code.anti_stabilizers, code.stabilizers)
    return list(chain(code.logical_basis, code.gauge_basis, stabilizer_images))


def _are_equivalent(
    left: PauliGroup, right: PauliGroup, *, including_signs: bool
) -> bool:
    canonical: Callable[[Pauli], Pauli] = (
        (lambda generator: generator) if including_signs else abs
    )
    return list(map(canonical, left.standard_generators)) == list(
        map(canonical, right.standard_generators)
    )


def _validate_stabilizers(stabilizers: Sequence[Pauli]) -> None:
    if not is_stabilizer_group(PauliGroup(stabilizers)):
        raise ValueError("The provided stabilizer generators are invalid.")


def _format_pauli(pauli: Pauli) -> str:
    return " ".join(f"{pauli[index]}_{index}" for index in sorted(pauli.support))


def _validate_basis(
    logical_basis: Sequence[Pauli],
    *,
    centralized: Sequence[Pauli],
    name: str,
) -> None:
    if not is_symplectic_basis(logical_basis):
        raise ValueError(
            f"{name} elements are not a symplectic basis: "
            f"{why_not_symplectic_basis(logical_basis)}."
        )
    if not _logical_basis_centralizes(logical_basis, centralized):
        raise ValueError(
            f"{name} basis elements do not commute with the complementary space."
        )


def _validate_anti_stabilizers(
    anti_stabilizers: Sequence[Pauli],
    stabilizers: Sequence[Pauli],
    logical_basis: Sequence[Pauli],
) -> None:
    if len(anti_stabilizers) != len(stabilizers):
        raise ValueError(
            f"Anti-stabilizer count ({len(anti_stabilizers)}) does not match "
            f"stabilizer count ({len(stabilizers)})"
        )
    interleaved = list(chain(*zip(stabilizers, anti_stabilizers)))
    if not is_symplectic_basis(interleaved):
        raise ValueError(
            "Anti-stabilizers do not form a symplectic basis with the "
            f"stabilizers: {why_not_symplectic_basis(interleaved)}."
        )
    if not _logical_pairs_anticommute(interleaved):
        raise ValueError(
            "Anti-stabilizers do not anti-commute with corresponding stabilizers."
        )
    if not _logical_ops_on_diff_qubits_commute(interleaved):
        raise ValueError(
            "Stabilizer/anti-stabilizer pairs acting on different qubits do not commute."
        )
    if not is_stabilizer_group(PauliGroup(anti_stabilizers)):
        raise ValueError("Anti-stabilizers do not form a stabilizer group.")
    if not are_mutually_commutative(
        PauliGroup(logical_basis), PauliGroup(anti_stabilizers)
    ):
        raise ValueError("Anti-stabilizers do not commute with logical operators.")


def _logical_basis_centralizes(
    logical_basis: Sequence[Pauli], generators: Sequence[Pauli]
) -> bool:
    return are_mutually_commutative(PauliGroup(logical_basis), PauliGroup(generators))


def _logical_pairs_anticommute(logical_basis: Sequence[Pauli]) -> bool:
    return all(
        not first.commutes_with(second) for first, second in chunked(logical_basis, 2)
    )


def _logical_ops_on_diff_qubits_commute(logical_basis: Sequence[Pauli]) -> bool:
    for index, (logical_x, logical_z) in enumerate(chunked(logical_basis, 2)):
        if not all(
            logical_x.commutes_with(element) and logical_z.commutes_with(element)
            for element in logical_basis[2 * index + 2 :]
        ):
            return False
    return True


def _anti_stabilizers_of(code: SubsystemCode) -> Sequence[Pauli]:
    generators = code.stabilizers
    logical_basis = tuple(code.logical_basis) + tuple(code.gauge_basis)
    pure_errors = full_binary_rank_completion_of(list(generators) + list(logical_basis))
    pure_errors = normalize(pure_errors, with_respect_to_basis=logical_basis)
    pure_errors = _ensure_anti_stabilizers_relations_with_generators(
        pure_errors, generators
    )
    return _make_abelian(pure_errors, generators)


def full_binary_rank_completion_of(generators: Sequence[Pauli]) -> Sequence[Pauli]:
    matrix, support = sparse_paulis_as_bitmatrix(generators)
    rank_profile = matrix.echelonize()
    qubit_count = len(support)
    complement = set(range(2 * qubit_count)).difference(rank_profile)
    result = []
    for index in complement:
        if index >= qubit_count:
            result.append(Pauli({support[index - qubit_count]: "Z"}))
        else:
            result.append(Pauli({support[index]: "X"}))
    return result


def _ensure_anti_stabilizers_relations_with_generators(
    pure_errors: Sequence[Pauli], generators: Sequence[Pauli]
) -> list[Pauli]:
    if len(pure_errors) != len(generators):
        raise ValueError(
            f"Pure errors ({len(pure_errors)}) and generators "
            f"({len(generators)}) have different lengths."
        )
    ordered_support = ordered_support_of(list(pure_errors) + list(generators))
    qubit_count = len(ordered_support)
    generator_count = len(generators)
    matrix = BitMatrix.zeros(len(pure_errors), generator_count + 2 * qubit_count)
    support_pos = {
        label: generator_count + position
        for position, label in enumerate(ordered_support)
    }
    assign_bitmatrix_from_sparse_paulis(pure_errors, support_pos, qubit_count, matrix)
    for row_id, pure_error in enumerate(pure_errors):
        for column_id, generator in enumerate(generators):
            matrix[row_id, column_id] = not pure_error.commutes_with(generator)
    matrix.echelonize()
    return [
        sparse_pauli_from_row(matrix, ordered_support, row_id, generator_count)
        for row_id in range(len(pure_errors))
    ]


def _make_abelian(
    pure_errors: list[Pauli], stabilizers: Sequence[Pauli]
) -> Sequence[Pauli]:
    anti_stabilizers = list(pure_errors)

    def commuting_pure_error(error: Pauli, index: int) -> Pauli:
        interleaved = list(interleave(anti_stabilizers, stabilizers))
        return normalizer_of_element(
            error, interleaved[: 2 * index] + interleaved[2 * index + 2 :]
        )

    for index in range(len(anti_stabilizers) - 1):
        anti_stabilizers[index] = commuting_pure_error(anti_stabilizers[index], index)
    return anti_stabilizers


def is_symplectic_basis(basis: Sequence[Pauli]) -> bool:
    return (
        _all_square_to_identity(basis)
        and _pairs_anticommute(basis)
        and _is_non_degenerate(basis)
    )


def why_not_symplectic_basis(basis: Sequence[Pauli]) -> str:
    if not _all_square_to_identity(basis):
        return "elements do not square to identity."
    if not _pairs_anticommute(basis):
        return "pairs do not anti-commute"
    if not _is_non_degenerate(basis):
        return "the basis is degenerate"
    return ""


def _all_square_to_identity(paulis: Sequence[Pauli]) -> bool:
    return all(_is_identity(pauli * pauli) for pauli in paulis)


def _is_identity(pauli: Pauli) -> bool:
    return pauli.weight == 0 and pauli.phase == 1


def _pairs_anticommute(basis: Sequence[Pauli]) -> bool:
    return all(not first.commutes_with(second) for first, second in chunked(basis, 2))


def _is_non_degenerate(basis: Sequence[Pauli]) -> bool:
    support = set().union(*(set(pauli.support) for pauli in basis)) if basis else set()
    qubit_count = len(support)
    dense_basis = [DensePauli.from_sparse(abs(pauli), qubit_count) for pauli in basis]
    for index, (logical_x, logical_z) in enumerate(chunked(dense_basis, 2)):
        remaining = dense_basis[2 * index + 2 :]
        if not (
            logical_x.commutes_with(remaining) and logical_z.commutes_with(remaining)
        ):
            return False
    return True


def normalize(
    elements: Sequence[Pauli], with_respect_to_basis: Sequence[Pauli]
) -> Sequence[Pauli]:
    return [
        normalizer_of_element(element, with_respect_to_basis) for element in elements
    ]


def normalizer_of_element(element: Pauli, basis: Sequence[Pauli]) -> Pauli:
    for logical_x, logical_z in chunked(basis, 2):
        if not element.commutes_with(logical_x):
            element *= logical_z
        if not element.commutes_with(logical_z):
            element *= logical_x
    return element


def sparse_paulis_as_bitmatrix(
    paulis: Sequence[Pauli],
) -> tuple[BitMatrix, list[int]]:
    ordered_support = ordered_support_of(paulis)
    support_pos = {
        element: position for position, element in enumerate(ordered_support)
    }
    qubit_count = len(ordered_support)
    result = BitMatrix.zeros(len(paulis), 2 * qubit_count)
    assign_bitmatrix_from_sparse_paulis(paulis, support_pos, qubit_count, result)
    return result, ordered_support


def assign_bitmatrix_from_sparse_paulis(
    paulis: Sequence[Pauli],
    support_pos: dict[int, int],
    qubit_count: int,
    result: BitMatrix,
) -> None:
    for row_id, pauli in enumerate(paulis):
        for qubit in pauli.support:
            qubit_id = support_pos[qubit]
            character = pauli[qubit]
            if character == "X":
                result[row_id, qubit_id] = True
            elif character == "Y":
                result[row_id, qubit_id] = True
                result[row_id, qubit_id + qubit_count] = True
            elif character == "Z":
                result[row_id, qubit_id + qubit_count] = True
            else:
                raise ValueError(f"Unexpected Pauli letter {character}.")


def sparse_pauli_from_row(
    matrix: BitMatrix,
    ordered_support: list[int],
    row_id: int,
    offset: int,
) -> Pauli:
    qubit_count = len(ordered_support)
    x_part = Pauli(
        {
            ordered_support[qubit_id]: "X"
            for qubit_id in range(qubit_count)
            if matrix[row_id, offset + qubit_id]
        }
    )
    z_part = Pauli(
        {
            ordered_support[qubit_id]: "Z"
            for qubit_id in range(qubit_count)
            if matrix[row_id, offset + qubit_count + qubit_id]
        }
    )
    return abs(x_part * z_part)


def logical_basis_of(
    group: PauliGroup,
    *,
    supported_by: Optional[Iterable[int]] = None,
) -> Iterable[Pauli]:
    if supported_by is None:
        supported_by = group.support
    supported_by = tuple(supported_by)
    logical_basis_size = 2 * max(0, len(supported_by) - group.binary_rank)
    basis_elements = list(
        symplectic_form_of(centralizer_of(group, supported_by=supported_by).generators)
    )
    for index in range(0, logical_basis_size, 2):
        x_operator, z_operator = basis_elements[index], basis_elements[index + 1]
        if "Z" in characters_of(x_operator).values():
            basis_elements[index], basis_elements[index + 1] = (
                z_operator,
                x_operator,
            )
    return take(logical_basis_size, map(abs, basis_elements))


def are_mutually_commutative(group1: PauliGroup, group2: PauliGroup) -> bool:
    return all(
        generator1.commutes_with(generator2)
        for generator1, generator2 in product(group1.generators, group2.generators)
    )


def ordered_support_of(generators: Iterable[Pauli]) -> list[int]:
    support: set[int] = set()
    for generator in generators:
        support.update(generator.support)
    return sorted(support)
