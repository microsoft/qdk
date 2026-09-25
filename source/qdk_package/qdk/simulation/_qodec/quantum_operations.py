from dataclasses import dataclass
from math import pi


@dataclass(frozen=True)
class LogicalSlot:
    block: int | str
    index: int
    block_type: str


@dataclass(frozen=True)
class RestoreMeasured:
    target: int | LogicalSlot
    value: bool


@dataclass(frozen=True)
class Operation:
    name: str
    targets: tuple[int | LogicalSlot, ...]
    angle: float | str | None = None


@dataclass(frozen=True)
class FrameUpdate:
    """A Pauli tracked in the program's Pauli frame rather than run as a gate.

    Frame updates are noiseless and consume no noise samples. Each encoded layer
    lowers one through its code's logical operator, and the backend folds the
    physical Pauli into the simulated state exactly, which is equivalent to
    classical frame tracking.
    """

    pauli: str
    target: int | LogicalSlot

    def __post_init__(self) -> None:
        if self.pauli not in ("x", "y", "z"):
            raise ValueError(f"Frame updates must be x, y, or z, not {self.pauli!r}")


def local_indices(operation: Operation) -> tuple[int, ...]:
    indices = []
    for target in operation.targets:
        if not isinstance(target, int):
            raise NotImplementedError(
                "This operation requires local integer qubit indices"
            )
        indices.append(target)
    return tuple(indices)


_ROTATIONS = {
    "t": ("rz", pi / 4),
    "t_adj": ("rz", -pi / 4),
    "s": ("rz", pi / 2),
    "s_adj": ("rz", -pi / 2),
    "sx": ("rx", pi / 2),
    "sx_adj": ("rx", -pi / 2),
}


def decompose_rotations(operation: Operation) -> tuple[Operation, ...] | None:
    if operation.name not in _ROTATIONS:
        return None
    if len(operation.targets) != 1 or operation.angle is not None:
        raise ValueError(
            f"Operation {operation.name!r} expects one target and no angle"
        )
    rotation, angle = _ROTATIONS[operation.name]
    return (Operation(rotation, operation.targets, angle),)
