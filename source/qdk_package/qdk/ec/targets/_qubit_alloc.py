"""Per-program physical-qubit allocation for ``StimSampler``.

Maps each *block* mentioned by a lowered program to disjoint physical
qubit ranges, so that a gadget's stim source — whose qubit indices are
local to the gadget — can be safely concatenated into one combined
circuit without colliding with neighbouring gadgets.

Used exclusively by :mod:`qdk.ec.targets.stim`. Public API is the
single function :func:`remap_call_source`.
"""

from __future__ import annotations

from collections.abc import Iterable

import stim

import qodec


def _channel_qubit_table(
    channel: qodec.Channel,
) -> dict[int, list[tuple[str, int]]]:
    """Map each source qubit index → the list of ``(operand_name,
    position)`` identities it carries across ``channel``'s encodings.

    Each ``Encoding`` lists the literal source-qubit labels that belong
    to its operand; the label's index within ``support`` gives the
    operand-local position. Source qubits not appearing in any encoding
    are gadget-internal ancillas and are absent from the returned map.

    A single source qubit may carry more than one identity: a gadget
    that merges two operands into one block (lattice-surgery merge) or
    splits a block back into separate operands binds the same physical
    wire to both an ``encoding_in`` identity and an ``encoding_out``
    identity. Those identities are aliases of one physical wire, and the
    allocator unifies them; the conflict is the linkage, not an error.
    """
    table: dict[int, list[tuple[str, int]]] = {}
    for encoding_list in (channel.encoding_in, channel.encoding_out):
        for encoding in encoding_list:
            name = encoding.operand
            for position, label in enumerate(encoding.support):
                try:
                    source_qubit = int(label)
                except ValueError as exc:
                    raise ValueError(
                        f"channel encoding for operand {name!r} has a "
                        f"non-integer support label {label!r}; stim sources "
                        "are indexed by integer qubit identifiers"
                    ) from exc
                identity = (name, position)
                identities = table.setdefault(source_qubit, [])
                if identity not in identities:
                    identities.append(identity)
    return table


class PhysicalQubitAllocator:
    """Assigns a stable global physical qubit index to each qubit
    referenced by a lowered program.

    Two distinct allocation modes:

    * **Block-bound** qubits — those reachable through a channel's
      ``encoding_in``/``encoding_out`` — are keyed by
      ``(block_name, position_within_block)``. Identical keys re-use
      the same physical index across calls, so a "qubit 0 of block X"
      that appears in call N and call M lands on the same physical
      wire (in-place semantics).

    * **Ancilla** qubits — source qubits internal to a gadget, with no
      operand binding — get a fresh physical index per call. They are
      never reused across calls.

    The block and ancilla pools share one global numbering space, so
    every returned index is unique within the combined circuit.

    Block-bound keys are held in a union-find structure so that
    lattice-surgery merges and splits can be represented. When a single
    physical wire carries two block identities at once — e.g. operand
    ``a`` position 0 merging into block ``blk`` position 0 —
    :meth:`unify` joins the two keys into one equivalence class that
    shares a single physical wire. A merged block may therefore occupy
    non-contiguous wires inherited from the operands it was built from.
    """

    def __init__(self) -> None:
        self._parent: dict[tuple[str, int], tuple[str, int]] = {}
        self._wire: dict[tuple[str, int], int] = {}
        self._next: int = 0

    def _find(self, key: tuple[str, int]) -> tuple[str, int]:
        if key not in self._parent:
            self._parent[key] = key
        root = key
        while self._parent[root] != root:
            root = self._parent[root]
        while self._parent[key] != root:
            self._parent[key], key = root, self._parent[key]
        return root

    def _wire_of(self, root: tuple[str, int]) -> int:
        wire = self._wire.get(root)
        if wire is None:
            wire = self._next
            self._wire[root] = wire
            self._next += 1
        return wire

    def get_block_qubit(self, block: str, position: int) -> int:
        return self._wire_of(self._find((block, position)))

    def unify(self, first: tuple[str, int], second: tuple[str, int]) -> int:
        root_a = self._find(first)
        root_b = self._find(second)
        if root_a == root_b:
            return self._wire_of(root_a)
        wire_a = self._wire.get(root_a)
        wire_b = self._wire.get(root_b)
        if wire_a is not None and wire_b is not None and wire_a != wire_b:
            raise ValueError(
                f"cannot unify block qubits {first} and {second}: both are "
                f"already bound to distinct physical wires {wire_a} and "
                f"{wire_b}"
            )
        if wire_b is not None:
            self._parent[root_a] = root_b
            return wire_b
        self._parent[root_b] = root_a
        return self._wire_of(root_a)

    def alloc_ancilla(self) -> int:
        new_index = self._next
        self._next += 1
        return new_index

    def __len__(self) -> int:
        return self._next


