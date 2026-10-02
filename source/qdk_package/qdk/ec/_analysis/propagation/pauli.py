"""Pauli conveniences used by exact propagation and qodec profiling."""

from __future__ import annotations

from typing import (
    Container,
    Final,
    Iterator,
    Literal,
    Mapping,
    cast,
    get_args,
)

from paulimer import SparsePauli

Pauli = SparsePauli
PauliCharacter = Literal["I", "X", "Y", "Z"]
pauli_characters: Final[frozenset[str]] = frozenset(get_args(PauliCharacter))

_PHASE_TO_EXPONENT: dict[complex, int] = {
    1 + 0j: 0,
    0 + 1j: 1,
    -1 + 0j: 2,
    0 - 1j: 3,
}


def identity(phase: complex = 1) -> Pauli:
    """Return the identity Pauli with an optional unit scalar phase."""
    try:
        exponent = _PHASE_TO_EXPONENT[complex(phase)]
    except KeyError as error:
        raise ValueError(f"Unsupported phase: {phase!r}") from error
    return SparsePauli({}, exponent=exponent)


def characters_of(pauli: Pauli) -> dict[int, PauliCharacter]:
    """Return non-identity characters keyed by qubit index."""
    return {
        qubit: cast(PauliCharacter, character)
        for qubit, character in zip(pauli.support, pauli.characters)
    }


def relabel(pauli: Pauli, mapping: Mapping[int, int]) -> Pauli:
    """Return ``pauli`` with its qubits renamed, keeping its phase.

    Qubits absent from ``mapping`` keep their label.
    """
    return Pauli(
        {mapping.get(qubit, qubit): pauli[qubit] for qubit in pauli.support}
    ) * identity(pauli.phase)


def restrict(pauli: Pauli, support: Container[int]) -> Pauli:
    """Return the part of ``pauli`` acting on ``support``, keeping its phase."""
    return Pauli(
        {qubit: pauli[qubit] for qubit in pauli.support if qubit in support}
    ) * identity(pauli.phase)


def complex_conjugate_of(pauli: Pauli) -> Pauli:
    """Return the complex conjugate of ``pauli``: a sign flip per ``Y``."""
    y_count = sum(character == "Y" for character in characters_of(pauli).values())
    return pauli * identity((-1) ** (y_count % 2))


def as_literal(character: str) -> PauliCharacter:
    if character not in pauli_characters:
        raise ValueError(f"Invalid Pauli character: {character}")
    return cast(PauliCharacter, character)


def as_literals(string: str) -> Iterator[PauliCharacter]:
    yield from map(as_literal, string)


def parse_term(token: str) -> tuple[PauliCharacter, int]:
    """Split one ``"<basis>_<index>"`` operator token; a bare letter is qubit 0."""
    basis, _, index = token.partition("_")
    return as_literal(basis), int(index) if index else 0


def characters_of_string(text: str) -> dict[int, PauliCharacter]:
    """Parse a ``"X_0 Z_2"`` operator string into ``{qubit: character}``."""
    characters: dict[int, PauliCharacter] = {}
    for token in text.split():
        basis, index = parse_term(token)
        characters[index] = basis
    return characters


__all__ = [
    "Pauli",
    "PauliCharacter",
    "as_literal",
    "as_literals",
    "characters_of",
    "identity",
]
