"""Check protocol declarations for consistency and valid references."""

from __future__ import annotations

from collections.abc import Iterator
from itertools import chain
import json
import re

import qodec as qc

from .._analysis.propagation.pauli import Pauli
from .._readouts import observe_count_of
from .._references import (
    LogicalSign,
    Outcome,
    ReadoutSign,
    reference_term,
    reference_terms,
)


def _pauli_indices(text: str) -> set[int]:
    return set(Pauli(text).support)


def _instruction_issues(
    instruction: qc.Instruction, blocks: dict[str, int] | None
) -> Iterator[str]:
    parameters = {
        parameter.name: parameter.kind for parameter in instruction.parameters
    }
    if len(parameters) != len(instruction.parameters):
        yield "duplicate parameter names"
    kind = qc.instructions.Parameter.Kind
    capacity = None
    if blocks is not None:
        operands = (*instruction.inputs, *instruction.outputs)
        for operand in operands:
            if operand.block not in blocks:
                yield f"operand references undeclared block type {operand.block!r}"
        if all(
            not operand.is_variadic and operand.block in blocks for operand in operands
        ):
            capacity = max(
                sum(blocks[operand.block] for operand in instruction.inputs),
                sum(blocks[operand.block] for operand in instruction.outputs),
            )
    if len(set(instruction.flags)) != len(instruction.flags):
        yield "duplicate flag names"
    temporary: set[int] = set()
    outcomes = 0
    try:
        actions = instruction.action
    except ValueError as error:
        yield f"action cannot be inspected: {error}"
        return
    for position, action in enumerate(actions):
        label = f"action[{position}]"
        condition = getattr(action, "condition", None)
        if condition is not None:
            for predicate in condition.predicates:
                match = re.fullmatch(r"outcomes\[([0-9]+)\]", predicate)
                if match:
                    if int(match[1]) >= outcomes:
                        yield f"{label}: guard {predicate!r} is out of bounds for {outcomes} preceding outcomes"
                elif (
                    parameters.get(predicate) != kind.BIT
                    or predicate in instruction.flags
                ):
                    yield f"{label}: guard {predicate!r} must name a bit parameter or a preceding outcomes[i]"
        if isinstance(action, qc.actions.Rotate) and isinstance(action.angle, str):
            if parameters.get(action.angle) not in (kind.NUMBER, kind.INTEGER):
                yield f"{label}: rotation angle {action.angle!r} must name a number or integer parameter"
        if isinstance(action, qc.actions.Stabilize):
            operators = action.operators
        elif isinstance(action, qc.actions.Observe):
            operators = action.observables
            outcomes += len(operators)
        elif isinstance(action, qc.actions.Clifford):
            operators = [text for pair in action.generators.items() for text in pair]
        elif isinstance(action, qc.actions.Pauli):
            operators = [action.operator]
        elif isinstance(action, qc.actions.Rotate):
            operators = [action.pauli]
        else:
            continue
        for operator in operators:
            if parameters.get(operator) == kind.PAULI:
                continue
            try:
                indices = _pauli_indices(operator)
            except ValueError as error:
                yield f"{label}: {error}"
                continue
            if capacity is None:
                continue
            unavailable = {index for index in indices if index >= capacity} - temporary
            if unavailable:
                if isinstance(action, qc.actions.Stabilize) and condition is None:
                    temporary.update(unavailable)
                else:
                    yield f"{label}: logical indices {sorted(unavailable)} are outside the operand range 0..{capacity} and have not been introduced by unconditional stabilize"


def _same_instruction_set(first: qc.InstructionSet, second: qc.InstructionSet) -> bool:
    return all(
        getattr(first, field) == getattr(second, field)
        for field in ("name", "description", "blocks", "instructions", "metadata")
    )


