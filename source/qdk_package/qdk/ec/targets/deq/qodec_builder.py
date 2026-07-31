"""Build a qodec :class:`~qodec.Qodec` from ``.deq`` source.

This is the inverse of :mod:`.source_emitter` (``to_deq``). A ``.deq`` file
is a *lower-level* artifact than a qodec: it carries the codes, the gadget
circuits, and the check/readout surface, but not the logical instruction
set's action semantics, nor an explicit layer/ISA structure. So
:func:`from_deq` *synthesizes* the two instruction sets a qodec needs:

* a physical (target) ISA, from the stim gates the gadget bodies use, and
* a logical (source) ISA, with one instruction per gadget.

Gadget bodies keep their stim instructions; noise gates (``X_ERROR`` and
friends) are dropped, since qodec gadgets are noiseless. Only ``CODE`` and
``GADGET`` definitions are converted — ``COMPOSE`` and ``PROGRAM`` blocks are
ignored (they are program-level constructs, not part of the code+gadget
library).

The conversion composes with :func:`to_deq` as a stable fixpoint:
``from_deq(to_deq(from_deq(src))) == from_deq(src)``.
"""

from __future__ import annotations

from collections.abc import Callable

import qodec
from qodec.actions import Clifford, Observe, Stabilize
from qodec.codes import Code
from qodec.gadgets import Circuit, Encoding
from qodec.instructions import Block, BlockOperand, Instruction, InstructionSet

from deq.circuit import model as deq_model
from deq.circuit.parser import parse

# Action factory: a callable producing a fresh qodec action list, so no action
# object is shared between synthesized instructions.
_ActionFactory = Callable[[], list[object]]

# stim gate -> (input qubits, output qubits, action factory) per application.
_GATE_TABLE: dict[str, tuple[int, int, _ActionFactory]] = {
    "R": (0, 1, lambda: [Stabilize(["Z_0"])]),
    "RZ": (0, 1, lambda: [Stabilize(["Z_0"])]),
    "RX": (0, 1, lambda: [Stabilize(["X_0"])]),
    "M": (1, 0, lambda: [Observe(["Z_0"])]),
    "MZ": (1, 0, lambda: [Observe(["Z_0"])]),
    "MX": (1, 0, lambda: [Observe(["X_0"])]),
    "H": (1, 1, lambda: [Clifford({"X_0": "Z_0", "Z_0": "X_0"})]),
    "CX": (2, 2, lambda: [Clifford({"X_0": "X_0 X_1", "Z_1": "Z_0 Z_1"})]),
    "CNOT": (2, 2, lambda: [Clifford({"X_0": "X_0 X_1", "Z_1": "Z_0 Z_1"})]),
    "CZ": (2, 2, lambda: [Clifford({"X_0": "X_0 Z_1", "X_1": "Z_0 X_1"})]),
}

# Noise mechanisms and stim annotations dropped from gadget bodies: qodec
# gadgets are noiseless, and checks/observables are recovered structurally.
_NOISE_GATES = frozenset(
    {
        "X_ERROR",
        "Y_ERROR",
        "Z_ERROR",
        "DEPOLARIZE1",
        "DEPOLARIZE2",
        "PAULI_CHANNEL_1",
        "PAULI_CHANNEL_2",
        "CORRELATED_ERROR",
        "ELSE_CORRELATED_ERROR",
        "E",
        "TICK",
        "QUBIT_COORDS",
        "SHIFT_COORDS",
        "DETECTOR",
        "OBSERVABLE_INCLUDE",
    }
)


def from_deq(source: str) -> qodec.Qodec:
    """Build a qodec :class:`~qodec.Qodec` from ``.deq`` ``source`` text.

    Parses the ``.deq`` source with deq's own parser, then reconstructs a
    two-layer codec (a synthesized logical ISA lowering to a synthesized
    physical/stim ISA). Raises :class:`NotImplementedError` if a gadget body
    uses a stim gate outside the supported set (see :data:`_GATE_TABLE`).

    ``COMPOSE`` and ``PROGRAM`` definitions in the source are ignored;
    noise gates and stim annotations are stripped from gadget bodies.
    """
    deq_file = parse(source)

    codes = {
        definition.name: _build_code(definition)
        for definition in deq_file.definitions
        if isinstance(definition, deq_model.CodeDefinition)
    }
    if not codes:
        raise ValueError("from_deq: no CODE definition found in source")

    gadget_defs = sorted(
        (
            definition
            for definition in deq_file.definitions
            if isinstance(definition, deq_model.GadgetDefinition)
        ),
        key=lambda definition: definition.name,
    )

    physical_isa = _build_physical_isa(gadget_defs)
    logical_isa = _build_logical_isa(gadget_defs, codes)
    gadgets = [
        _build_gadget(definition, logical_isa, physical_isa, codes)
        for definition in gadget_defs
    ]

    return qodec.Qodec(
        layers=[
            qodec.Layer(logical_isa, gadgets=gadgets),
            qodec.Layer(physical_isa),
        ],
        name=next(iter(codes)),
    )


