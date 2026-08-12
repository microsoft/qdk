"""Run a QIR program through a qodec: the error-corrected execution path.

``qdk.simulation.run_qir`` simulates a QIR program on *physical* qubits, with
optional noise. This module answers the next question: what if those qubits were
*encoded*?

:func:`run_qir_encoded` takes the same QIR a physical simulator would run, maps
each of its gates onto the corresponding logical instruction of a qodec, samples
the resulting encoded circuit, and decodes the logical measurement outcomes back
into the ``Result`` values the caller expects. The program is unchanged; only the
substrate it runs on differs.

What the caller gets back
-------------------------
:func:`run_qir_encoded` returns the same shape as ``run_qir``: one list of
``Result`` values per shot. What changes is that each value is a *logical*
measurement, reconstructed from the encoded block's physical readouts, and that
shots the code detected as corrupted can be dropped (see ``postselect``).

The qodec must express the program
-----------------------------------
A qodec supplies a *finite* logical instruction set — the operations for which
its author supplied fault-tolerant gadgets. A QIR program using a gate the qodec
does not implement cannot be encoded, and this module raises rather than
silently substituting an unprotected operation. :func:`encodable_gates_of`
reports what a given qodec can express.

The mapping from QIR gates to logical mnemonics is by *action*, not by name: a
qodec instruction is a candidate for QIR's ``X`` on logical qubit ``k`` when its
declared action is exactly the Pauli ``X`` on that qubit. So a qodec is not
required to use any particular naming convention.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass, field
from typing import Any, Optional

import qodec

from .._qodec_compat import observables_as_xor_map

#: Single-qubit Pauli gates, as ``(qir mnemonic, qodec action basis)``.
_PAULI_GATES = {"X": "X", "Y": "Y", "Z": "Z"}

#: Measurement gates that consume a qubit and record one bit.
_MEASURE_GATES = frozenset({"M", "MZ", "MResetZ"})


@dataclass(frozen=True)
class LogicalSlot:
    """Where a QIR qubit lives inside the qodec's encoded blocks."""

    block: int
    index: int


@dataclass
class EncodedProgram:
    """A QIR program rewritten as a qodec logical program.

    ``program`` is what the sampler runs. ``result_slots`` records, in QIR
    result order, which logical slot each recorded measurement came from, so the
    raw physical readouts can be decoded back into per-result values.
    """

    program: Any
    slots: dict[int, LogicalSlot]
    result_slots: list[LogicalSlot] = field(default_factory=list)
    measurement_gadgets: list[str] = field(default_factory=list)


def _action_signature(instruction: qodec.Instruction) -> Optional[tuple]:
    """A comparable summary of what a qodec instruction does.

    Returns ``("pauli", basis, index)`` for a single-qubit Pauli,
    ``("observe", (basis, ...))`` for a measurement, ``("stabilize", (...))``
    for a preparation, ``("idle",)`` for a no-op, or ``None`` for anything this
    module does not know how to match against a QIR gate.
    """
    actions = list(instruction.action)
    if not actions:
        return ("idle",)
    if len(actions) != 1:
        return None
    action = actions[0]

    if isinstance(action, qodec.actions.Pauli):
        token = str(action.operator).strip()
        basis, _, index = token.partition("_")
        if basis in _PAULI_GATES and index.isdigit():
            return ("pauli", basis, int(index))
        return None

    if isinstance(action, qodec.actions.Observe):
        bases = []
        for observable in action.observables:
            token = str(getattr(observable, "pauli", observable)).strip()
            basis, _, index = token.partition("_")
            if not index.isdigit():
                return None
            bases.append((basis, int(index)))
        return ("observe", tuple(bases))

    if isinstance(action, qodec.actions.Stabilize):
        bases = []
        for operator in action.operators:
            token = str(operator).strip()
            basis, _, index = token.partition("_")
            if not index.isdigit():
                return None
            bases.append((basis, int(index)))
        return ("stabilize", tuple(bases))

    return None