def _resolve_block_name(operand_binding: object) -> str:
    """Return the block name from an ``InstructionCall`` operand binding.

    Bindings are typically plain strings; the integer-binding form
    (e.g. ``Qubit(usize)`` returning ``int``) is treated as a single
    block name via ``str()``.
    """
    if isinstance(operand_binding, str):
        return operand_binding
    return str(operand_binding)


def remap_call_source(
    source_circuit: stim.Circuit,
    channel: qodec.Channel,
    call: qodec.instructions.InstructionCall,
    allocator: PhysicalQubitAllocator,
) -> stim.Circuit:
    """Return a copy of ``source_circuit`` with every qubit target
    rewritten via ``allocator`` so that the resulting circuit can be
    concatenated into a global combined circuit alongside other calls.

    Source qubits reachable through the channel's encodings are
    rewritten to block-bound physical indices (stable across calls).
    Any other source qubits are treated as gadget-internal ancillas
    and given fresh per-call physical indices.

    Non-qubit targets (measurement-record references, sweep-bits,
    ``rec[…]``) are passed through unchanged.
    """
    layout = _channel_qubit_table(channel)

    # Encodings are positional: the i-th input encoding carries operand name
    # ``str(i)`` (see ``_channel_qubit_table``), so bind it to the i-th value
    # the call supplies in ``inputs`` (then ``outputs``), matching by position.
    bindings: dict[str, object] = {}
    for entry, value in enumerate(call.inputs.values()):
        bindings[str(entry)] = value
    for entry, value in enumerate(call.outputs.values()):
        bindings.setdefault(str(entry), value)

    ancilla_map: dict[int, int] = {}

    def remap(source_qubit: int) -> int:
        identities = layout.get(source_qubit)
        if identities:
            keys = [
                (_resolve_block_name(bindings[operand_name]), position)
                for operand_name, position in identities
            ]
            first = keys[0]
            for other in keys[1:]:
                allocator.unify(first, other)
            return allocator.get_block_qubit(*first)
        cached = ancilla_map.get(source_qubit)
        if cached is None:
            cached = allocator.alloc_ancilla()
            ancilla_map[source_qubit] = cached
        return cached

    def rewrite(circuit: stim.Circuit) -> stim.Circuit:
        out = stim.Circuit()
        for instruction in circuit:
            if isinstance(instruction, stim.CircuitRepeatBlock):
                out.append(
                    stim.CircuitRepeatBlock(
                        instruction.repeat_count,
                        rewrite(instruction.body_copy()),
                    )
                )
                continue
            assert isinstance(instruction, stim.CircuitInstruction)
            new_targets: list[stim.GateTarget] = []
            for target in instruction.targets_copy():
                if target.is_qubit_target:
                    new_targets.append(stim.GateTarget(remap(target.qubit_value)))
                else:
                    new_targets.append(target)
            out.append(
                stim.CircuitInstruction(
                    instruction.name,
                    new_targets,
                    instruction.gate_args_copy(),
                )
            )
        return out

    return rewrite(source_circuit)


__all__ = [
    "PhysicalQubitAllocator",
    "remap_call_source",
]