def _pauli_product(product: deq_model.PauliProduct) -> str:
    """Render a deq ``PauliProduct`` as a qodec Pauli string (``'Z_0 Z_1'``)."""
    return " ".join(f"{term.pauli}_{term.index}" for term in product.terms)


def _build_code(definition: deq_model.CodeDefinition) -> Code:
    return Code(
        name=definition.name,
        stabilizers=[_pauli_product(stab) for stab in definition.stabilizers],
        x=[_pauli_product(logical.x_operator) for logical in definition.logicals],
        z=[_pauli_product(logical.z_operator) for logical in definition.logicals],
    )


def _body_instructions(
    definition: deq_model.GadgetDefinition,
) -> list[deq_model.Instruction]:
    """The non-noise stim instructions of a gadget body, in order."""
    return [
        statement
        for statement in definition.body
        if isinstance(statement, deq_model.Instruction)
        and statement.name not in _NOISE_GATES
    ]


def _build_physical_isa(
    gadget_defs: list[deq_model.GadgetDefinition],
) -> InstructionSet:
    used_gates: set[str] = set()
    for definition in gadget_defs:
        for instruction in _body_instructions(definition):
            used_gates.add(instruction.name)

    instructions: list[Instruction] = []
    for name in sorted(used_gates):
        if name not in _GATE_TABLE:
            raise NotImplementedError(
                f"from_deq: unsupported stim gate {name!r}; "
                f"supported gates are {sorted(_GATE_TABLE)}"
            )
        n_in, n_out, action_factory = _GATE_TABLE[name]
        instructions.append(
            Instruction(
                name,
                inputs=[BlockOperand("qubit")] * n_in,
                outputs=[BlockOperand("qubit")] * n_out,
                action=action_factory(),
            )
        )
    return InstructionSet(
        name="stim",
        blocks=[Block("qubit", encodes=1)],
        instructions=instructions,
    )


def _build_logical_isa(
    gadget_defs: list[deq_model.GadgetDefinition], codes: dict[str, Code]
) -> InstructionSet:
    instructions = [
        Instruction(
            definition.name,
            inputs=[BlockOperand(port.code_name) for port in definition.input_ports],
            outputs=[BlockOperand(port.code_name) for port in definition.output_ports],
            action=_logical_action(definition),
        )
        for definition in gadget_defs
    ]
    blocks = [Block(name, encodes=len(code.x)) for name, code in codes.items()]
    return InstructionSet(name="logical", blocks=blocks, instructions=instructions)


def _readout_statements(
    definition: deq_model.GadgetDefinition,
) -> list[deq_model.ReadoutStatement]:
    return [
        statement
        for statement in definition.body
        if isinstance(statement, deq_model.ReadoutStatement)
    ]


def _logical_action(definition: deq_model.GadgetDefinition) -> list[object]:
    """Synthesize the logical instruction's action from its READOUTs.

    Each READOUT statement becomes one observed logical outcome. The basis
    cannot be recovered from a ``.deq`` READOUT (it lists only measurement
    records), so the logical-Z observable of each logical qubit is used.
    """
    readouts = _readout_statements(definition)
    if not readouts:
        return []
    return [Observe([f"Z_{index}" for index in range(len(readouts))])]


def _measurement_count(definition: deq_model.GadgetDefinition) -> int:
    """Number of measurement records the (noise-stripped) body produces."""
    count = 0
    for instruction in _body_instructions(definition):
        n_in, n_out, _ = _GATE_TABLE[instruction.name]
        if n_in == 1 and n_out == 0:
            count += sum(
                1
                for target in instruction.targets
                if isinstance(target, deq_model.QubitTarget)
            )
    return count