def _index_isa(isa: qodec.InstructionSet) -> dict[tuple, str]:
    """Map each recognisable action signature to its instruction mnemonic."""
    index: dict[tuple, str] = {}
    for mnemonic, instruction in isa.instructions.items():
        signature = _action_signature(instruction)
        if signature is not None:
            index.setdefault(signature, mnemonic)
    return index


def _logical_capacity(isa: qodec.InstructionSet) -> int:
    """How many logical qubits one encoded block of this ISA holds."""
    blocks = list(isa.blocks)
    if not blocks:
        raise ValueError("qodec's logical ISA declares no blocks")
    return blocks[0].encodes


def encodable_gates_of(codec: qodec.Qodec) -> set[str]:
    """The QIR gate mnemonics ``codec`` can express.

    Reports what :func:`run_qir_encoded` will accept for this qodec, derived
    from the declared action of each of its logical instructions. Useful for
    telling a user *why* their program cannot be encoded before they run it.
    """
    index = _index_isa(codec.layers[0].isa)
    gates = set()
    for signature in index:
        if signature[0] == "pauli":
            gates.add(signature[1])
        elif signature[0] == "observe":
            bases = {basis for basis, _ in signature[1]}
            if bases == {"Z"}:
                gates.update(_MEASURE_GATES)
        elif signature[0] == "idle":
            gates.add("I")
    return gates


def _call(
    isa: qodec.InstructionSet, mnemonic: str, block: str = "q"
) -> "qodec.instructions.InstructionCall":
    """An ``InstructionCall`` binding every operand of ``mnemonic`` to ``block``."""
    instruction = isa.instruction(mnemonic)
    inputs = {str(i): block for i in range(len(list(instruction.inputs)))}
    outputs = {str(i): block for i in range(len(list(instruction.outputs)))}
    if not inputs and not outputs:
        return qodec.instructions.InstructionCall(mnemonic)
    return qodec.instructions.InstructionCall(mnemonic, inputs=inputs, outputs=outputs)


def _gate_name(gate: object) -> str:
    """The bare mnemonic of a QIR instruction id (``QirInstructionId.X`` -> ``X``)."""
    return str(gate).rsplit(".", maxsplit=1)[-1]


def _extract_gates(module: Any) -> tuple[list[Any], int]:
    """Flatten a QIR module into ``(gate list, qubit count)``.

    Wraps the simulator's own :class:`AggregateGatesPass`, extended to follow
    calls into locally-defined wrapper functions. The Q# compiler emits those
    for the Adaptive profile (``call void @X(%Qubit* %q)`` around
    ``__quantum__qis__x__body``), and the base pass rejects anything that is not
    a known intrinsic — so without this, the same program would encode under one
    target profile and fail under another.

    Two details make this correct rather than merely working:

    * Only the entry point is walked. The visitor would otherwise also visit
      each wrapper as a top-level function and emit its gates a second time.
    * The caller's arguments are substituted for the wrapper's parameters by
      positional index, so the qubit a gate acts on survives the indirection.
    """
    import pyqir

    from ...simulation._simulation import AggregateGatesPass

    class _InliningPass(AggregateGatesPass):
        def __init__(self) -> None:
            super().__init__()
            self._bindings: list[list[Any]] = []

        def _resolve(self, call: Any) -> Any:
            """``call`` with wrapper parameters replaced by caller arguments."""
            if not self._bindings:
                return call
            binding = self._bindings[-1]
            resolved = []
            changed = False
            for arg in call.args:
                index = _parameter_index(arg)
                if index is not None and index < len(binding):
                    resolved.append(binding[index])
                    changed = True
                else:
                    resolved.append(arg)
            return _SubstitutedCall(call, resolved) if changed else call

        def _on_call_instr(self, call: Any) -> None:
            callee = call.callee
            blocks = list(getattr(callee, "basic_blocks", []))
            if not callee.name.startswith("__quantum__") and blocks:
                resolved = self._resolve(call)
                self._bindings.append(list(resolved.args))
                try:
                    for block in blocks:
                        for instruction in block.instructions:
                            if isinstance(instruction, pyqir.Call):
                                self._on_call_instr(instruction)
                finally:
                    self._bindings.pop()
                return
            super()._on_call_instr(self._resolve(call))

        def run(self, qir: Any) -> None:
            errors = qir.verify()
            if errors is not None:
                raise ValueError(f"Module verification failed: {errors}")
            entry = next(filter(pyqir.is_entry_point, qir.functions))
            self.required_num_qubits = pyqir.required_num_qubits(entry)
            self.required_num_results = pyqir.required_num_results(entry)
            # Walk only the entry point; wrappers are reached through their
            # call sites, so visiting them again would duplicate every gate.
            self._on_function(entry)

    pass_ = _InliningPass()
    gates, qubit_count, _ = pass_.run_and_collect(module)
    return list(gates), qubit_count


