from collections.abc import Mapping, Sequence
import re

from paulimer import CliffordUnitary, DensePauli, SparsePauli, UnitaryOpcode

from .quantum_operations import Operation, local_indices

_GATES = {
    "x": (UnitaryOpcode.X, 1),
    "y": (UnitaryOpcode.Y, 1),
    "z": (UnitaryOpcode.Z, 1),
    "h": (UnitaryOpcode.Hadamard, 1),
    "s": (UnitaryOpcode.SqrtZ, 1),
    "s_adj": (UnitaryOpcode.SqrtZInv, 1),
    "sx": (UnitaryOpcode.SqrtX, 1),
    "sx_adj": (UnitaryOpcode.SqrtXInv, 1),
    "cx": (UnitaryOpcode.ControlledX, 2),
    "cz": (UnitaryOpcode.ControlledZ, 2),
    "swap": (UnitaryOpcode.Swap, 2),
}


def pauli(expression: str, width: int = 0) -> DensePauli:
    if width < 0:
        raise ValueError("Pauli width must be nonnegative")
    text = expression.replace("_", "").replace("*", " ").strip()
    indices = [int(index) for index in re.findall(r"[IXYZ](\d+)", text)]
    declared_width = (
        max(indices) + 1 if indices else sum(character in "IXYZ" for character in text)
    )
    text = re.sub(r"I\d+", "", text).strip()
    if text in ("", "+", "-", "i", "+i", "-i"):
        text += "I"
    operator = SparsePauli(text)
    return DensePauli.from_sparse(operator, max(width, declared_width))


def named_clifford(
    name: str, arity: int, angle: float | str | None = None
) -> CliffordUnitary | None:
    if angle is not None:
        return None
    if name == "cy" and arity == 2:
        clifford = CliffordUnitary.identity(2)
        clifford.left_mul_controlled_pauli(DensePauli.z(0, 2), DensePauli.y(1, 2))
        return clifford
    gate = _GATES.get(name)
    if gate is None or gate[1] != arity:
        return None
    clifford = CliffordUnitary.identity(arity)
    clifford.left_mul(gate[0], list(range(arity)))
    return clifford


def clifford_tableau(generators: Mapping[str, str], width: int) -> CliffordUnitary:
    if width < 0:
        raise ValueError("Clifford width must be nonnegative")
    images = {}
    for index in range(width):
        for basis in ("X", "Z"):
            generator = pauli(f"{basis}_{index}", width)
            images[generator.characters] = generator
    for source, target in generators.items():
        generator = pauli(source, width)
        if generator.phase != 1 or generator.characters not in images:
            raise NotImplementedError(
                "Clifford keys must be individual X or Z generators"
            )
        image = pauli(target, width)
        if image.size != width or image.phase not in (1, -1):
            raise ValueError(
                "Clifford images must be Hermitian Paulis of the declared width"
            )
        images[generator.characters] = image
    ordered_images = list(images.values())
    for index, image in enumerate(ordered_images):
        for previous in range(index):
            paired = index // 2 == previous // 2
            if image.commutes_with(ordered_images[previous]) == paired:
                raise ValueError("Clifford images must preserve generator commutation")
    return CliffordUnitary.from_images(ordered_images)


def compose_cliffords(
    steps: Sequence[Operation | CliffordUnitary], width: int
) -> CliffordUnitary | None:
    if not steps:
        return None
    tableau = CliffordUnitary.identity(width)
    for step in steps:
        if isinstance(step, CliffordUnitary):
            if step.qubit_count != width:
                raise ValueError("Composed Cliffords must have the same width")
            tableau.left_mul_clifford(step, list(range(width)))
        else:
            gate = named_clifford(step.name, len(step.targets), step.angle)
            if gate is None:
                return None
            targets = local_indices(step)
            if len(set(targets)) != len(targets) or any(
                target < 0 or target >= width for target in targets
            ):
                raise ValueError(
                    "Clifford targets must be distinct and within the declared width"
                )
            tableau.left_mul_clifford(gate, list(targets))
    return tableau


def lower_clifford(tableau: CliffordUnitary) -> tuple[Operation, ...]:
    for name in (*_GATES, "cy"):
        if tableau == named_clifford(name, tableau.qubit_count):
            return (Operation(name, tuple(range(tableau.qubit_count))),)
    from .stim_synthesis import synthesize_clifford

    return synthesize_clifford(tableau)


def clifford_operations(
    generators: Mapping[str, str], width: int
) -> tuple[Operation, ...]:
    return lower_clifford(clifford_tableau(generators, width))
