from typing import Iterable
from itertools import combinations
from qdk.ec._analysis.propagation.pauli import Pauli, PauliCharacter
from qdk.ec._analysis.stabilizer_code import StabilizerCode


def make_repetition_code(
    size: int,
) -> StabilizerCode:
    if size <= 1:
        raise ValueError("number_of_repetitions must be > 1.")
    generators = [Pauli({index: "X", index + 1: "X"}) for index in range(size - 1)]
    return StabilizerCode(generators)


def make_shor_code() -> StabilizerCode:
    return StabilizerCode(
        [
            Pauli({0: "Z", 1: "Z"}),
            Pauli({1: "Z", 2: "Z"}),
            Pauli({3: "Z", 4: "Z"}),
            Pauli({4: "Z", 5: "Z"}),
            Pauli({6: "Z", 7: "Z"}),
            Pauli({7: "Z", 8: "Z"}),
            Pauli({0: "X", 1: "X", 2: "X", 3: "X", 4: "X", 5: "X"}),
            Pauli({3: "X", 4: "X", 5: "X", 6: "X", 7: "X", 8: "X"}),
        ]
    )


def make_five_qubit_code() -> StabilizerCode:
    return StabilizerCode(
        [
            Pauli.from_string("ZXXZI"),
            Pauli.from_string("IZXXZ"),
            Pauli.from_string("ZIZXX"),
            Pauli.from_string("XZIZX"),
        ]
    )


class BinaryMonomial:
    """
    A binary monomial x0^{a0}... x[m-1]^{a[m-1]} with m variables
    x0, ..., x[m-1] and ai = 0 or 1.
    It is represented by the set of indices i such that ai = 1.
    The empty set is interpreted the constant 1.
    """

    def __init__(self, variables: set[int]) -> None:
        self.variables = variables

    def evaluate(self, support: set[int]) -> int:
        """
        Return the value of the monimial when
        xi = 1 if i is in support and xi = 0 otherwise.
        """
        if len(self.variables) == 0:
            return 1
        for index in self.variables:
            if index not in support:
                return 0
        return 1


def _evaluation_vector_of(
    monomial: BinaryMonomial, number_of_variables: int
) -> list[int]:
    """
    Return a list with length 2^m containing the evaluation of
    the given monomial for all the vectors of Z2^m.
    """
    evaluation_vector = []
    for weight in range(number_of_variables + 1):
        for support in combinations(range(number_of_variables), weight):
            evaluation_vector.append(monomial.evaluate(set(support)))
    return evaluation_vector


def _reed_muller_code_generator_matrix(
    number_of_variables: int, maximum_degree: int
) -> list[list[int]]:
    """
    The rows of the generator matrix of a RM code are the vectors
    with length 2^m obtained by evaluating monomials with m variables
    with degree <= r in all the points of Z2^m where
    m = number_of_variables,
    r = maximum_degree.
    """
    matrix = []
    for weight in range(maximum_degree + 1):
        for monomial_terms in combinations(range(number_of_variables), weight):
            monomial = BinaryMonomial(set(monomial_terms))
            matrix.append(_evaluation_vector_of(monomial, number_of_variables))
    return matrix


def _generators_from_matrix(
    matrix: list[list[int]], generators_type: PauliCharacter
) -> list[Pauli]:
    if generators_type in "iI":
        raise ValueError("Generators_type must be X, Y or Z.")
    generators = []
    for row in matrix:
        generators.append(
            Pauli(
                {
                    index: generators_type
                    for index, value in enumerate(row)
                    if value == 1
                }
            )
        )
    return generators


def make_quantum_reed_muller_code(
    number_of_variables: int, maximum_x_degree: int, maximum_z_degree: int
) -> StabilizerCode:
    """
    The X stabilizers correspond to the polynomials with m variables
    with degree <= rX and the Z stabilizers correspond to the
    polynomials with m variables with degree <= rZ where:
    m = number_of_variables,
    rX = maximum_x_degree,
    rZ = maximum_z_degree.
    """
    if maximum_x_degree + maximum_z_degree > number_of_variables - 1:
        raise ValueError("Degrees too large to define a Reed-Muller code.")
    x_matrix = _reed_muller_code_generator_matrix(number_of_variables, maximum_x_degree)
    z_matrix = _reed_muller_code_generator_matrix(number_of_variables, maximum_z_degree)
    x_generators = _generators_from_matrix(x_matrix, "X")
    z_generators = _generators_from_matrix(z_matrix, "Z")
    return StabilizerCode(x_generators + z_generators)


def _punctured_reed_muller_code_generator_matrix(
    number_of_variables: int, maximum_degree: int
) -> list[list[int]]:
    matrix = []
    for weight in range(1, maximum_degree + 1):
        for monomial_terms in combinations(range(number_of_variables), weight):
            monomial = BinaryMonomial(set(monomial_terms))
            matrix.append(_evaluation_vector_of(monomial, number_of_variables)[1:])
    return matrix


