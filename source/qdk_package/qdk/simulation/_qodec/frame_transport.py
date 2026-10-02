from collections.abc import Mapping

from paulimer import CliffordUnitary, DensePauli, SparsePauli
from qodec import Gadget

from .action_runtime import prepare_actions
from .call_binding import bind_call
from .clifford_semantics import pauli
from .quantum_instruments import CliffordGate, PauliGate
from .readout_equations import BinarySystem, InconsistentParity, Parity


def _place(operator: DensePauli, positions: tuple[int, ...], width: int) -> DensePauli:
    return DensePauli.from_sparse(
        SparsePauli({positions[index]: operator[index] for index in operator.support}),
        width,
    )


def _image(operator: DensePauli, clifford: CliffordUnitary) -> DensePauli:
    result = DensePauli.identity(clifford.qubit_count)
    for index in operator.support:
        if operator[index] in ("X", "Y"):
            result *= clifford.image_x(index)
        if operator[index] in ("Z", "Y"):
            result *= clifford.image_z(index)
    return result


def circuit_transport(gadget: Gadget) -> Mapping[tuple[int, str, int], Parity] | None:
    if not gadget.inputs or not gadget.outputs or gadget.implements.parameters:
        return None
    if gadget.circuit.effective_format.lstrip(".") in ("qir", "qasm", "openqasm"):
        return None
    calls = tuple(gadget.circuit.calls())
    target = gadget.circuit.instruction_set
    capacities = {block.name: block.encodes for block in target.blocks}
    types = {}
    for encoding in (*gadget.inputs, *gadget.outputs):
        lower_types = tuple(encoding.block_types)
        if not lower_types and len(capacities) == 1:
            lower_types = (next(iter(capacities)),) * len(encoding.support)
        for label, block_type in zip(encoding.support, lower_types):
            if types.setdefault(label, block_type) != block_type:
                return None
    bindings = []
    for call in calls:
        binding = bind_call(target, call)
        if binding.inputs != binding.outputs:
            return None
        bindings.append(binding)
        for operand in binding.inputs:
            types.setdefault(str(operand.label), operand.block_type)
    positions = {}
    width = 0
    for label, block_type in types.items():
        positions[label] = tuple(range(width, width + capacities[block_type]))
        width += capacities[block_type]
    clifford = CliffordUnitary.identity(width)
    for call, binding in zip(calls, bindings):
        program = prepare_actions(
            binding.instruction,
            binding.input_capacity,
            binding.output_capacity,
            call.arguments,
        )
        targets = tuple(
            position
            for operand in binding.inputs
            for position in positions[str(operand.label)]
        )
        for step in program.steps:
            if step.guard is not None:
                return None
            if isinstance(step.instrument, CliffordGate):
                clifford.left_mul_clifford(step.instrument.operator, targets)
            elif not isinstance(step.instrument, PauliGate):
                return None
    generators = {}
    for entry, encoding in enumerate(gadget.inputs):
        support = tuple(
            position for label in encoding.support for position in positions[label]
        )
        for basis in ("stabilizers", "x", "z"):
            for index, text in enumerate(getattr(encoding.code, basis)):
                generators[(entry, basis, index)] = _image(
                    _place(pauli(text, len(support)), support, width), clifford
                )
    transported = {}
    for entry, encoding in enumerate(gadget.outputs):
        support = tuple(
            position for label in encoding.support for position in positions[label]
        )
        for basis in ("x", "z"):
            for index, text in enumerate(getattr(encoding.code, basis)):
                operator = _place(pauli(text, len(support)), support, width)
                equations = []
                for qubit in range(width):
                    for axes in (("X", "Y"), ("Z", "Y")):
                        variables = frozenset(
                            key
                            for key, image in generators.items()
                            if image[qubit] in axes
                        )
                        equations.append(Parity(variables, operator[qubit] in axes))
                try:
                    solution = BinarySystem(equations).solution()
                except InconsistentParity as error:
                    raise ValueError(
                        "Supplied Clifford circuit does not preserve the declared code boundary"
                    ) from error
                transported[(entry, basis, index)] = Parity(
                    frozenset(
                        ("encoding", "in", source, property_name, source_index)
                        for source, property_name, source_index in generators
                        if solution.get((source, property_name, source_index), False)
                    )
                )
    return transported