def _argument_issue(
    value: object,
    parameter: qc.instructions.Parameter,
    gadget: qc.Gadget,
    preceding: int,
) -> str | None:
    kind = qc.instructions.Parameter.Kind
    expected = parameter.kind
    if isinstance(value, str):
        try:
            term = reference_term(value)
        except ValueError:
            term = None
        if isinstance(term, Outcome):
            if expected != kind.BIT:
                return "a circuit readout can bind only a bit parameter"
            if term.index >= preceding:
                return f"{value} is out of bounds for {preceding} preceding readouts"
            return None
        for name, source_name in gadget.parameter_bindings.items():
            if value == source_name:
                source = next(
                    (
                        item
                        for item in gadget.implements.parameters
                        if item.name == name
                    ),
                    None,
                )
                if source is not None and (
                    source.kind == expected
                    or (source.kind == kind.INTEGER and expected == kind.NUMBER)
                ):
                    return None
                return f"forwarded parameter {name!r} has an incompatible type"
    if expected == kind.BIT:
        valid = type(value) is bool or (type(value) is int and value in (0, 1))
    elif expected in (kind.INTEGER, kind.NUMBER):
        valid = type(value) is int or (expected == kind.NUMBER and type(value) is float)
        valid |= isinstance(value, list) and all(type(item) is int for item in value)
    elif expected == kind.BOOLEAN:
        valid = type(value) is bool
    elif expected == kind.STRING:
        valid = isinstance(value, str) or (
            isinstance(value, list) and all(isinstance(item, str) for item in value)
        )
    else:
        valid = False
        if isinstance(value, str) or (
            isinstance(value, list) and all(isinstance(item, str) for item in value)
        ):
            try:
                _pauli_indices(value if isinstance(value, str) else " ".join(value))
                valid = True
            except ValueError:
                pass
    return None if valid else f"expected {expected.name.lower()}, supplied {value!r}"


