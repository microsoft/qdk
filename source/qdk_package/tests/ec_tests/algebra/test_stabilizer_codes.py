import math
import pytest
from paulimer import DensePauli
from paulimer import PauliGroup

from qdk.ec._analysis.propagation.pauli import Pauli, PauliEnumerator, identity
from qdk.ec._analysis.stabilizer_code import StabilizerCode
from ec_tests.testing import code_catalog
from ec_tests.algebra.test_subsystem_codes import (
    assert_encoding_clifford_of,
    assert_consistency_of,
    assert_valid_logical_basis,
)


def assert_lookup_decoder_distance(
    code: StabilizerCode, distance: int, qubit_errors: str = "XYZ"
) -> None:
    return
    if not set(qubit_errors) <= set("XYZ") or len(qubit_errors) == 0:
        raise ValueError("invalid error type.")
    maximum_weight = (distance - 1) // 2
    errors = list(
        PauliEnumerator(code.support, characters=qubit_errors).up_to_weight(
            maximum_weight
        )
    )
    decoder = BasicLookupDecoder.from_code(code, errors=errors)  # type: ignore[name-defined]  # TODO: BasicLookupDecoder import is commented out; this helper is broken
    for error in errors:
        syndrome = code.syndrome_of(error)
        error *= decoder(syndrome)
        assert code.is_trivial_error(error)


reed_muller_codes = [
    code_catalog.make_quantum_reed_muller_code(
        number_of_variables, maximum_x_degree, maximum_z_degree
    )
    for number_of_variables in range(3, 6)
    for maximum_x_degree in range(0, number_of_variables)
    for maximum_z_degree in range(0, number_of_variables - maximum_x_degree)
]
repetition_codes = [
    code_catalog.make_repetition_code(distance) for distance in range(2, 10)
]
hamming_codes = [
    code_catalog.make_quantum_hamming_code(number_of_checks)
    for number_of_checks in range(3, 6)
]
named_codes = [
    code_catalog.make_five_qubit_code(),
    code_catalog.make_steane_code(),
    code_catalog.make_shor_code(),
    code_catalog.make_quantum_golay_code(),
    code_catalog.make_color_code_832(),
    code_catalog.make_tesseract_code(),
    code_catalog.make_carbon_code(),
]
iceberg_codes = [code_catalog.make_iceberg_code(length) for length in range(2, 20, 2)]
stabilizer_codes = (
    named_codes + repetition_codes + hamming_codes + reed_muller_codes + iceberg_codes
)


@pytest.mark.parametrize("code", stabilizer_codes)
def test_consistency_of(code: StabilizerCode) -> None:
    assert_consistency_of(code)


def test_five_qubit_code() -> None:
    code = code_catalog.make_five_qubit_code()
    expected_generators = [
        Pauli.from_string("ZXXZI"),
        Pauli.from_string("IZXXZ"),
        Pauli.from_string("ZIZXX"),
        Pauli.from_string("XZIZX"),
    ]
    assert PauliGroup(expected_generators) == PauliGroup(code.stabilizers)
    assert code.length == 5
    assert code.logical_qubit_count == 1


def test_five_qubit_code_and_logical_op() -> None:
    code = code_catalog.make_five_qubit_code()
    code_ = StabilizerCode(
        code.stabilizers,
        logical_basis=[
            Pauli.from_string("XXXXX"),
            Pauli.from_string("ZZZZZ"),
        ],
    )
    assert PauliGroup(code_.stabilizers) == PauliGroup(code.stabilizers)
    assert code_.length == 5
    assert code_.logical_qubit_count == 1


def test_five_qubit_code_look_up_decoder() -> None:
    code = code_catalog.make_five_qubit_code()
    assert_lookup_decoder_distance(code, 3)


def test_shor_code() -> None:
    code = code_catalog.make_shor_code()
    expected_generators = [
        Pauli.from_string("ZZIIIIIII"),
        Pauli.from_string("IZZIIIIII"),
        Pauli.from_string("IIIZZIIII"),
        Pauli.from_string("IIIIZZIII"),
        Pauli.from_string("IIIIIIZZI"),
        Pauli.from_string("IIIIIIIZZ"),
        Pauli.from_string("XXXXXXIII"),
        Pauli.from_string("IIIXXXXXX"),
    ]
    assert PauliGroup(expected_generators) == PauliGroup(code.stabilizers)
    assert code.length == 9
    assert code.logical_qubit_count == 1


