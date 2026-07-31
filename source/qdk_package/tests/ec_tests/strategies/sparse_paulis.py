from typing import Any, Optional, Callable
from hypothesis import strategies
from ec_tests.strategies.sparse_phases import sparse_phases
from qdk.ec.profile.propagation.pauli import Pauli, identity


def pauli_characters() -> strategies.SearchStrategy[str]:
    return strategies.sampled_from("IXYZ")


@strategies.composite
def pauli_strings(
    draw_from: Callable[..., Any],
    size: Optional[int] = None,
    min_weight: int = 0,
    max_weight: int = 100,
) -> str:
    if size is None:
        size = draw_from(strategies.integers(min_value=min_weight, max_value=100))
    if size < min_weight:
        raise ValueError(f"Size {size} is less than minimum weight {min_weight}.")
    if size == 0:
        return ""
    max_weight = min(size, max_weight)
    weight = draw_from(strategies.integers(min_value=min_weight, max_value=max_weight))
    support = draw_from(
        strategies.lists(
            strategies.integers(min_value=0, max_value=size - 1),
            min_size=weight,
            max_size=weight,
            unique=True,
        )
    )
    support_string = draw_from(strategies.text("XYZ", min_size=weight, max_size=weight))
    characters = ["I"] * size
    for index, character in zip(support, support_string):
        characters[index] = character
    return "".join(characters)


@strategies.composite
def sparse_pauli_elements(  # pylint: disable=too-many-arguments, too-many-positional-arguments
    draw_from: Callable[..., Any],
    size: Optional[int] = None,
    min_weight: int = 0,
    max_weight: int = 100,
    phase_strategy: strategies.SearchStrategy[complex] = sparse_phases(),
    qubit_strategy: strategies.SearchStrategy[int] = strategies.integers(min_value=0, max_value=1000),
) -> Pauli:
    character_string = draw_from(
        pauli_strings(size=size, min_weight=min_weight, max_weight=max_weight)
    )
    qubits = draw_from(
        strategies.lists(
            qubit_strategy,
            min_size=len(character_string),
            max_size=len(character_string),
            unique=True,
        )
    )
    characters = dict(zip(qubits, character_string))
    phase = draw_from(phase_strategy)
    return Pauli(characters) * identity(phase)


@strategies.composite
def equal_length_sparse_pauli_elements(
    draw_from: Callable[..., Any],
    count: int = 2,
    max_length: int = 100,
    phase_strategy: strategies.SearchStrategy[complex] = sparse_phases(),
) -> tuple[Pauli, ...]:
    size = draw_from(strategies.integers(min_value=0, max_value=max_length))
    element_stategy = sparse_pauli_elements(size=size, phase_strategy=phase_strategy)
    elements = draw_from(
        strategies.lists(element_stategy, min_size=count, max_size=count)
    )
    return tuple(elements)


@strategies.composite
def distinct_length_sparse_pauli_elements(
    draw_from: Callable[..., Any],
) -> tuple[Pauli, Pauli]:
    size_strategy = strategies.tuples(
        strategies.integers(min_value=0, max_value=100),
        strategies.integers(min_value=0, max_value=100),
    ).filter(lambda sizes: sizes[0] != sizes[1])
    left_size, right_size = draw_from(size_strategy)
    left = draw_from(sparse_pauli_elements(size=left_size))
    right = draw_from(sparse_pauli_elements(size=right_size))
    return (left, right)
