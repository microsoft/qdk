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

    # Without stim installed (Linux aarch64, Windows ARM64), pyright resolves
    # `import stim` to the local `qdk.stim` package and flags these attributes.
    def image(
        operator: DensePauli,
    ) -> stim.PauliString:  # pyright: ignore[reportAttributeAccessIssue]
        result = stim.PauliString(  # pyright: ignore[reportAttributeAccessIssue]
            operator.characters
        )
        result.sign = operator.phase
        return result

    tableau = stim.Tableau.from_conjugated_generators(  # pyright: ignore[reportAttributeAccessIssue]
        xs=[image(clifford.image_x(index)) for index in range(clifford.qubit_count)],
        zs=[image(clifford.image_z(index)) for index in range(clifford.qubit_count)],
    )
    operations = []
    for instruction in tableau.to_circuit():
        assert isinstance(
            instruction,
            stim.CircuitInstruction,  # pyright: ignore[reportAttributeAccessIssue]
        )
        targets = tuple(target.value for target in instruction.targets_copy())
        arity = 2 if instruction.name == "CX" else 1
        for offset in range(0, len(targets), arity):
            operations.append(
                Operation(instruction.name.lower(), targets[offset : offset + arity])
            )
    return tuple(operations)
