from paulimer import CliffordUnitary, DensePauli

from .quantum_operations import Operation


def synthesize_clifford(clifford: CliffordUnitary) -> tuple[Operation, ...]:
    try:
        import stim
    except ModuleNotFoundError as error:
        if error.name != "stim":
            raise
        raise NotImplementedError(
            "Nonprimitive Clifford synthesis requires the optional Stim package; "
            "Pauli algebra and primitive Clifford operations do not"
        ) from error

    def image(operator: DensePauli) -> stim.PauliString:
        result = stim.PauliString(operator.characters)
        result.sign = operator.phase
        return result

    tableau = stim.Tableau.from_conjugated_generators(
        xs=[image(clifford.image_x(index)) for index in range(clifford.qubit_count)],
        zs=[image(clifford.image_z(index)) for index in range(clifford.qubit_count)],
    )
    operations = []
    for instruction in tableau.to_circuit():
        assert isinstance(instruction, stim.CircuitInstruction)
        targets = tuple(target.value for target in instruction.targets_copy())
        arity = 2 if instruction.name == "CX" else 1
        for offset in range(0, len(targets), arity):
            operations.append(
                Operation(instruction.name.lower(), targets[offset : offset + arity])
            )
    return tuple(operations)
