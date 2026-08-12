from itertools import product
from qdk.ec._analysis.stabilizer_code import StabilizerCode
from typing import cast

from qdk.ec._analysis.propagation.pauli import Pauli, PauliCharacter

Coordinate = tuple[float, float]


def make_rotated_surface_code(*, x_distance: int, z_distance: int) -> StabilizerCode:
    return make_rotated_surface_code_with_labels(
        x_distance=x_distance, z_distance=z_distance
    )[0]


def make_rotated_surface_code_with_labels(
    *, x_distance: int, z_distance: int
) -> tuple[StabilizerCode, list[Coordinate]]:
    data_qubits = _rotated_surface_code_data_qubits(x_distance, z_distance)
    data_qubit_index = {coord: index for index, coord in enumerate(sorted(data_qubits))}

    labeled_generators = _rotated_surface_code_stabilizer_generators(
        x_distance, z_distance
    )
    generators = [
        _remap_pauli(pauli, data_qubit_index) for pauli in labeled_generators.values()
    ]
    labels = list(labeled_generators.keys())
    return StabilizerCode(generators), labels


def _remap_pauli(
    coord_pauli: dict[Coordinate, str], index_of: dict[Coordinate, int]
) -> Pauli:
    return Pauli({index_of[coord]: cast(PauliCharacter, char) for coord, char in coord_pauli.items()})


def _rotated_surface_code_data_qubits(
    x_distance: int, z_distance: int
) -> set[Coordinate]:
    if x_distance % 2 == 0 or z_distance % 2 == 0:
        raise ValueError(
            f"Invalid distances {x_distance, z_distance}. Both distances must be odd."
        )
    return set((row, col) for row, col in product(range(z_distance), range(x_distance)))


def _rotated_surface_code_x_ancilla_qubits(
    x_distance: int, z_distance: int
) -> set[Coordinate]:
    if x_distance % 2 == 0 or z_distance % 2 == 0:
        raise ValueError(
            f"Invalid distances {x_distance, z_distance}. Both distances must be odd."
        )
    return set(
        (row + 0.5, col + 0.5)
        for row, col in product(range(z_distance - 1), range(-1, x_distance))
        if (row + col) % 2 == 0
    )


def _rotated_surface_code_z_ancilla_qubits(
    x_distance: int, z_distance: int
) -> set[Coordinate]:
    if x_distance % 2 == 0 or z_distance % 2 == 0:
        raise ValueError(
            f"Invalid distances {x_distance, z_distance}. Both distances must be odd."
        )
    return set(
        (row + 0.5, col + 0.5)
        for row, col in product(range(-1, z_distance), range(x_distance - 1))
        if (row + col) % 2 == 1
    )


def _rotated_surface_code_x_stabilizer_generators(
    x_distance: int, z_distance: int
) -> dict[Coordinate, dict[Coordinate, str]]:
    data_qubits = _rotated_surface_code_data_qubits(x_distance, z_distance)
    x_ancillas = _rotated_surface_code_x_ancilla_qubits(x_distance, z_distance)
    x_generators = {}
    for ancilla in x_ancillas:
        generator_characters: dict[Coordinate, str] = {}
        for direction in [(0.5, 0.5), (0.5, -0.5), (-0.5, 0.5), (-0.5, -0.5)]:
            neighbor = (ancilla[0] + direction[0], ancilla[1] + direction[1])
            if neighbor in data_qubits:
                generator_characters[neighbor] = "X"
        x_generators[ancilla] = generator_characters
    return x_generators


def _rotated_surface_code_z_stabilizer_generators(
    x_distance: int, z_distance: int
) -> dict[Coordinate, dict[Coordinate, str]]:
    data_qubits = _rotated_surface_code_data_qubits(x_distance, z_distance)
    z_ancillas = _rotated_surface_code_z_ancilla_qubits(x_distance, z_distance)
    z_generators = {}
    for ancilla in z_ancillas:
        generator_characters: dict[Coordinate, str] = {}
        for direction in [(0.5, 0.5), (0.5, -0.5), (-0.5, 0.5), (-0.5, -0.5)]:
            neighbor = (ancilla[0] + direction[0], ancilla[1] + direction[1])
            if neighbor in data_qubits:
                generator_characters[neighbor] = "Z"
        z_generators[ancilla] = generator_characters
    return z_generators


def _rotated_surface_code_stabilizer_generators(
    x_distance: int, z_distance: int
) -> dict[Coordinate, dict[Coordinate, str]]:
    x_generators = _rotated_surface_code_x_stabilizer_generators(x_distance, z_distance)
    z_generators = _rotated_surface_code_z_stabilizer_generators(x_distance, z_distance)
    return x_generators | z_generators
