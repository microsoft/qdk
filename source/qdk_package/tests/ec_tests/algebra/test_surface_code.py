from hypothesis import strategies, given, settings
from ec_tests.testing.code_catalog.surface_codes import (
    make_rotated_surface_code,
)
from ec_tests.algebra.test_stabilizer_codes import (
    assert_lookup_decoder_distance,
)
from ec_tests.algebra.test_subsystem_codes import (
    assert_valid_logical_basis,
)


def odd_integers_strategy(
    min_value: int, max_value: int
) -> strategies.SearchStrategy[int]:
    return strategies.integers(min_value=min_value, max_value=max_value).filter(
        lambda x: x % 2 == 1
    )


@given(
    odd_integers_strategy(min_value=3, max_value=13),
    odd_integers_strategy(min_value=3, max_value=13),
)
def test_rotated_surface_code_length(x_distance: int, z_distance: int) -> None:
    code = make_rotated_surface_code(x_distance=x_distance, z_distance=z_distance)
    assert code.length == x_distance * z_distance


@given(
    odd_integers_strategy(min_value=3, max_value=5),
)
@settings(deadline=10000, max_examples=2)
def test_rotated_surface_code_distance(distance: int) -> None:
    code = make_rotated_surface_code(x_distance=distance, z_distance=distance)
    assert_lookup_decoder_distance(code, distance)


@given(
    odd_integers_strategy(min_value=3, max_value=5),
)
def test_rotated_surface_code_logicals(distance: int) -> None:
    code = make_rotated_surface_code(x_distance=distance, z_distance=distance)
    assert code.logical_qubit_count == 1
    assert_valid_logical_basis(code)
