"""Recursive lowering compiler.

Walks the qodec's translation chain from top (logical) to bottom
(physical), substituting each source-layer instruction with the
gadget that realizes it on the next layer down. After all translations
have been applied, the resulting program is in the qodec's bottom-layer
ISA.

Block qubits in gadget bodies are *namespaced*: the i-th qubit of a
block named ``"alice"`` is rewritten to the label ``"alice.i"`` (with
the implicit block name ``""`` producing ``".0"``, ``".1"``, ...).
This lets multi-block programs lower without collisions and without
the compiler needing to know any physical-qubit layout.

To produce a program with concrete integer (or otherwise non-namespaced)
qubit labels, follow `RecursiveLowering` with a relocation compiler
such as `Relocate` or `AutoRelocate`.

Qubits in gadget bodies that are not part of any encoding's ``support``
(typically ancillas) pass through unchanged with their authored
integer indices.
"""

from __future__ import annotations

import qodec as qc

from ..._operands import QubitLabel, map_call_labels, qubit_labels

from qodec.circuits import Program

from .compiler import CompileResult


class RecursiveLowering:
    """Lower a Program through gadget substitution across all layers.

    The compiler's "source" is ``qodec.layers[0].isa``; its "target" is
    ``qodec.layers[-1].isa``. To compile only part of a larger qodec's
    chain, slice it with ``Qodec.slice(top, bottom + 1)`` first and pass
    the sub-qodec to this compiler.

    Block qubit references in gadget bodies are rewritten to namespaced
    labels of the form ``"<block_name>.<index>"``. To get integer or
    other concrete qubit labels, chain with a relocation compiler.
    """

    def __init__(self, qodec: qc.Qodec) -> None:
        self._qodec = qodec

    @property
    def qodec(self) -> qc.Qodec:
        return self._qodec

    def compile(self, program: Program) -> CompileResult:
        if not self._qodec.layers:
            raise ValueError("RecursiveLowering: qodec has no layers")
        top_isa = self._qodec.layers[0].isa
        if program.isa.name != top_isa.name:
            raise ValueError(
                f"program ISA {program.isa.name!r} does not match qodec's "
                f"top layer {top_isa.name!r}"
            )

        current_program = program
        # Each non-bottom layer carries the gadgets that lower it to the
        # layer below; the bottom layer has no gadgets.
        for layer_index, layer in enumerate(self._qodec.layers[:-1]):
            target_isa = self._qodec.layers[layer_index + 1].isa
            current_program = _apply_translation(current_program, layer, target_isa)

        return CompileResult(program=current_program)


def _apply_translation(
    program: Program,
    layer: qc.Layer,
    target_isa: qc.InstructionSet,
) -> Program:
    """Substitute each call with its gadget's namespaced target instructions."""
    lowered: list[qc.instructions.InstructionCall] = []
    gadgets = layer.gadgets

    for call in program.instructions:
        if call.mnemonic not in gadgets:
            raise KeyError(
                f"no gadget for instruction {call.mnemonic!r} in lowering "
                f"to {target_isa.name!r}"
            )
        gadget = gadgets[call.mnemonic]
        remap = build_namespaced_remap(gadget, call, call.mnemonic)
        for body_call in gadget.circuit.instructions:
            lowered.append(remap_call(body_call, remap))
    return Program(lowered, target_isa)


def build_namespaced_remap(
    gadget: qc.Gadget,
    call: qc.instructions.InstructionCall,
    mnemonic: str,
    namespace_internal_blocks: bool = False,
) -> dict[int, str]:
    """Build ``{gadget_body_qubit -> "<block_name>.<index>"}`` for one call.

    For each input/output encoding of the gadget (positional, aligned with
    the call's ``inputs`` / ``outputs`` operand values in order), rewrite each
    ``Encoding.support[i]`` to ``"<block_label>.<i>"`` where ``block_label`` is
    the value the call binds to that operand.

    Input and output encodings of the same operand must produce a consistent
    remap; otherwise raises.

    Body qubits that are *not* part of any encoding but are referenced as
    block operands by more than one body call (transient blocks created by
    one body instruction and consumed by another) are namespaced with a
    per-call-instance prefix when ``namespace_internal_blocks`` is set.
    """
    remap: dict[int, str] = {}
    pairs = list(zip(gadget.inputs, call.inputs.values())) + list(
        zip(gadget.outputs, call.outputs.values())
    )
    for encoding, block_value in pairs:
        block_name = str(block_value)
        for i, support_qubit in enumerate(encoding.support):
            body_qubit = int(support_qubit)
            label = f"{block_name}.{i}"
            if body_qubit in remap and remap[body_qubit] != label:
                raise ValueError(
                    f"gadget {mnemonic!r}: inconsistent placement for body "
                    f"qubit {body_qubit} ({remap[body_qubit]!r} vs {label!r})"
                )
            remap[body_qubit] = label

    block_values = [*call.inputs.values(), *call.outputs.values()]
    if namespace_internal_blocks and block_values:
        instance_prefix = (
            mnemonic + ":" + "+".join(sorted({str(value) for value in block_values}))
        )
        for body_call in gadget.circuit.instructions:
            operand_values = (
                *body_call.inputs.values(),
                *body_call.outputs.values(),
            )
            for value in operand_values:
                for label in qubit_labels(value):
                    if isinstance(label, int) and label not in remap:
                        remap[label] = f"{instance_prefix}#{label}"
    return remap


def remap_call(
    call: qc.instructions.InstructionCall,
    remap: dict[int, str],
) -> qc.instructions.InstructionCall:
    """Return a copy of ``call`` with every authored qubit index placed.

    Labels absent from ``remap`` pass through: symbolic labels are already
    placed, and authored indices with no encoding entry are the gadget's
    ancillas, which keep their own numbering.
    """
    if not remap:
        return call

    def placed(label: QubitLabel) -> QubitLabel:
        return remap.get(label, label) if isinstance(label, int) else label

    return map_call_labels(call, placed)
