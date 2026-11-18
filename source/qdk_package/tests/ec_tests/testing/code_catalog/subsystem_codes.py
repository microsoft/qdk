from itertools import product
from paulimer import centralizer_of
from paulimer import PauliGroup

from qdk.ec._analysis.propagation.pauli import Pauli
from qdk.ec._analysis.code_algebra import SubsystemCode


def center_of(group: PauliGroup) -> PauliGroup:
    """The center of ``group`` — the elements that commute with all of it."""
    return group & centralizer_of(group)


def make_bacon_shor_code(x_distance: int, z_distance: int) -> SubsystemCode:
    qubit_index = {
        (row, col): row * z_distance + col
        for row, col in product(range(x_distance), range(z_distance))
    }
    centralizers = [
        Pauli({qubit_index[(row, column)]: "Z", qubit_index[(row, column + 1)]: "Z"})
        for row, column in product(range(x_distance), range(z_distance - 1))
    ] + [
        Pauli({qubit_index[(row, column)]: "X", qubit_index[(row + 1, column)]: "X"})
        for row, column in product(range(x_distance - 1), range(z_distance))
    ]
    logical_z = Pauli({qubit_index[(row, 0)]: "Z" for row in range(x_distance)})
    logical_x = Pauli({qubit_index[(0, column)]: "X" for column in range(z_distance)})
    stabilizer = center_of(PauliGroup(centralizers + [logical_x, logical_z]))
    generators = [generator for generator in stabilizer.generators if generator.weight]
    return SubsystemCode(generators, [logical_x, logical_z])
