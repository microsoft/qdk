"""Input/output stabilizer and logical action of a qodec program."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Iterable, Mapping, Sequence, Union
from warnings import warn

import qodec as qc
from paulimer import PauliGroup
from qodec.actions import Stabilize
from qodec.circuits import Program

from .._layout import ProgramLayout
from .propagation.conditional import conditional_choi_state
from .propagation.frames import FrameGroup, PauliFrame
from .propagation.interpreter import program_of
from .propagation.isa_actions import remap_pauli
from .propagation.pauli import (
    Pauli,
    complex_conjugate_of,
    identity,
    relabel,
    restrict,
)
from .propagation.pauli_remap import encoding_qubit_relocation
from .code_algebra import SubsystemCode, subsystem_code_of
from .separable_code import SeparableCode
from .stabilizer_code import StabilizerCode


@dataclass
class ChannelAction:
    """Input/output stabilizers and logical mapping of a program."""

    observables: FrameGroup
    stabilizers: FrameGroup
    mapping: Mapping[Pauli, PauliFrame]

    def is_equivalent_to(
        self, other: "ChannelAction", *, modulo_paulis: bool = False
    ) -> bool:
        return are_equivalent_mod_paulis(self, other) and (
            modulo_paulis or are_outcome_equivalent(self, other)
        )

    def why_not_equivalent_to(self, other: "ChannelAction") -> str:
        if self.is_equivalent_to(other):
            return ""
        if self.is_equivalent_to(other, modulo_paulis=True):
            return "Channels differ in their outcome-dependent Pauli signs."
        return "Channels differ."

    def __str__(self) -> str:
        return (
            f"observables: {self.observables}\n"
            f"stabilizers: {self.stabilizers}\n"
            f"mapping: {self.mapping}"
        )


def input_qubits_of(program: Program) -> frozenset[int]:
    seen: set[int] = set()
    prepared: set[int] = set()
    layout = ProgramLayout.of(program)
    for call in program.instructions:
        instruction = program.lookup(call.mnemonic)
        qubit_map = layout.call_qubit_map(call)
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
    return frozenset(range(layout.total_qubits)) - prepared


def action_of(
    program: Program,
    with_respect_to: Union[
        SubsystemCode, tuple[SubsystemCode, SubsystemCode], None
    ] = None,
) -> ChannelAction:
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
) -> ChannelAction:
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
        range(ProgramLayout.of(program).total_qubits)
        if output_support is None
        else output_support
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
) -> ChannelAction:
    def input_adjust(pauli: Pauli) -> Pauli:
        relabeled = Pauli(
            {
                auxiliary_to_input[qubit]: pauli[qubit]
                for qubit in set(pauli.support) & auxiliary
            }
        ) * identity(pauli.phase)
        return complex_conjugate_of(relabeled)

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
    return ChannelAction(observables, stabilizers_out.standardized(), mapping)


def _aux_origin_of(
    program: Program,
    *,
    input_qubits: Sequence[int],
    codespace_projector: Sequence[Pauli],
    output_support: Sequence[int] | None,
) -> int:
    support = set(range(ProgramLayout.of(program).total_qubits)) | set(input_qubits)
    for stabilizer in codespace_projector:
        support |= set(stabilizer.support)
    if output_support is not None:
        support |= set(output_support)
    return max(support) + 1 if support else 0


def _decode(
    action: ChannelAction,
    *,
    with_respect_to: tuple[SubsystemCode, SubsystemCode],
) -> ChannelAction:
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
    input_generators = [
        _quotient_of(key, action.observables.unframed) for key in action.mapping
    ]
    output_generators = FrameGroup(
        _quotient_framed(value, action.stabilizers) for value in action.mapping.values()
    )
    indexed_inputs = FrameGroup(
        PauliFrame(generator, frozenset({index}))
        for index, generator in enumerate(input_generators)
    )
    mapping = {}
    for basis_element in code_in.logical_basis:
        target = _quotient_of(basis_element, action.observables.unframed)
        # A logical with no image is normal here, not a failure to characterize:
        # a destructive measurement produces both cases below.
        if not target.weight:
            # Read out by the circuit rather than carried forward.
            continue
        factorization = indexed_inputs.factorization_of(target)
        if factorization is None:
            # Nothing the channel carries reproduces it, so no output holds it.
            continue
        factors: frozenset[int] = frozenset()
        for factor in factorization:
            factors ^= factor.frame
        output = output_generators.subgroup(
            [[index in factors for index in range(len(input_generators))]]
        ).generators[0]
        mapping[code_in.logical_action_of(target)] = PauliFrame(
            code_out.logical_action_of(output.pauli), output.frame
        ) * (target.phase**3)
    return ChannelAction(observables, stabilizers, mapping)


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


def _abs_restricting_to(support: Iterable[int]) -> Callable[[Pauli], Pauli]:
    support_set = frozenset(support)
    return lambda pauli: Pauli(
        {qubit: pauli[qubit] for qubit in set(pauli.support) & support_set}
    )


def _restricting_to(support: Iterable[int]) -> Callable[[Pauli], Pauli]:
    support_set = frozenset(support)
    return lambda pauli: restrict(pauli, support_set)


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
    action: ChannelAction,
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
        warn("Output code signs are conditional.", RuntimeWarning, stacklevel=3)


def _validate_group(group: PauliGroup, *, against: SubsystemCode) -> None:
    quotient = PauliGroup(against.stabilizers) % group
    if sum(generator.weight for generator in quotient.generators) > 0:
        raise ValueError(
            "Circuit generators do not include the respective code stabilizers."
        )
    if not against.support >= set(group.support):
        raise ValueError("Code support does not include the circuit support.")


def _quotient_of(pauli: Pauli, group: PauliGroup) -> Pauli:
    return (PauliGroup([pauli]) % group).generators[0]


def _quotient_framed(framed: PauliFrame, group: FrameGroup) -> PauliFrame:
    return (FrameGroup([framed]) % group).generators[0]


def _unsigned(group: PauliGroup) -> PauliGroup:
    return PauliGroup([abs(generator) for generator in group.generators])


def are_equivalent_mod_paulis(action1: ChannelAction, action2: ChannelAction) -> bool:
    """Whether two actions agree once measurement-dependent signs are ignored.

    Precondition: both actions must be decoded against the same logical
    labelling, because the mappings are compared key by key rather than
    canonicalized first. Actions produced by :func:`action_of` for the same
    pair of codes satisfy this; two actions decoded against different logical
    bases for the same code do not.
    """
    if _unsigned(action1.observables.unframed) != _unsigned(
        action2.observables.unframed
    ) or _unsigned(action1.stabilizers.unframed) != _unsigned(
        action2.stabilizers.unframed
    ):
        return False
    mapping1 = {abs(key): abs(value.pauli) for key, value in action1.mapping.items()}
    mapping2 = {abs(key): abs(value.pauli) for key, value in action2.mapping.items()}
    return mapping1 == mapping2


def _abs_of(iterable: Iterable[Pauli]) -> list[Pauli]:
    return list(map(abs, iterable))


def are_outcome_equivalent(action1: ChannelAction, action2: ChannelAction) -> bool:
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
    action: ChannelAction,
) -> list[tuple[complex, frozenset[int], bool]]:
    items = []
    for framed in action.observables.standardized().generators:
        items.append((framed.pauli.phase, framed.frame, False))
    for framed in action.stabilizers.standardized().generators:
        items.append((framed.pauli.phase, framed.frame, False))
    mapping = sorted(action.mapping.items(), key=lambda item: _sort_key(item[0]))
    for key, _ in mapping:
        items.append((key.phase, frozenset(), False))
    for _, value in mapping:
        items.append((value.pauli.phase, value.frame, True))
    return items


def _sort_key(pauli: Pauli) -> tuple[tuple[int, ...], tuple[str, ...]]:
    """A structural order, so comparison does not depend on Pauli formatting."""
    return tuple(pauli.support), tuple(str(character) for character in pauli.characters)


def declared_program_of(gadget: qc.Gadget) -> Program:
    instruction = gadget.implements
    input_count, output_count = _declared_logical_counts(gadget)
    unit = qc.instructions.BlockOperand("declared")
    synthetic = qc.Instruction(
        mnemonic=instruction.mnemonic,
        inputs=[unit for _ in range(input_count)],
        outputs=[unit for _ in range(output_count)],
        flags=list(instruction.flags),
        action=list(instruction.action),
    )
    isa = _declared_isa(synthetic)
    call = qc.instructions.InstructionCall(
        instruction.mnemonic,
        inputs={str(index): index for index in range(input_count)},
        outputs={str(index): index for index in range(output_count)},
    )
    return Program([call], isa)


def _declared_isa(
    instruction: qc.Instruction,
) -> qc.InstructionSet:
    block = qc.instructions.Block("declared", encodes=1)
    return qc.InstructionSet(
        name="declared", blocks=[block], instructions=[instruction]
    )


def _declared_logical_counts(gadget: qc.Gadget) -> tuple[int, int]:
    return (
        sum(len(list(encoding.code.x)) for encoding in gadget.inputs),
        sum(len(list(encoding.code.x)) for encoding in gadget.outputs),
    )


def declared_codes_of(
    gadget: qc.Gadget,
) -> tuple[SeparableCode, SeparableCode]:
    input_count, output_count = _declared_logical_counts(gadget)
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


def realized_codes_of(
    gadget: qc.Gadget,
) -> tuple[SeparableCode, SeparableCode]:
    return (
        _stack_encodings(gadget.inputs),
        _stack_encodings(gadget.outputs),
    )


def _stack_encodings(encodings: Sequence[qc.Encoding]) -> SeparableCode:
    blocks = []
    for encoding in encodings:
        code = subsystem_code_of(encoding.code)
        blocks.append(code.relocated(encoding_qubit_relocation(encoding)))
    return SeparableCode(*blocks)


def declared_action_of(gadget: qc.Gadget) -> ChannelAction:
    codes_in, codes_out = declared_codes_of(gadget)
    return action_of(
        declared_program_of(gadget),
        with_respect_to=(codes_in, codes_out),
    )


def realized_action_of(gadget: qc.Gadget) -> ChannelAction:
    codes_in, codes_out = realized_codes_of(gadget)
    return action_of(
        program_of(gadget),
        with_respect_to=(codes_in, codes_out),
    )


def gadget_action_mismatch(gadget: qc.Gadget) -> str | None:
    expected = declared_action_of(gadget)
    actual = realized_action_of(gadget)
    if expected.is_equivalent_to(actual):
        return None
    if expected.is_equivalent_to(actual, modulo_paulis=True):
        return "logical action matches up to Pauli signs but not outcome-wise"
    return "logical action differs between declared and realized"


__all__ = [
    "ChannelAction",
    "action_of",
    "are_equivalent_mod_paulis",
    "are_outcome_equivalent",
    "gadget_action_mismatch",
    "declared_action_of",
    "realized_action_of",
    "input_qubits_of",
    "declared_codes_of",
    "declared_program_of",
    "realized_codes_of",
]
