from typing import Any, Iterable, Callable
from hypothesis import strategies
from more_itertools import split_into


@strategies.composite
def partitions(
    draw: Callable[..., Any],
    iterables: strategies.SearchStrategy[Iterable[Any]],
) -> Iterable[Iterable[Any]]:
    elements = list(draw(iterables))
    bin_count = draw(strategies.integers(min_value=1, max_value=max(1, len(elements))))
    bin_lengths: list[int] = []
    for bin_index in range(bin_count - 1):
        max_length = len(elements) - sum(bin_lengths) - (bin_count - bin_index) + 1
        length = draw(strategies.integers(min_value=1, max_value=max_length))
        bin_lengths.append(length)
    bin_lengths.append(len(elements) - sum(bin_lengths))
    return split_into(elements, bin_lengths)