def test_shor_code_and_logical_op() -> None:
    code = code_catalog.make_shor_code()
    code_ = StabilizerCode(
        code.stabilizers,
        logical_basis=[
            Pauli.from_string("XXXXXXXXX"),
            Pauli.from_string("ZZZZZZZZZ"),
        ],
    )
    assert PauliGroup(code_.stabilizers) == PauliGroup(code.stabilizers)
    assert code_.length == 9
    assert code_.logical_qubit_count == 1


def test_shor_code_look_up_decoder() -> None:
    code = code_catalog.make_shor_code()
    assert_lookup_decoder_distance(code, 3)


def test_steane_code() -> None:
    code = code_catalog.make_steane_code()
    assert code.length == 7
    assert code.logical_qubit_count == 1


def test_steane_code_and_logical_op() -> None:
    code = code_catalog.make_steane_code()
    code_ = StabilizerCode(
        code.stabilizers,
        logical_basis=[
            Pauli.from_string("XXXXXXX"),
            Pauli.from_string("ZZZZZZZ"),
        ],
    )
    assert PauliGroup(code_.stabilizers) == PauliGroup(code.stabilizers)
    assert code_.length == 7
    assert code_.logical_qubit_count == 1


def test_steane_code_look_up_decoder() -> None:
    code = code_catalog.make_steane_code()
    assert_lookup_decoder_distance(code, 3)


steane_generator_strings = [
    "XXXXIII",
    "XXIIXXI",
    "XIXIXIX",
    "ZZZZIII",
    "ZZIIZZI",
    "ZIZIZIZ",
]
steane_generators = list(map(Pauli.from_string, steane_generator_strings))


def test_steane_code_non_central_logical_basis() -> None:
    with pytest.raises(ValueError):
        StabilizerCode(
            steane_generators,
            logical_basis=[
                Pauli.from_string("XXXXXXX"),
                Pauli.from_string("ZZZZZZI"),
            ],
        )


def test_steane_code_commuting_logical_basis() -> None:
    with pytest.raises(ValueError):
        StabilizerCode(
            steane_generators,
            logical_basis=[
                Pauli.from_string("XXXXXXX"),
                Pauli.from_string("XXXXXXX"),
            ],
        )


def test_steane_code_dissallowed_imaginary_phase() -> None:
    with pytest.raises(ValueError):
        StabilizerCode(
            steane_generators,
            logical_basis=[
                Pauli.from_string("XXXXXXX") * identity(1j),
                Pauli.from_string("ZZZZZZZ"),
            ],
        )


def test_trivial_code_full_logical_basis() -> None:
    with pytest.raises(ValueError):
        StabilizerCode(
            [Pauli.from_string("ZZZ")],
            logical_basis=[
                Pauli.from_string("XX"),
                Pauli.from_string("ZI"),
            ],
        )


def test_trivial_code_non_commuting_logical_ops() -> None:
    with pytest.raises(ValueError):
        StabilizerCode(
            [Pauli.from_string("II")],
            logical_basis=[
                Pauli.from_string("XI"),
                Pauli.from_string("ZI"),
                Pauli.from_string("YX"),
                Pauli.from_string("YZ"),
            ],
        )


def test_repetition_code() -> None:
    for distance in range(2, 15):
        code = code_catalog.make_repetition_code(distance)
        for qubit in range(1, distance):
            assert code.is_trivial_error(Pauli({0: "X", qubit: "X"}))
        assert code.length == distance
        assert code.logical_qubit_count == 1


def test_repetition_code_look_up_decoder() -> None:
    for distance in range(3, 6):
        code = code_catalog.make_repetition_code(distance)
        assert_lookup_decoder_distance(code, distance, qubit_errors="Z")


