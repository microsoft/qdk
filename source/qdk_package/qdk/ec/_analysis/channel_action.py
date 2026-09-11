"""Input/output stabilizer and logical action of a qodec program."""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Mapping, Sequence, Union

import qodec as qc
from paulimer import PauliGroup
from qodec.actions import Stabilize
from qodec.gadgets import Circuit

from .._layout import ProgramLayout
from .propagation.conditional import conditional_choi_state
from .propagation.frames import FrameGroup, PauliFrame
from .propagation.interpreter import program_of
from .propagation.isa_actions import remap_pauli
from .propagation.pauli import (
    Pauli,
    complex_conjugate_of,
    identity,
    restrict,
)
from .propagation.pauli_remap import encoding_qubit_relocation
from .code_algebra import SubsystemCode, subsystem_code_of
from .separable_code import SeparableCode
from .stabilizer_code import StabilizerCode


@dataclass(init=False, repr=False, match_args=False, slots=True)
class ChannelAction:
    """Opaque channel behavior returned by ``GadgetProfile.action`` or ``.objective``.

    Compare results with :meth:`is_equivalent_to` or
    :meth:`why_not_equivalent_to`. Direct construction and access to the
    internal stabilizers, observables, and logical mapping are not supported.
    All internal frames index simulation measurement outcomes, including input
    preparation, code projection, and resets, not Circuit.readouts positions.
    """

    _observables: FrameGroup
    _stabilizers: FrameGroup
    _mapping: Mapping[Pauli, PauliFrame]

    def __new__(cls) -> "ChannelAction":
        raise TypeError(
            "ChannelAction cannot be constructed directly; "
            "use GadgetProfile.action or GadgetProfile.objective"
        )

    @classmethod
    def _create(
        cls,
        observables: FrameGroup,
        stabilizers: FrameGroup,
        mapping: Mapping[Pauli, PauliFrame],
    ) -> "ChannelAction":
        action = object.__new__(cls)
        action._observables = observables
        action._stabilizers = stabilizers
        action._mapping = mapping
        return action

    def is_equivalent_to(
        self, other: "ChannelAction", *, modulo_paulis: bool = False
    ) -> bool:
        """Compare operators and their joint outcome-sign relations.

        Outcome labels are local to each action, so renumbering them does not
        change equivalence. Outcome-dependent signs on mapping images are
        compared modulo Pauli corrections. With modulo_paulis, ignore all signs.
        """
        if self is other:
            return True
        return are_equivalent_mod_paulis(self, other) and (
            modulo_paulis or are_outcome_equivalent(self, other)
        )

    def why_not_equivalent_to(self, other: "ChannelAction") -> str:
        """Return the first difference, or an empty string for equivalent actions."""
        if self is other:
            return ""
        for name, expected, actual in (
            ("measured logical observable", self._observables, other._observables),
            ("output logical stabilizer", self._stabilizers, other._stabilizers),
        ):
            expected_group = _unsigned(expected.unframed)
            actual_group = _unsigned(actual.unframed)
            if expected_group == actual_group:
                continue
            for generator in expected_group.standard_generators:
                if actual_group.factorization_of(generator) is None:
                    return (
                        f"Expected {name} {generator}; absent from the circuit's group."
                    )
            for generator in actual_group.standard_generators:
                if expected_group.factorization_of(generator) is None:
                    return f"Circuit has unexpected {name} {generator}."
        expected_mapping = {abs(key): value for key, value in self._mapping.items()}
        actual_mapping = {abs(key): value for key, value in other._mapping.items()}
        for operator in sorted(
            expected_mapping.keys() | actual_mapping.keys(), key=_sort_key
        ):
            expected_image = expected_mapping.get(operator)
            actual_image = actual_mapping.get(operator)
            if (
                expected_image is None
                or actual_image is None
                or abs(expected_image.pauli) != abs(actual_image.pauli)
            ):
                expected_text = (
                    str(expected_image.pauli)
                    if expected_image is not None
                    else "no output image"
                )
                actual_text = (
                    str(actual_image.pauli)
                    if actual_image is not None
                    else "no output image"
                )
                return f"Logical {operator}: expected {expected_text}; circuit gives {actual_text}."
        return _sign_difference(self, other)

    def __str__(self) -> str:
        return (
            f"observables: {self._observables}\n"
            f"stabilizers: {self._stabilizers}\n"
            f"mapping: {self._mapping}"
        )


