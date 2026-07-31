"""Recursive lowering compiler.

Walks the codec's translation chain from top (logical) to bottom
(physical), substituting each source-layer instruction with the
gadget that realizes it on the next layer down. After all translations
have been applied, the resulting program is in the codec's bottom-layer
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

import qodec

from ..._typed_ir import value_to_string as _value_to_string
from ..._typed_ir import value_tokens as _value_tokens

from ..._qodec_compat import realization
from qodec.circuits import Program

from .compiler import CompileResult


class RecursiveLowering:
    """Lower a Program through gadget substitution across all layers.

    The compiler's "source" is ``codec.layers[0].isa``; its "target" is
    ``codec.layers[-1].isa``. To compile only part of a larger codec's
    chain, slice it with ``Qodec.slice(top, bottom + 1)`` first and pass
    the sub-codec to this compiler.

    Block qubit references in gadget bodies are rewritten to namespaced
    labels of the form ``"<block_name>.<index>"``. To get integer or
    other concrete qubit labels, chain with a relocation compiler.
    """

    def __init__(self, codec: qodec.Qodec) -> None:
        self._codec = codec

    @property
    def codec(self) -> qodec.Qodec:
        return self._codec

    def compile(self, program: Program) -> CompileResult:
        if not self._codec.layers:
            raise ValueError("RecursiveLowering: codec has no layers")
        top_isa = self._codec.layers[0].isa
        if program.isa.name != top_isa.name:
            raise ValueError(
                f"program ISA {program.isa.name!r} does not match codec's "
                f"top layer {top_isa.name!r}"
            )

        current_program = program
        # Each non-bottom layer carries the gadgets that lower it to the
        # layer below; the bottom layer has no gadgets.
        for layer_index, layer in enumerate(self._codec.layers[:-1]):
            target_isa = self._codec.layers[layer_index + 1].isa
            current_program = _apply_translation(current_program, layer, target_isa)

        return CompileResult(program=current_program)


def _apply_translation(
    program: Program,
    layer: qodec.Layer,
    target_isa: qodec.InstructionSet,
) -> Program:
    """Substitute each call with its gadget's namespaced target instructions."""
    lowered: list[qodec.instructions.InstructionCall] = []
    gadgets = layer.gadgets

    for call in program.instructions:
        if call.mnemonic not in gadgets:
            raise KeyError(
                f"no gadget for instruction {call.mnemonic!r} in lowering "
                f"to {target_isa.name!r}"
            )
        gadget = gadgets[call.mnemonic]
        remap = _build_namespaced_remap(gadget, call, call.mnemonic)
        for body_call in realization(gadget).instructions:
            lowered.append(_remap_call(body_call, remap))
    return Program(lowered, target_isa)


def _build_namespaced_remap(
    gadget: qodec.Gadget,
    call: qodec.instructions.InstructionCall,
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
    channel = realization(gadget)
    pairs = list(zip(channel.encoding_in, call.inputs.values())) + list(
        zip(channel.encoding_out, call.outputs.values())
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
        for body_call in channel.instructions:
            operand_values = (
                *body_call.inputs.values(),
                *body_call.outputs.values(),
            )
            for value in operand_values:
                for token in _value_tokens(value):
                    try:
                        internal_qubit = int(token)
                    except ValueError:
                        continue
                    if internal_qubit not in remap:
                        remap[internal_qubit] = f"{instance_prefix}#{internal_qubit}"
    return remap


def _remap_call(
    call: qodec.instructions.InstructionCall,
    remap: dict[int, str],
) -> qodec.instructions.InstructionCall:
    """Return a copy of ``call`` with every qubit operand remapped."""
    if not remap:
        return call
    new_inputs = {
        name: _remap_qubits(value, remap) for name, value in call.inputs.items()
    }
    new_outputs = {
        name: _remap_qubits(value, remap) for name, value in call.outputs.items()
    }
    return qodec.instructions.InstructionCall(
        call.mnemonic,
        inputs=new_inputs,
        outputs=new_outputs,
        parameters=call.parameters,
    )


def _remap_qubits(value: object, remap: dict[int, str]) -> str:
    """Remap each whitespace-separated qubit-index token in ``value``.

    Tokens that don't parse as integers (e.g., classical bit names) are
    passed through unchanged. Integer tokens missing from ``remap`` also
    pass through unchanged (these are the gadget's ancilla / scratch
    qubits, which keep their authored integer indices).
    """
    tokens = _value_tokens(value)
    if not tokens:
        return _value_to_string(value)
    out_tokens: list[str] = []
    for token in tokens:
        try:
            qubit = int(token)
        except ValueError:
            out_tokens.append(token)
            continue
        if qubit in remap:
            out_tokens.append(remap[qubit])
        else:
            out_tokens.append(token)
    return " ".join(out_tokens)
