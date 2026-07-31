from more_itertools import interleave
from qdk.ec.profile.propagation.pauli import Pauli
from qdk.ec.profile.stabilizer_code import StabilizerCode


def make_422_code() -> StabilizerCode:
    return make_iceberg_code(4)


def make_iceberg_code(length: int) -> StabilizerCode:
    if (length % 2 == 1) or length < 1:
        raise ValueError(f"Length {length} is not a positive multiple of two.")

    x_berg = 0
    z_berg = length - 1
    generators = [
        Pauli({index: "X" for index in range(length)}),
        Pauli({index: "Z" for index in range(length)}),
    ]
    x_logicals = [Pauli({index: "X", x_berg: "X"}) for index in range(1, length - 1)]
    z_logicals = [Pauli({index: "Z", z_berg: "Z"}) for index in range(1, length - 1)]
    logicals = list(interleave(x_logicals, z_logicals))
    return StabilizerCode(generators, logical_basis=logicals)