def _gadget_issues(
    gadget: qc.Gadget, blocks: dict[str, int] | None = None
) -> Iterator[str]:
    try:
        declared = observe_count_of(gadget.implements) + len(gadget.implements.flags)
    except ValueError:
        declared = None
    target_blocks = {
        block.name: block.encodes for block in gadget.circuit.instruction_set.blocks
    }
    for side, encodings, operands in (
        ("in", gadget.inputs, gadget.implements.inputs),
        ("out", gadget.outputs, gadget.implements.outputs),
    ):
        boundary_types: dict[str, str] = {}
        boundary_support: dict[str, tuple[int, int]] = {}
        if len(encodings) != len(operands):
            yield f"{side}: {len(encodings)} encodings for {len(operands)} operands"
        for entry, (encoding, operand) in enumerate(zip(encodings, operands)):
            for position, label in enumerate(encoding.support):
                previous = boundary_support.get(label)
                if previous is not None:
                    previous_entry, previous_position = previous
                    yield f"{side}[{entry}].support[{position}] and {side}[{previous_entry}].support[{previous_position}] both use circuit label {label!r}"
                else:
                    boundary_support[label] = (entry, position)
            code = encoding.code
            if (
                blocks is not None
                and operand.block in blocks
                and code.logical_count != blocks[operand.block]
            ):
                yield f"{side}[{entry}]: code {code.name!r} declares {code.logical_count} logical qubits; block {operand.block!r} requires {blocks[operand.block]}"
            types = encoding.block_types
            if not types and len(target_blocks) == 1:
                types = [next(iter(target_blocks))] * len(encoding.support)
            if len(types) != len(encoding.support):
                yield f"{side}[{entry}]: support block types cannot be determined"
                continue
            if any(name not in target_blocks for name in types):
                yield f"{side}[{entry}]: undeclared circuit block type"
                continue
            for label, block_type in zip(encoding.support, types):
                if label in boundary_types and boundary_types[label] != block_type:
                    yield f"{side}[{entry}]: circuit label {label!r} has conflicting block types {boundary_types[label]!r} and {block_type!r}"
                boundary_types[label] = block_type
            capacity = sum(target_blocks[name] for name in types)
            for property_name, operators in (
                ("stabilizers", code.stabilizers),
                ("x", code.x),
                ("z", code.z),
            ):
                for index, operator in enumerate(operators):
                    try:
                        invalid = sorted(
                            value
                            for value in _pauli_indices(operator)
                            if value >= capacity
                        )
                    except ValueError as error:
                        yield f"{side}[{entry}].{property_name}[{index}]: {error}"
                        continue
                    if invalid:
                        yield f"{side}[{entry}].{property_name}[{index}]: code indices {invalid} are out of bounds for {capacity} support qubits"
    if declared is not None and len(gadget.readouts) > declared:
        yield f"readouts: {len(gadget.readouts)} entries supplied; expected {declared}"
    parameters = {parameter.name for parameter in gadget.implements.parameters}
    for name in gadget.parameter_bindings:
        if name not in parameters:
            yield f"parameter binding {name!r} is not declared by the implemented instruction"
    circuit_count = None
    try:
        calls = gadget.circuit.calls()
    except (ValueError, TypeError) as error:
        missing_parser = (
            f"No source parser registered for '.{gadget.circuit.effective_format}'"
        )
        if str(error) != missing_parser:
            yield f"circuit: {error}"
    else:
        circuit_count = 0
        for position, call in enumerate(calls):
            instruction = gadget.circuit.instruction_set.instructions[call.mnemonic]
            parameter_map = {
                parameter.name: parameter for parameter in instruction.parameters
            }
            for name, value in call.arguments.items():
                if name not in parameter_map:
                    yield f"circuit.calls[{position}]: argument {name!r} is not declared by {call.mnemonic!r}"
                else:
                    issue = _argument_issue(
                        value, parameter_map[name], gadget, circuit_count
                    )
                    if issue:
                        yield f"circuit.calls[{position}] argument {name!r}: {issue}"
            for pattern in call.select:
                for reference in pattern:
                    match = re.fullmatch(r"flags\[([0-9]+)\]", reference)
                    if (
                        match is not None and int(match[1]) >= len(instruction.flags)
                    ) or (match is None and reference not in instruction.flags):
                        yield f"circuit.calls[{position}]: select references unknown flag {reference!r}"
            try:
                circuit_count += observe_count_of(instruction) + len(instruction.flags)
            except ValueError as error:
                yield f"circuit.calls[{position}]: {error}"
                circuit_count = None
                break
    equations = [
        (f"checks[{index}]", equation) for index, equation in enumerate(gadget.checks)
    ]
    equations += [
        (f"readouts[{index}]", readout.equation)
        for index, readout in enumerate(gadget.readouts)
    ]
    for label, equation in equations:
        for reference in equation:
            if isinstance(reference, int):
                continue
            capacity = None
            terms = reference_terms(reference)
            first = next(terms)
            if isinstance(first, Outcome):
                capacity = circuit_count
            elif isinstance(first, ReadoutSign):
                capacity = len(gadget.readouts)
            else:
                encodings = gadget.inputs if first.side == "in" else gadget.outputs
                entry = first.entry
                if entry >= len(encodings):
                    yield f"{label}: {reference} is out of bounds for {len(encodings)} {first.side} encodings"
                    continue
                property_name = (
                    first.basis if isinstance(first, LogicalSign) else "stabilizers"
                )
                capacity = len(getattr(encodings[entry].code, property_name))
            if capacity is not None:
                invalid = next(
                    (
                        atom.index
                        for atom in chain((first,), terms)
                        if atom.index >= capacity
                    ),
                    None,
                )
                if invalid is not None:
                    yield f"{label}: {reference} index {invalid} is out of bounds for {capacity} entries"