def input_qubits_of(program: Circuit) -> frozenset[int]:
    seen: set[int] = set()
    prepared: set[int] = set()
    layout = ProgramLayout.of(program)
    for call in program.calls:
        instruction = program.instruction_set.instructions[call.mnemonic]
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
    program: Circuit,
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
    program: Circuit,
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
    result = conditional_choi_state(
        program,
        input_qubits=input_qubits,
        codespace_projector=codespace_projector,
        aux_origin=auxiliary_origin,
    )
    auxiliary = {auxiliary_origin + offset for offset in range(len(input_qubits))}
    physical_support = frozenset(
        range(ProgramLayout.of(program).total_qubits)
        if output_support is None
        else output_support
    )
    random_to_outcome = tuple(result.simulation.random_outcome_indicator.support)
    group = FrameGroup(
        PauliFrame(
            generator.pauli,
            frozenset(random_to_outcome[index] for index in generator.frame),
        )
        for generator in result.group.generators
    )
    stabilizers_out, stabilizers_in, logicals = group.partition(over=physical_support)
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
    mapping = {
        input_adjust(abs(framed.pauli)): PauliFrame(
            restrict(framed.pauli, physical_support), framed.frame
        )
        for framed in logicals.standardized().generators
    }
    observables = FrameGroup(
        PauliFrame(input_adjust(framed.pauli), framed.frame)
        for framed in stabilizers_in.standardized().generators
    )
    return ChannelAction._create(observables, stabilizers_out.standardized(), mapping)