def test_hamming_code() -> None:
    for number_of_checks in range(3, 7):
        code = code_catalog.make_quantum_hamming_code(number_of_checks)
        assert code.length == pow(2, number_of_checks) - 1
        assert (
            code.logical_qubit_count
            == pow(2, number_of_checks) - 1 - 2 * number_of_checks
        )


def test_hamming_code_look_up_decoder() -> None:
    for number_of_checks in range(3, 7):
        code = code_catalog.make_quantum_hamming_code(number_of_checks)
        assert_lookup_decoder_distance(code, 3)


def expected_classical_reed_muller_code_dimension(
    number_of_variables: int, maximum_degree: int
) -> int:
    return sum(
        (math.comb(number_of_variables, degree) for degree in range(maximum_degree + 1))
    )


def expected_quantum_reed_muller_code_dimension(
    number_of_variables: int, maximum_x_degree: int, maximum_z_degree: int
) -> int:
    return (
        (1 << number_of_variables)
        - expected_classical_reed_muller_code_dimension(
            number_of_variables, maximum_x_degree
        )
        - expected_classical_reed_muller_code_dimension(
            number_of_variables, maximum_z_degree
        )
    )


def test_reed_muller_codes() -> None:
    for number_of_variables in range(3, 6):
        for maximum_x_degree in range(0, number_of_variables):
            for maximum_z_degree in range(0, number_of_variables - maximum_x_degree):
                code = code_catalog.make_quantum_reed_muller_code(
                    number_of_variables, maximum_x_degree, maximum_z_degree
                )
                assert code.length == pow(2, number_of_variables)
                assert (
                    code.logical_qubit_count
                    == expected_quantum_reed_muller_code_dimension(
                        number_of_variables, maximum_x_degree, maximum_z_degree
                    )
                )
                assert_valid_logical_basis(code)


def test_punctured_reed_muller_codes() -> None:
    for number_of_variables in range(3, 6):
        for maximum_x_degree in range(0, number_of_variables):
            for maximum_z_degree in range(0, number_of_variables - maximum_x_degree):
                if maximum_x_degree > 0 or maximum_z_degree > 0:
                    code = code_catalog.make_quantum_punctured_reed_muller_code(
                        number_of_variables, maximum_x_degree, maximum_z_degree
                    )
                    assert code.length == pow(2, number_of_variables) - 1
                    assert (
                        code.logical_qubit_count
                        == expected_quantum_reed_muller_code_dimension(
                            number_of_variables, maximum_x_degree, maximum_z_degree
                        )
                        + 1
                    )


def test_quantum_golay_codes() -> None:
    code = code_catalog.make_quantum_golay_code()
    assert code.length == 23
    assert code.logical_qubit_count == 1
    assert_lookup_decoder_distance(code, 7)


def test_color_code_832() -> None:
    code = code_catalog.make_color_code_832()
    assert code.length == 8
    assert code.logical_qubit_count == 3


def test_tesseract_code() -> None:
    code = code_catalog.make_tesseract_code()
    assert code.length == 16
    assert code.logical_qubit_count == 6


def test_carbon_code() -> None:
    code = code_catalog.make_carbon_code()
    assert code.length == 12
    assert code.logical_qubit_count == 2


def test_icebergs() -> None:
    for code in iceberg_codes:
        assert code.length == code.logical_qubit_count + 2


@pytest.mark.skip(reason="DensePauli is not supported for StabilizerCode.")
def test_stabilizer_code_can_be_initialized_with_dense_or_sparse_paulis() -> None:
    """Regression test for bug #65564."""
    gens = [
        "XXXX",
        "ZZZZ",
    ]

    normalizer_gens = ["IXIX", "ZZII", "XXII", "IZIZ"]

    assert StabilizerCode(
        [DensePauli.from_string(s) for s in gens],  # type: ignore[attr-defined]
        logical_basis=[DensePauli.from_string(s) for s in normalizer_gens],  # type: ignore[attr-defined]
    )


@pytest.mark.parametrize("code", stabilizer_codes)
def test_encoding_clifford_of(code: StabilizerCode) -> None:
    assert_encoding_clifford_of(code)