def _instruction_measurements(instruction: deq_model.Instruction) -> int:
    """Real measurement records produced by a single body instruction."""
    if instruction.name in _NOISE_GATES:
        return 0
    entry = _GATE_TABLE.get(instruction.name)
    if entry is None:
        return 0
    n_in, n_out, _ = entry
    if n_in == 1 and n_out == 0:
        return sum(
            1 for t in instruction.targets if isinstance(t, deq_model.QubitTarget)
        )
    return 0


def _build_checks(
    definition: deq_model.GadgetDefinition, codes: dict[str, Code]
) -> list[list[str]]:
    """Parse ``CHECK rec[-k]`` statements back into qodec check references.

    Inverse of ``to_deq``'s check emission: deq's record stream is
    ``[input-virtual | real | output-virtual]``, so each ``rec[-k]`` resolves
    (relative to the running record count at the statement's position) to a
    global index that maps back to ``in[p].stabilizers[k]``,
    ``circuit.readouts[i]``, or ``out[p].stabilizers[k]``. Single-record checks
    on one output-virtual stabilizer are the coverage checks ``to_deq``
    synthesizes for deterministic preparations; qodec represents that
    implicitly, so they are dropped.
    """
    in_counts = [len(codes[p.code_name].stabilizers) for p in definition.input_ports]
    out_counts = [len(codes[p.code_name].stabilizers) for p in definition.output_ports]
    num_input = sum(in_counts)
    ov_start = num_input + _measurement_count(definition)
    in_offsets = [sum(in_counts[:i]) for i in range(len(in_counts))]
    out_offsets = [sum(out_counts[:i]) for i in range(len(out_counts))]

    def to_reference(global_index: int) -> str:
        if global_index < num_input:
            port = max(
                p for p in range(len(in_counts)) if in_offsets[p] <= global_index
            )
            return f"in[{port}].stabilizers[{global_index - in_offsets[port]}]"
        if global_index < ov_start:
            return f"circuit.readouts[{global_index - num_input}]"
        relative = global_index - ov_start
        port = max(p for p in range(len(out_counts)) if out_offsets[p] <= relative)
        return f"out[{port}].stabilizers[{relative - out_offsets[port]}]"

    checks: list[list[str]] = []
    running = 0
    for statement in definition.body:
        if isinstance(statement, (deq_model.InputPort, deq_model.OutputPort)):
            running += len(codes[statement.code_name].stabilizers)
        elif isinstance(statement, deq_model.Instruction):
            running += _instruction_measurements(statement)
        elif isinstance(statement, deq_model.CheckStatement):
            references = [
                to_reference(running - target.offset)
                for target in statement.targets
                if isinstance(target, deq_model.MeasurementRecordTarget)
            ]
            if len(references) == 1 and references[0].startswith("out["):
                continue
            checks.append(references)
    return checks


def _build_gadget(
    definition: deq_model.GadgetDefinition,
    logical_isa: InstructionSet,
    physical_isa: InstructionSet,
    codes: dict[str, Code],
) -> qodec.Gadget:
    body = "\n".join(_stim_line(instr) for instr in _body_instructions(definition))
    inputs = [
        Encoding(
            code=codes[port.code_name], support=[str(i) for i in port.qubit_indices]
        )
        for port in definition.input_ports
    ]
    outputs = [
        Encoding(
            code=codes[port.code_name], support=[str(i) for i in port.qubit_indices]
        )
        for port in definition.output_ports
    ]

    boundary = "in" if inputs else "out"
    measurement_count = _measurement_count(definition)
    readouts: list[list[str]] = []
    for index, statement in enumerate(_readout_statements(definition)):
        references = [
            f"circuit.readouts[{measurement_count - target.offset}]"
            for target in statement.targets
            if isinstance(target, deq_model.MeasurementRecordTarget)
        ]
        references.append(f"{boundary}[0].z[{index}]")
        readouts.append(references)

    return qodec.Gadget(
        implements=logical_isa.instruction(definition.name),
        circuit=Circuit(physical_isa, body, format="stim"),
        inputs=inputs,
        outputs=outputs,
        checks=_build_checks(definition, codes),
        readouts=readouts,
    )


def _stim_line(instruction: deq_model.Instruction) -> str:
    targets = " ".join(
        str(target.index)
        for target in instruction.targets
        if isinstance(target, deq_model.QubitTarget)
    )
    return f"{instruction.name} {targets}".rstrip()