def _aux_origin_of(
    program: Circuit,
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
    observables_group = action._observables.unframed
    stabilizers_group = action._stabilizers.unframed

    def phase_of(pauli: Pauli) -> Pauli:
        return _phase_of(pauli, within=stabilizers_group)

    code_out = SubsystemCode(
        [phase_of(generator) * generator for generator in code_out.stabilizers],
        logical_basis=code_out.logical_basis,
        gauge_basis=code_out.gauge_basis,
    )
    observables = _logical_form_of(action._observables, with_respect_to=code_in)
    stabilizers = _logical_form_of(action._stabilizers, with_respect_to=code_out)
    input_generators = [
        (PauliGroup([key]) % observables_group).generators[0] for key in action._mapping
    ]
    output_generators = FrameGroup(
        (FrameGroup([value]) % action._stabilizers).generators[0]
        for value in action._mapping.values()
    )
    indexed_inputs = FrameGroup(
        PauliFrame(generator, frozenset({index}))
        for index, generator in enumerate(input_generators)
    )
    phased_inputs = indexed_inputs | FrameGroup([PauliFrame(identity(1j))])
    mapping = {}
    for basis_element in code_in.logical_basis:
        target = (PauliGroup([basis_element]) % observables_group).generators[0]
        # A logical with no image is normal here, not a failure to characterize:
        # a destructive measurement produces both cases below.
        if not target.weight:
            # Read out by the circuit rather than carried forward.
            continue
        factorization = indexed_inputs.factorization_of(target)
        phase_extended = factorization is None
        if phase_extended:
            factorization = phased_inputs.factorization_of(target)
        if factorization is None:
            # Nothing the channel carries reproduces it, so no output holds it.
            continue
        factors: frozenset[int] = frozenset()
        for factor in factorization:
            factors ^= factor.frame
        input_product = identity()
        for index, generator in enumerate(input_generators):
            if index in factors:
                input_product *= generator
        output = output_generators.subgroup(
            [[index in factors for index in range(len(input_generators))]]
        ).generators[0]
        if phase_extended:
            output *= target.phase / input_product.phase
        mapping[code_in.logical_action_of(target)] = PauliFrame(
            code_out.logical_action_of(output.pauli), output.frame
        ) * (target.phase**3)
    return ChannelAction._create(observables, stabilizers, mapping)


def _phase_of(pauli: Pauli, *, within: PauliGroup) -> Pauli:
    reduced = (PauliGroup([pauli]) % within).generators[0]
    phases = within.phases
    if reduced.weight or len(phases) != 1:
        raise ValueError(f"{pauli} does not have a unique phase.")
    return reduced * identity(1j ** phases[0])


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
    observables_group = action._observables.unframed
    stabilizers_group = action._stabilizers.unframed
    _validate_group(observables_group, against=code_in)
    _validate_group(stabilizers_group, against=code_out)
    observables = observables_group % (observables_group % code_in.stabilizer)
    stabilizers = stabilizers_group % (stabilizers_group % code_out.stabilizer)
    relative_syndrome = observables % stabilizers
    if -Pauli.identity() in relative_syndrome.generators:
        raise ValueError("Syndrome mapping is non-linear.")


def _validate_group(group: PauliGroup, *, against: SubsystemCode) -> None:
    quotient = PauliGroup(against.stabilizers) % group
    if any(generator.weight for generator in quotient.generators):
        raise ValueError(
            "Circuit generators do not include the respective code stabilizers."
        )
    if not against.support >= set(group.support):
        raise ValueError("Code support does not include the circuit support.")


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
    if _unsigned(action1._observables.unframed) != _unsigned(
        action2._observables.unframed
    ) or _unsigned(action1._stabilizers.unframed) != _unsigned(
        action2._stabilizers.unframed
    ):
        return False
    mapping1 = {abs(key): abs(value.pauli) for key, value in action1._mapping.items()}
    mapping2 = {abs(key): abs(value.pauli) for key, value in action2._mapping.items()}
    return mapping1 == mapping2


def are_outcome_equivalent(action1: ChannelAction, action2: ChannelAction) -> bool:
    return not _sign_difference(action1, action2)


def _outcome_items(
    action: ChannelAction,
) -> list[tuple[complex, frozenset[int], bool]]:
    items = []
    for framed in action._observables.standardized().generators:
        items.append((framed.pauli.phase, framed.frame, False))
    for framed in action._stabilizers.standardized().generators:
        items.append((framed.pauli.phase, framed.frame, False))
    mapping = sorted(action._mapping.items(), key=lambda item: _sort_key(item[0]))
    for key, _ in mapping:
        items.append((key.phase, frozenset(), False))
    for _, value in mapping:
        items.append((value.pauli.phase, value.frame, True))
    return items


def _sort_key(pauli: Pauli) -> tuple[tuple[int, ...], tuple[str, ...]]:
    """A structural order, so comparison does not depend on Pauli formatting."""
    return tuple(pauli.support), tuple(str(character) for character in pauli.characters)


def _sign_difference(expected: ChannelAction, actual: ChannelAction) -> str:
    expected_items = _outcome_items(expected)
    actual_items = _outcome_items(actual)
    if len(expected_items) != len(actual_items):
        return "Different numbers of outcome-sign relations."
    if expected_items == actual_items:
        return ""
    labels = [
        *(
            f"observable {abs(item.pauli)}"
            for item in expected._observables.standardized().generators
        ),
        *(
            f"output stabilizer {abs(item.pauli)}"
            for item in expected._stabilizers.standardized().generators
        ),
        *(
            f"input {abs(operator)}"
            for operator in sorted(expected._mapping, key=_sort_key)
        ),
        *(
            f"output image of {abs(operator)}"
            for operator in sorted(expected._mapping, key=_sort_key)
        ),
    ]
    products = []
    for index, (
        (expected_phase, expected_frame, expected_correctable),
        (actual_phase, actual_frame, actual_correctable),
    ) in enumerate(zip(expected_items, actual_items, strict=True)):
        if (expected_correctable and expected_frame) or (
            actual_correctable and actual_frame
        ):
            continue
        product = (
            Pauli({2 * bit: "Z" for bit in expected_frame})
            * Pauli({2 * bit + 1: "Z" for bit in actual_frame})
            * identity(expected_phase * actual_phase)
        )
        products.append(PauliFrame(product, frozenset({index})))
    support = {qubit for item in products for qubit in item.pauli.support}
    group = FrameGroup(products)
    for side in (0, 1):
        restricted, _, _ = group.partition(
            over={qubit for qubit in support if qubit % 2 == side}
        )
        for witness in restricted.generators:
            if not witness.pauli.weight and witness.pauli.phase == 1:
                continue
            terms = " ⊕ ".join(labels[index] for index in sorted(witness.frame))
            if not witness.pauli.weight:
                return f"Opposite sign parity: {terms}."
            variable = "expected" if side == 0 else "circuit"
            fixed = "circuit" if side == 0 else "expected"
            return f"Sign parity ({terms}): {variable} varies with outcomes; {fixed} is fixed."
    return ""


def declared_program_of(gadget: qc.Gadget) -> Circuit:
    instruction = gadget.implements
    input_count, output_count = _declared_logical_counts(gadget)
    unit = qc.instructions.BlockOperand("declared")
    synthetic = qc.Instruction(
        mnemonic=instruction.mnemonic,
        inputs=[unit for _ in range(input_count)],
        outputs=[unit for _ in range(output_count)],
        parameters=list(instruction.parameters),
        flags=list(instruction.flags),
        action=list(instruction.action),
    )
    isa = _declared_isa(synthetic)
    return Circuit(
        isa,
        json.dumps(
            [
                {
                    instruction.mnemonic: {
                        "operands": list(range(max(input_count, output_count))),
                        "arguments": {
                            parameter.name: parameter.name
                            for parameter in instruction.parameters
                        },
                    }
                }
            ]
        ),
        format="yaml",
    )


def _declared_isa(
    instruction: qc.Instruction,
) -> qc.InstructionSet:
    block = qc.instructions.Block("declared", encodes=1)
    return qc.InstructionSet(
        name="declared", blocks=[block], instructions=[instruction]
    )


def _declared_logical_counts(gadget: qc.Gadget) -> tuple[int, int]:
    return (
        sum(len(encoding.code.x) for encoding in gadget.inputs),
        sum(len(encoding.code.x) for encoding in gadget.outputs),
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


def _stack_encodings(encodings: Sequence[qc.gadgets.Encoding]) -> SeparableCode:
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
    if not are_equivalent_mod_paulis(expected, actual):
        return "logical action differs between declared and realized"
    if not are_outcome_equivalent(expected, actual):
        return "logical action matches up to Pauli signs but not outcome-wise"
    return None


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