def make_quantum_punctured_reed_muller_code(
    number_of_variables: int, maximum_x_degree: int, maximum_z_degree: int
) -> StabilizerCode:
    """
    Remove the two stabilizer generators X...X and Z...Z from the
    quantum Reed Muller group and remove qubit 0.
    """
    if maximum_x_degree == 0 and maximum_z_degree == 0:
        raise ValueError("Maximum degrees cannot be both equal to 0.")
    if maximum_x_degree + maximum_z_degree > number_of_variables - 1:
        raise ValueError("Degrees too large to define a Reed-Muller code.")
    x_matrix = _punctured_reed_muller_code_generator_matrix(
        number_of_variables, maximum_x_degree
    )
    z_matrix = _punctured_reed_muller_code_generator_matrix(
        number_of_variables, maximum_z_degree
    )
    x_generators = _generators_from_matrix(x_matrix, "X")
    z_generators = _generators_from_matrix(z_matrix, "Z")
    return StabilizerCode(x_generators + z_generators)


def make_steane_code() -> StabilizerCode:
    return make_quantum_hamming_code(3)


def make_quantum_hamming_code(number_of_checks: int) -> StabilizerCode:
    return make_quantum_punctured_reed_muller_code(number_of_checks, 1, 1)


def make_quantum_extended_hamming_code(number_of_checks: int) -> StabilizerCode:
    return make_quantum_reed_muller_code(number_of_checks, 1, 1)


def _make_golay_code_generator_matrix() -> list[list[int]]:
    return [
        [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 1, 1, 1],
        [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 1, 1, 0, 1, 0, 0, 0],
        [0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 1, 1, 0, 1, 0, 0],
        [0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 1, 1, 0, 1, 0],
        [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 1, 1, 0, 1],
        [0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 0, 0, 1],
        [0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1, 1],
        [0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 1, 1, 1, 0, 0, 0, 1, 1, 0],
        [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 1, 1, 1, 0, 0, 0, 1, 1],
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 1, 1, 1, 1, 0],
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 1, 1, 1, 1],
    ]


def make_quantum_golay_code() -> StabilizerCode:
    check_matrix = _make_golay_code_generator_matrix()
    x_generators = _generators_from_matrix(check_matrix, "X")
    z_generators = _generators_from_matrix(check_matrix, "Z")
    return StabilizerCode(x_generators + z_generators)


def make_color_code_832() -> StabilizerCode:
    return StabilizerCode(
        [
            Pauli.from_string("XXXXXXXX"),
            Pauli.from_string("ZZZZZZZZ"),
            Pauli.from_string("ZZZZIIII"),
            Pauli.from_string("ZZIIZZII"),
            Pauli.from_string("ZIZIZIZI"),
        ],
        logical_basis=[
            Pauli.from_string("XXXXIIII"),
            Pauli.from_string("ZIIIZIII"),
            Pauli.from_string("XXIIXXII"),
            Pauli.from_string("ZIZIIIII"),
            Pauli.from_string("XIXIXIXI"),
            Pauli.from_string("ZZIIIIII"),
        ],
    )


def make_tesseract_code() -> StabilizerCode:
    qubits = tuple(range(16))
    rows = [qubits[4 * row : 4 * (row + 1)] for row in range(4)]
    columns = [qubits[col::4] for col in range(4)]
    squares = [
        (0, 1, 4, 5),
        (5, 6, 9, 10),
        (1, 2, 5, 6),
        (4, 5, 8, 9),
    ]
    generator_supports = [
        rows[0] + rows[1],
        rows[1] + rows[2],
        rows[2] + rows[3],
        columns[0] + columns[1],
        columns[1] + columns[2],
    ]
    generators = [_pauli_on(support, "Z") for support in generator_supports]
    generators += [_pauli_on(support, "X") for support in generator_supports]
    logicals = [
        _pauli_on(rows[0], "X"),
        _pauli_on(columns[0], "Z"),
        _pauli_on(columns[0], "X"),
        _pauli_on(rows[0], "Z"),
        _pauli_on(squares[0], "X"),
        _pauli_on(squares[1], "Z"),
        _pauli_on(squares[1], "X"),
        _pauli_on(squares[0], "Z"),
        _pauli_on(squares[2], "X"),
        _pauli_on(squares[3], "Z"),
        _pauli_on(squares[3], "X"),
        _pauli_on(squares[2], "Z"),
    ]
    return StabilizerCode(generators, logical_basis=logicals)


def make_carbon_code() -> StabilizerCode:
    return StabilizerCode(
        [
            Pauli.from_string("XXXX"),
            Pauli.from_string("IIIIXXXX"),
            Pauli.from_string("IIIIIIIIXXXX"),
            Pauli.from_string("ZZZZ"),
            Pauli.from_string("IIIIZZZZ"),
            Pauli.from_string("IIIIIIIIZZZZ"),
            Pauli.from_string("XXIIIXIXXIIX"),
            Pauli.from_string("XIIXXXIIIXIX"),
            Pauli.from_string("ZIZIIIZZZIIZ"),
            Pauli.from_string("ZIIZZIZIIIZZ"),
        ]
    )


def _pauli_on(support: Iterable[int], character: PauliCharacter) -> Pauli:
    return Pauli({index: character for index in support})