def structural_issues(target: object) -> Iterator[tuple[str, str]]:
    if isinstance(target, qc.Code):
        if len(target.x) != len(target.z):
            yield "", f"logical operator counts disagree: x has {len(target.x)} entries, z has {len(target.z)}"
        for kind, operators in (
            ("stabilizers", target.stabilizers),
            ("x", target.x),
            ("z", target.z),
        ):
            for index, operator in enumerate(operators):
                try:
                    _pauli_indices(operator)
                except ValueError as error:
                    yield "", f"{kind}[{index}]: {error}"
    elif isinstance(target, qc.InstructionSet):
        blocks = {block.name: block.encodes for block in target.blocks}
        if len(blocks) != len(target.blocks):
            yield "", "duplicate block declaration names"
        for instruction in target.instructions.values():
            for issue in _instruction_issues(instruction, blocks):
                yield "", f"instruction {instruction.mnemonic!r}: {issue}"
    elif isinstance(target, qc.Gadget):
        from .._frames import FrameMap

        try:
            FrameMap(target)
        except (ValueError, TypeError, KeyError) as error:
            yield "frames", str(error)
        for issue in _instruction_issues(target.implements, None):
            yield "", issue
        for issue in _gadget_issues(target):
            yield "", issue
    elif isinstance(target, qc.Qodec):
        if len(target.layers) < 2:
            yield "", f"protocol requires at least two layers, got {len(target.layers)}"
        seen_sets: dict[str, qc.InstructionSet] = {}
        seen_codes: dict[str, qc.Code] = {}
        for index, layer in enumerate(target.layers):
            instruction_set = layer.instruction_set
            previous_set = seen_sets.get(instruction_set.name)
            if previous_set is not None and not _same_instruction_set(
                previous_set, instruction_set
            ):
                yield "", f"conflicting instruction sets named {instruction_set.name!r}"
            seen_sets[instruction_set.name] = instruction_set
            for _, issue in structural_issues(instruction_set):
                yield f"layers[{index}].instruction_set", issue
            blocks = {block.name: block.encodes for block in instruction_set.blocks}
            bindings = dict(layer.codes)
            if index + 1 < len(target.layers):
                for block in blocks:
                    if block not in bindings:
                        yield f"layers[{index}]", f"block {block!r} has no code binding in the layer"
            for mnemonic, gadget in layer.gadgets.items():
                path = f"layers[{index}].gadgets[{json.dumps(mnemonic, ensure_ascii=False)}]"
                if index + 1 == len(target.layers):
                    yield "", "the bottom layer has gadgets but no target layer"
                elif not _same_instruction_set(
                    gadget.circuit.instruction_set,
                    target.layers[index + 1].instruction_set,
                ):
                    yield path, "circuit instruction set differs from the next layer's instruction set"
                if mnemonic not in instruction_set.instructions:
                    yield path, f"instruction {mnemonic!r} is not declared by the layer"
                elif gadget.implements != instruction_set.instructions[mnemonic]:
                    yield path, "implements differs from the layer's instruction"
                for encoding in (*gadget.inputs, *gadget.outputs):
                    code = encoding.code
                    previous_code = seen_codes.get(code.name)
                    if previous_code is not None and previous_code != code:
                        yield "", f"conflicting codes named {code.name!r}"
                    elif previous_code is None:
                        for _, issue in structural_issues(code):
                            yield f"codes[{json.dumps(code.name, ensure_ascii=False)}]", issue
                    seen_codes[code.name] = code
                for encodings, operands in (
                    (gadget.inputs, gadget.implements.inputs),
                    (gadget.outputs, gadget.implements.outputs),
                ):
                    for encoding, operand in zip(encodings, operands):
                        code = encoding.code
                        if (
                            operand.block in bindings
                            and bindings[operand.block] != code
                        ):
                            yield path, f"block {operand.block!r} is bound to different codes in the layer"
                        bindings.setdefault(operand.block, code)
                for issue in _gadget_issues(gadget, blocks):
                    yield path, issue