def _parameter_index(value: Any) -> Optional[int]:
    """Positional index of ``value`` if it is a function parameter, else ``None``.

    ``pyqir`` names unnamed parameters ``var_<n>`` in textual order, which is
    the only handle available for matching a wrapper's parameter to the
    caller's argument.
    """
    name = getattr(value, "name", None)
    if isinstance(name, str) and name.startswith("var_") and name[4:].isdigit():
        return int(name[4:])
    return None


class _SubstitutedCall:
    """A ``Call`` view whose ``args`` are the caller's, not the wrapper's.

    ``pyqir`` call instructions are read-only, so inlining a wrapper needs a
    lightweight stand-in that presents substituted arguments while delegating
    everything else (notably ``callee``) to the original.
    """

    def __init__(self, call: Any, args: list[Any]) -> None:
        self._call = call
        self.args = args

    def __getattr__(self, name: str) -> Any:
        return getattr(self._call, name)


def encode_qir(
    gates: Sequence[Sequence[Any]],
    codec: qodec.Qodec,
    *,
    qubit_count: int,
) -> EncodedProgram:
    """Rewrite an extracted QIR gate list as a qodec logical program.

    ``gates`` is the ``(instruction id, *operands)`` sequence the simulator's
    own front end produces. Every QIR qubit is assigned a logical slot in an
    encoded block, the program is opened with the qodec's Z-basis preparation,
    and each gate is translated to the logical instruction whose declared action
    matches it.

    Raises :class:`NotImplementedError` naming the offending gate when the qodec
    has no instruction for it — encoding must never silently downgrade an
    operation to an unprotected one.
    """
    from qodec.circuits import Program

    isa = codec.layers[0].isa
    index = _index_isa(isa)
    per_block = _logical_capacity(isa)

    slots = {
        qubit: LogicalSlot(block=qubit // per_block, index=qubit % per_block)
        for qubit in range(qubit_count)
    }
    blocks_needed = (qubit_count + per_block - 1) // per_block
    if blocks_needed > 1:
        raise NotImplementedError(
            f"program needs {qubit_count} qubits but one {isa.name!r} block "
            f"encodes {per_block}; multi-block encoding is not supported yet"
        )

    prepare = index.get(
        ("stabilize", tuple(("Z", i) for i in range(per_block)))
    )
    if prepare is None:
        raise NotImplementedError(
            f"qodec {codec.name!r} has no Z-basis preparation instruction, so a "
            "QIR program (which starts from |0>) cannot be encoded"
        )

    calls = [_call(isa, prepare)]
    result_slots: list[LogicalSlot] = []
    measurement_gadgets: list[str] = []

    for gate in gates:
        name = _gate_name(gate[0])

        if name in ("ResultRecordOutput", "ArrayRecordOutput", "TupleRecordOutput"):
            continue

        if name == "I":
            idle = index.get(("idle",))
            if idle is None:
                continue
            calls.append(_call(isa, idle))
            continue

        if name in _PAULI_GATES:
            qubit = int(gate[1])
            slot = slots[qubit]
            mnemonic = index.get(("pauli", _PAULI_GATES[name], slot.index))
            if mnemonic is None:
                raise NotImplementedError(
                    f"qodec {codec.name!r} has no instruction applying logical "
                    f"{name} to logical qubit {slot.index}"
                )
            calls.append(_call(isa, mnemonic))
            continue

        if name in _MEASURE_GATES:
            qubit = int(gate[1])
            slot = slots[qubit]
            mnemonic = index.get(
                ("observe", tuple(("Z", i) for i in range(per_block)))
            )
            if mnemonic is None:
                raise NotImplementedError(
                    f"qodec {codec.name!r} has no Z-basis logical measurement"
                )
            calls.append(_call(isa, mnemonic))
            result_slots.append(slot)
            measurement_gadgets.append(mnemonic)
            continue

        raise NotImplementedError(
            f"qodec {codec.name!r} cannot encode QIR gate {name!r}; it can "
            f"express {sorted(encodable_gates_of(codec))}"
        )

    return EncodedProgram(
        program=Program(calls, isa),
        slots=slots,
        result_slots=result_slots,
        measurement_gadgets=measurement_gadgets,
    )


def _decode_logical(
    codec: qodec.Qodec,
    encoded: EncodedProgram,
    readouts: "Any",
) -> "Any":
    """Recover per-result logical bits from raw physical measurement records.

    Each measurement gadget contributes a block of physical records at the end
    of the shot; the gadget's own readout bindings say which XOR of those
    records carries each logical qubit's value.
    """
    import numpy as np

    gadgets = codec.layers[0].gadgets
    values = np.zeros((readouts.shape[0], len(encoded.result_slots)), dtype=bool)

    # Measurement gadgets appear in program order; walk the record stream from
    # the end so each gadget's block is located without re-deriving widths.
    offsets: list[tuple[int, int]] = []
    cursor = readouts.shape[1]
    for mnemonic in reversed(encoded.measurement_gadgets):
        width = _measurement_width(gadgets[mnemonic])
        offsets.append((cursor - width, width))
        cursor -= width
    offsets.reverse()

    for position, (slot, mnemonic) in enumerate(
        zip(encoded.result_slots, encoded.measurement_gadgets)
    ):
        start, width = offsets[position]
        block = readouts[:, start : start + width]
        pattern = observables_as_xor_map(gadgets[mnemonic]).get(str(slot.index))
        if not pattern:
            raise ValueError(
                f"gadget {mnemonic!r} binds no readout for logical qubit "
                f"{slot.index}; the qodec cannot report that measurement"
            )
        bits = np.zeros(readouts.shape[0], dtype=bool)
        for record in pattern:
            bits = bits ^ block[:, record]
        values[:, position] = bits
    return values


def _measurement_width(gadget: qodec.Gadget) -> int:
    """Number of physical measurement records one gadget's circuit produces."""
    width = 0
    for line in gadget.circuit.source.splitlines():
        parts = line.split()
        if parts and parts[0] in ("M", "MZ", "MX", "MY", "MR", "MRZ", "MRX", "MRY"):
            width += len(parts) - 1
    return width


#: Gate-noise keys the stim emitter understands.
_STIM_DATA_KEY = "p_data"
_STIM_MEAS_KEY = "p_meas"


def stim_noise_from(noise: Any) -> Optional[dict[str, float]]:
    """Translate a QDK ``NoiseConfig`` into the stim emitter's noise model.

    The physical simulator is configured per QIR intrinsic
    (``noise.x.x = 0.01``); the encoded path runs a stim circuit whose gates are
    the qodec's, not the program's, so per-intrinsic rates cannot carry over
    literally. The two knobs the emitter exposes are the data-gate and
    measurement error rates, so this takes the *strongest* single-qubit gate
    error as ``p_data`` and the measurement error as ``p_meas`` — the reading
    that preserves "how noisy is this machine" across the two substrates.

    A mapping is returned unchanged (already in stim's vocabulary), and ``None``
    passes through as noiseless.
    """
    if noise is None:
        return None
    if isinstance(noise, Mapping):
        return dict(noise)

    def total(table: Any) -> float:
        return sum(
            float(getattr(table, axis, 0.0) or 0.0) for axis in ("x", "y", "z")
        )

    gate_tables = [
        getattr(noise, name, None)
        for name in ("x", "y", "z", "h", "s", "cx", "cy", "cz")
    ]
    p_data = max((total(t) for t in gate_tables if t is not None), default=0.0)
    measure_tables = [getattr(noise, name, None) for name in ("mz", "mresetz")]
    p_meas = max((total(t) for t in measure_tables if t is not None), default=0.0)

    if p_data == 0.0 and p_meas == 0.0:
        return None
    return {_STIM_DATA_KEY: p_data, _STIM_MEAS_KEY: p_meas}


def run_qir_encoded(
    input: Any,
    codec: qodec.Qodec,
    *,
    shots: int = 1,
    noise: Any = None,
    seed: Optional[int] = None,
    postselect: bool = True,
) -> list[Any]:
    """Simulate a QIR program with its qubits encoded in ``codec``.

    Returns results in the same shape ``qdk.simulation.run_qir`` returns for the
    same program — but every value is a *logical* measurement decoded from an
    encoded block rather than a physical qubit readout.

    Parameters
    ----------
    input:
        QIR source, as accepted by ``qdk.simulation.run_qir``.
    codec:
        The qodec to encode into. Must express every gate the program uses; see
        :func:`encodable_gates_of`.
    shots:
        Number of shots to sample.
    noise:
        Either a QDK :class:`~qdk.simulation.NoiseConfig` — the same object the
        physical simulator takes, translated by :func:`stim_noise_from` — or a
        stim gate-noise mapping such as ``{"p_data": 0.01, "p_meas": 0.01}``.
        ``None`` runs noiseless.
    seed:
        Seed forwarded to QIR preprocessing. The stim sampler draws its own
        randomness, so runs are not bit-for-bit reproducible from this alone.
    postselect:
        When ``True`` (the default), shots in which the code detected an error
        are dropped, and fewer than ``shots`` results may be returned. This is
        what an error-*detecting* code such as [[4,2,2]] buys you. Set to
        ``False`` to keep every shot.

    Raises
    ------
    NotImplementedError
        If the program uses a gate ``codec`` cannot express.
    """
    import numpy as np

    from ...simulation._simulation import (
        OutputRecordingPass,
        preprocess_simulation_input,
    )
    from .stim import StimSampler

    module, shots, _, seed = preprocess_simulation_input(input, shots, None, seed)
    gates, qubit_count = _extract_gates(module)

    encoded = encode_qir(gates, codec, qubit_count=qubit_count)

    sampler = StimSampler(codec, noise=stim_noise_from(noise))
    readouts = np.asarray(sampler.execute(encoded.program, shots=shots), dtype=bool)

    values = _decode_logical(codec, encoded, readouts)

    keep = np.ones(readouts.shape[0], dtype=bool)
    if postselect:
        events = sampler.emitter.detection_events(encoded.program, readouts)
        if events.size:
            keep = ~events.any(axis=1)

    from ..._native import Result

    # Shape each shot the way the physical simulator would, so an encoded run
    # is a drop-in for `run_qir` on the same program.
    recorder = OutputRecordingPass()
    recorder.run(module)
    return [
        recorder.process_output(
            [Result.One if bit else Result.Zero for bit in row]
        )
        for row, alive in zip(values, keep)
        if alive
    ]


__all__ = [
    "EncodedProgram",
    "LogicalSlot",
    "encodable_gates_of",
    "encode_qir",
    "run_qir_encoded",
    "stim_noise_from",
]
