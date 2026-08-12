"""Input/output stabilizer and logical action of a qodec program."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Iterable, Mapping, Sequence, Union
from warnings import warn

import qodec
from paulimer import PauliGroup, symplectic_form_of
from qodec.actions import Stabilize
from qodec.circuits import Program

from .._qodec_compat import EncodingView, realization
from .propagation.conditional import conditional_choi_state
from .propagation.frames import FrameGroup, PauliFrame
from .propagation.groups import subgroup_of
from .propagation.isa_actions import (
    block_operands,
    block_strides,
    build_qubit_map,
    remap_pauli,
)
from .propagation.pauli import Pauli, characters_of, identity
from .propagation.pauli_remap import encoding_qubit_relocation
from .code_algebra import SubsystemCode
from .separable_code import SeparableCode
from .stabilizer_code import StabilizerCode


@dataclass
class CircuitAction:
    """Input/output stabilizers and logical mapping of a program."""

    observables: FrameGroup
    stabilizers: FrameGroup
    mapping: Mapping[Pauli, PauliFrame]

    def is_equivalent_to(
        self, other: "CircuitAction", modulo_paulis: bool = False
    ) -> bool:
        return are_equivalent_mod_paulis(self, other) and (
            modulo_paulis or are_outcome_equivalent(self, other)
        )


def input_qubits_of(program: Program) -> frozenset[int]:
    seen: set[int] = set()
    prepared: set[int] = set()
    strides = block_strides(program.isa)
    operands_flat = block_operands(program)
    operand_offset = 0
    for call in program.instructions:
        instruction = program.lookup(call.mnemonic)
        operand_count = len(call.inputs)
        call_operands = operands_flat[operand_offset : operand_offset + operand_count]
        operand_offset += operand_count
        qubit_map = build_qubit_map(call, call_operands, strides)
        for action in instruction.action:
            touched: set[int] = set()
            if isinstance(action, Stabilize):
                for pauli_str in action.operators:
                    remapped = remap_pauli(pauli_str, qubit_map)
                    support = set(remapped.support)
                    touched |= support
                    if len(support) == 1:
                        qubit = next(iter(support))
                        if qubit not in seen:
                            prepared.add(qubit)
            else:
                touched |= set(qubit_map.values())
            seen |= touched
    return frozenset(range(program.qubit_count)) - prepared


def action_of(
    program: Program,
    with_respect_to: Union[
        SubsystemCode, tuple[SubsystemCode, SubsystemCode], None
    ] = None,
) -> CircuitAction:
    if with_respect_to is None:
        return _action_of(program, input_qubits=sorted(input_qubits_of(program)))
    if isinstance(with_respect_to, SubsystemCode):
        with_respect_to = (with_respect_to, with_respect_to)
    code_in, code_out = with_respect_to
    physical = _action_of(
        program,
        input_qubits=sorted(code_in.support),
        codespace_projector=tuple(code_in.stabilizers),
        output_support=sorted(code_out.support),
    )
    return _decode(physical, with_respect_to=(code_in, code_out))


def _action_of(
    program: Program,
    *,
    input_qubits: Sequence[int],
    codespace_projector: Sequence[Pauli] = (),
    output_support: Sequence[int] | None = None,
) -> CircuitAction:
    auxiliary_origin = _aux_origin_of(
        program,
        input_qubits=input_qubits,
        codespace_projector=codespace_projector,
        output_support=output_support,
    )
    choi = conditional_choi_state(
        program,
        input_qubits=input_qubits,
        codespace_projector=codespace_projector,
        aux_origin=auxiliary_origin,
    ).group
    auxiliary = {auxiliary_origin + offset for offset in range(len(input_qubits))}
    physical_support = frozenset(
        range(program.qubit_count) if output_support is None else output_support
    )
    stabilizers_out, stabilizers_in, logicals = choi.partition(over=physical_support)
    auxiliary_to_input = {
        auxiliary_origin + offset: qubit for offset, qubit in enumerate(input_qubits)
    }
    return _assemble_action(
        stabilizers_out,
        stabilizers_in,
        logicals,
        auxiliary=auxiliary,
        auxiliary_to_input=auxiliary_to_input,
        physical_support=physical_support,
    )


def _assemble_action(
    stabilizers_out: FrameGroup,
    stabilizers_in: FrameGroup,
    logicals: FrameGroup,
    *,
    auxiliary: set[int],
    auxiliary_to_input: Mapping[int, int],
    physical_support: frozenset[int],
) -> CircuitAction:
    def input_adjust(pauli: Pauli) -> Pauli:
        relabeled = Pauli(
            {
                auxiliary_to_input[qubit]: pauli[qubit]
                for qubit in set(pauli.support) & auxiliary
            }
        ) * identity(pauli.phase)
        return _complex_conjugate_of(relabeled)

    logicals = logicals % (stabilizers_in | stabilizers_out)
    to_input = _abs_restricting_to(auxiliary)
    to_output = _restricting_to(physical_support)
    mapping = {
        input_adjust(to_input(framed.pauli)): PauliFrame(
            to_output(framed.pauli), framed.frame
        )
        for framed in logicals.standardized().generators
    }
    observables = FrameGroup(
        PauliFrame(input_adjust(framed.pauli), framed.frame)
        for framed in stabilizers_in.standardized().generators
    )
    return CircuitAction(observables, stabilizers_out.standardized(), mapping)


def _aux_origin_of(
    program: Program,
    *,
    input_qubits: Sequence[int],
    codespace_projector: Sequence[Pauli],
    output_support: Sequence[int] | None,
) -> int:
    support = set(range(program.qubit_count)) | set(input_qubits)
    for stabilizer in codespace_projector:
        support |= set(stabilizer.support)
    if output_support is not None:
        support |= set(output_support)
    return max(support) + 1 if support else 0


def _decode(
    action: CircuitAction,
    *,
    with_respect_to: tuple[SubsystemCode, SubsystemCode],
) -> CircuitAction:
    _validate(action, with_respect_to=with_respect_to)
    code_in, code_out = with_respect_to
    stabilizers_group = action.stabilizers.unframed

    def phase_of(pauli: Pauli) -> Pauli:
        return _phase_of(pauli, within=stabilizers_group)

    code_out = SubsystemCode(
        [phase_of(generator) * generator for generator in code_out.stabilizers],
        logical_basis=code_out.logical_basis,
        gauge_basis=code_out.gauge_basis,
    )
    observables = _logical_form_of(action.observables, with_respect_to=code_in)
    stabilizers = _logical_form_of(action.stabilizers, with_respect_to=code_out)
    logicals_in = [code_in.logical_action_of(key) for key in action.mapping]
    logicals_out = [
        PauliFrame(code_out.logical_action_of(value.pauli), value.frame)
        for value in action.mapping.values()
    ]
    decoded = CircuitAction(
        observables, stabilizers, dict(zip(logicals_in, logicals_out))
    )
    decoded.mapping = _standard_form_of(decoded.mapping, decoded)
    return decoded


def _phase_of(pauli: Pauli, *, within: PauliGroup) -> Pauli:
    reduced = (PauliGroup([pauli]) % within).generators[0]
    phases = (
        [reduced * identity(1j**exponent) for exponent in within.phases]
        if not reduced.weight
        else []
    )
    if len(phases) != 1:
        raise ValueError(f"{pauli} does not have a unique phase.")
    return phases[0]


def _complex_conjugate_of(pauli: Pauli) -> Pauli:
    y_count = sum(character == "Y" for character in characters_of(pauli).values())
    return pauli * identity((-1) ** (y_count % 2))


def _abs_restricting_to(support: Iterable[int]) -> Callable[[Pauli], Pauli]:
    support_set = frozenset(support)
    return lambda pauli: Pauli(
        {qubit: pauli[qubit] for qubit in set(pauli.support) & support_set}
    )


def _restricting_to(support: Iterable[int]) -> Callable[[Pauli], Pauli]:
    support_set = frozenset(support)

    def restrict(pauli: Pauli) -> Pauli:
        return Pauli(
            {qubit: pauli[qubit] for qubit in set(pauli.support) & support_set}
        ) * identity(pauli.phase)

    return restrict


def _logical_form_of(
    group: FrameGroup, *, with_respect_to: SubsystemCode
) -> FrameGroup:
    logical_action = FrameGroup(
        PauliFrame(with_respect_to.logical_action_of(framed.pauli), framed.frame)
        for framed in group.generators
    )
    return FrameGroup(
        framed
        for framed in logical_action.standardized().generators
        if framed.pauli.weight
    )


def _validate(
    action: CircuitAction,
    *,
    with_respect_to: tuple[SubsystemCode, SubsystemCode],
) -> None:
    code_in, code_out = with_respect_to
    observables_group = action.observables.unframed
    stabilizers_group = action.stabilizers.unframed
    _validate_group(observables_group, against=code_in)
    _validate_group(stabilizers_group, against=code_out)
    observables = observables_group % (observables_group % code_in.stabilizer)
    stabilizers = stabilizers_group % (stabilizers_group % code_out.stabilizer)
    relative_syndrome = observables % stabilizers
    if -Pauli.identity() in relative_syndrome.generators:
        raise ValueError("Syndrome mapping is non-linear.")
    if any(
        complex(generator.phase) != generator.phase
        for generator in relative_syndrome.generators
    ):
        warn("Output code signs are conditional.")


def _validate_group(group: PauliGroup, *, against: SubsystemCode) -> None:
    quotient = PauliGroup(against.stabilizers) % group
    if sum(generator.weight for generator in quotient.generators) > 0:
        raise ValueError(
            "Circuit generators do not include the respective code stabilizers."
        )
    if not against.support >= set(group.support):
        raise ValueError("Code support does not include the circuit support.")


def _shuffled(pauli: Pauli, mapping: Mapping[int, int]) -> Pauli:
    return Pauli(
        {mapping.get(qubit, qubit): pauli[qubit] for qubit in pauli.support}
    ) * identity(pauli.phase)


def _standard_form_of(
    mapping: Mapping[Pauli, PauliFrame], action: CircuitAction
) -> dict[Pauli, PauliFrame]:
    input_group = PauliGroup(
        [_quotient_of(key, action.observables.unframed) for key in mapping]
    )
    output_group = FrameGroup(
        _quotient_framed(value, action.stabilizers) for value in mapping.values()
    )
    indicators = list(_standard_indicators_of(input_group))
    standard_in = subgroup_of(input_group, indicated_by=indicators)
    standard_out = output_group.subgroup(indicators)
    symplectic_indicators = list(
        _indicators_of(standard_in, transformed_by=symplectic_form_of)
    )
    symplectic_in = subgroup_of(
        standard_in, indicated_by=symplectic_indicators
    ).generators
    symplectic_out = standard_out.subgroup(symplectic_indicators).generators
    return {
        abs(operator_in): operator_out * (operator_in.phase**3)
        for operator_in, operator_out in zip(symplectic_in, symplectic_out)
    }


def _quotient_of(pauli: Pauli, group: PauliGroup) -> Pauli:
    return (PauliGroup([pauli]) % group).generators[0]


def _quotient_framed(framed: PauliFrame, group: FrameGroup) -> PauliFrame:
    return (FrameGroup([framed]) % group).generators[0]


def _standard_indicators_of(group: PauliGroup) -> Iterable[list[bool]]:
    return _indicators_of(
        group,
        transformed_by=lambda generators: PauliGroup(generators).standard_generators,
    )


def _indicators_of(
    group: PauliGroup,
    transformed_by: Callable[[Sequence[Pauli]], Iterable[Pauli]],
) -> Iterable[list[bool]]:
    generator_count = len(group.generators)
    if generator_count == 0:
        return
    base = max(group.support) + 1 if group.support else 0
    primary_map = {qubit: qubit for qubit in group.support}
    generators = [
        _shuffled(generator, primary_map) * Pauli({base + index: "Z"})
        for index, generator in enumerate(group.generators)
    ]
    for generator in transformed_by(generators):
        indicator = [False] * generator_count
        for index in generator.support:
            if index >= base:
                indicator[index - base] = True
        yield indicator


def _unsigned(group: PauliGroup) -> PauliGroup:
    return PauliGroup([abs(generator) for generator in group.generators])


def are_equivalent_mod_paulis(action1: CircuitAction, action2: CircuitAction) -> bool:
    if _unsigned(action1.observables.unframed) != _unsigned(
        action2.observables.unframed
    ) or _unsigned(action1.stabilizers.unframed) != _unsigned(
        action2.stabilizers.unframed
    ):
        return False
    mapping1 = _standard_form_of(action1.mapping, action1)
    mapping2 = _standard_form_of(action2.mapping, action2)
    return _abs_of(mapping1.keys()) == _abs_of(mapping2.keys()) and _abs_of(
        value.pauli for value in mapping1.values()
    ) == _abs_of(value.pauli for value in mapping2.values())


def _abs_of(iterable: Iterable[Pauli]) -> list[Pauli]:
    return list(map(abs, iterable))


def are_outcome_equivalent(action1: CircuitAction, action2: CircuitAction) -> bool:
    items1 = _outcome_items(action1)
    items2 = _outcome_items(action2)
    if len(items1) != len(items2):
        return False
    conditions1: list[Pauli] = []
    conditions2: list[Pauli] = []
    for (phase1, frame1, correctable1), (
        phase2,
        frame2,
        correctable2,
    ) in zip(items1, items2):
        if (correctable1 and frame1) or (correctable2 and frame2):
            continue
        conditions1.append(
            Pauli({2 * outcome: "Z" for outcome in frame1}) * identity(phase1)
        )
        conditions2.append(
            Pauli({2 * outcome + 1: "Z" for outcome in frame2}) * identity(phase2)
        )
    products = [left * right for left, right in zip(conditions1, conditions2)]
    for generator in PauliGroup(products).standard_generators:
        only1 = sum(qubit % 2 == 0 for qubit in generator.support)
        only2 = sum(qubit % 2 == 1 for qubit in generator.support)
        if 0 in (only1, only2) and only1 + only2 > 0:
            return False
        if generator.weight == 0 and complex(generator.phase) != 1:
            return False
    return PauliGroup(conditions1).binary_rank == PauliGroup(conditions2).binary_rank


def _outcome_items(
    action: CircuitAction,
) -> list[tuple[complex, frozenset[int], bool]]:
    mapping = _standard_form_of(action.mapping, action)
    items = []
    for framed in action.observables.standardized().generators:
        items.append((framed.pauli.phase, framed.frame, False))
    for framed in action.stabilizers.standardized().generators:
        items.append((framed.pauli.phase, framed.frame, False))
    for key in mapping:
        items.append((key.phase, frozenset(), False))
    for value in mapping.values():
        items.append((value.pauli.phase, value.frame, True))
    return items


def objective_program_of(gadget: qodec.Gadget) -> Program:
    instruction = gadget.implements
    input_count, output_count = _objective_logical_counts(gadget)
    unit = qodec.instructions.BlockOperand("objective")
    synthetic = qodec.Instruction(
        mnemonic=instruction.mnemonic,
        inputs=[unit for _ in range(input_count)],
        outputs=[unit for _ in range(output_count)],
        flags=list(instruction.flags),
        action=list(instruction.action),
    )
    isa = _objective_isa(synthetic)
    binding = [*range(input_count), *range(output_count)]
    call = qodec.instructions.InstructionCall(
        instruction.mnemonic,
        inputs={str(index): value for index, value in enumerate(binding)},
    )
    return Program([call], isa)


def _objective_isa(
    instruction: qodec.Instruction,
) -> qodec.InstructionSet:
    block = qodec.instructions.Block("objective", encodes=1)
    return qodec.InstructionSet(
        name="objective", blocks=[block], instructions=[instruction]
    )


def _objective_logical_counts(gadget: qodec.Gadget) -> tuple[int, int]:
    channel = realization(gadget)
    return (
        sum(len(list(encoding.code.x)) for encoding in channel.encoding_in),
        sum(len(list(encoding.code.x)) for encoding in channel.encoding_out),
    )


def objective_codes_of(
    gadget: qodec.Gadget,
) -> tuple[SeparableCode, SeparableCode]:
    input_count, output_count = _objective_logical_counts(gadget)
    return (
        _identity_codes_over(range(input_count)),
        _identity_codes_over(range(output_count)),
    )


def _identity_codes_over(qubit_indices: Sequence[int] | range) -> SeparableCode:
    blocks = [
        StabilizerCode(
            [],
            logical_basis=[
                Pauli({qubit: "X"}),
                Pauli({qubit: "Z"}),
            ],
        )
        for qubit in qubit_indices
    ]
    return SeparableCode(*blocks)


def realization_program_of(gadget: qodec.Gadget) -> Program:
    channel = realization(gadget)
    return Program(channel.instructions, channel.isa)


def realization_codes_of(
    gadget: qodec.Gadget,
) -> tuple[SeparableCode, SeparableCode]:
    channel = realization(gadget)
    return (
        _stack_encodings(channel.encoding_in),
        _stack_encodings(channel.encoding_out),
    )


def _stack_encodings(encodings: Sequence[EncodingView]) -> SeparableCode:
    blocks = []
    for encoding in encodings:
        code = SubsystemCode.from_qodec(encoding.code)
        blocks.append(code.relocated(encoding_qubit_relocation(encoding)))
    return SeparableCode(*blocks)


def gadget_objective_action_of(gadget: qodec.Gadget) -> CircuitAction:
    codes_in, codes_out = objective_codes_of(gadget)
    return action_of(
        objective_program_of(gadget),
        with_respect_to=(codes_in, codes_out),
    )


def gadget_realization_action_of(gadget: qodec.Gadget) -> CircuitAction:
    codes_in, codes_out = realization_codes_of(gadget)
    return action_of(
        realization_program_of(gadget),
        with_respect_to=(codes_in, codes_out),
    )


def gadget_action_mismatch(gadget: qodec.Gadget) -> str | None:
    expected = gadget_objective_action_of(gadget)
    actual = gadget_realization_action_of(gadget)
    if expected.is_equivalent_to(actual):
        return None
    if expected.is_equivalent_to(actual, modulo_paulis=True):
        return "logical action matches up to Pauli signs but not outcome-wise"
    return "logical action differs between objective and realisation"


__all__ = [
    "CircuitAction",
    "action_of",
    "are_equivalent_mod_paulis",
    "are_outcome_equivalent",
    "gadget_action_mismatch",
    "gadget_objective_action_of",
    "gadget_realization_action_of",
    "input_qubits_of",
    "objective_codes_of",
    "objective_program_of",
    "realization_codes_of",
    "realization_program_of",
]
