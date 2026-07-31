from typing import Any, Callable
import math
from hypothesis import strategies, given
# from qdk.ec.collections.big_sequence import BigSequence
from qdk.ec.profile.propagation.pauli import Pauli, PauliEnumerator


@strategies.composite
def error_characters(draw_from: Callable[..., Any]) -> str:
    characters = draw_from(strategies.permutations("XYZ"))
    length = draw_from(strategies.integers(min_value=0, max_value=3))
    return "".join(characters[:length])


@given(
    strategies.sets(strategies.integers(min_value=0, max_value=100), max_size=5),
    strategies.integers(min_value=0, max_value=5),
    error_characters(),
)
def test_enumeration_of_weight(support: set[int], weight: int, characters: str) -> None:
    weight = min(len(support), weight, len(characters))
    enumerator = PauliEnumerator(support, characters=characters)
    enumeration = enumerator.of_weight(weight)
    expected_length = math.comb(len(support), weight) * (len(characters) ** weight)
    assert len(set(enumeration)) == expected_length
    assert all(pauli.weight == weight for pauli in enumeration)


@given(
    strategies.sets(strategies.integers(min_value=0, max_value=100), max_size=5),
    strategies.lists(strategies.integers(min_value=0, max_value=5)),
    error_characters(),
)
def test_enumeration_by_weight(
    support: set[int], weights: list[int], characters: str
) -> None:
    enumerator = PauliEnumerator(support, characters=characters)
    of_weights: list[Pauli] = []
    for weight in weights:
        of_weights.extend(enumerator.of_weight(weight))
    by_weight = enumerator.by_weight(weights)
    assert list(of_weights) == list(by_weight)


@given(
    strategies.sets(strategies.integers(min_value=0, max_value=100), max_size=5),
    strategies.integers(min_value=0, max_value=5),
    error_characters(),
)
def test_enumeration_up_to_weight(
    support: set[int], weight: int, characters: str
) -> None:
    enumerator = PauliEnumerator(support, characters=characters)
    by_weight = enumerator.by_weight(range(weight + 1))
    up_to_weight = enumerator.up_to_weight(weight)
    assert list(by_weight) == list(up_to_weight)
