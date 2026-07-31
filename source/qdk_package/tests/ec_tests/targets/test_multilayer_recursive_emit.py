"""Regression test for multi-layer (recursive) decoding-surface composition.

The :mod:`qdk.ec.targets.stim` emitter composes *every* layer's decoding
surface (``checks`` / ``readouts``) down to physical records when a codec has
more than one lowering edge and no explicit compiler is supplied. Historically
only the bottom layer's surface was emitted, silently discarding any
intermediate-layer detectors / observables.

This test wraps the distance-3 split-syndrome repetition codec (the
:mod:`test_cross_gadget_frames` fixture, a single ``logical -> physical``
lowering) in a trivial top layer whose gadgets merely expand to the logical
instructions:

    top.prepare -> [prepare_ref]
    top.idle    -> [syndrome_a, syndrome_b]
    top.measure -> [measure]

The top code is the trivial 1-qubit code (no stabilizers), so the top layer
contributes no decoding surface of its own. The repetition code's detectors
and logical observable live entirely on the *intermediate* (logical ->
physical) lowering — exactly the surface the old emitter dropped.

Oracle: the equivalent two-layer codec (logical -> physical only) running the
already-flattened program. Lowering ``[prepare, idle, idle, measure]`` through
the wrapper yields the same logical schedule
``[prepare_ref, syndrome_a, syndrome_b, syndrome_a, syndrome_b, measure]``, so
the two emitted circuits must agree structurally and both be a valid,
deterministic encoding of the same logical schedule.

The codecs are built directly through the qodec Python API (rather than loaded
from on-disk YAML): the mid/physical layers use Stim gadget bodies, and the top
gadgets use inline-program (``format="yaml"``) bodies that call into the middle
ISA.
"""

from __future__ import annotations

import re

import numpy as np
import pytest

pytest.importorskip("stim")

import qodec  # noqa: E402
from qodec.actions import Clifford, Observe, Stabilize  # noqa: E402
from qodec.instructions import InstructionCall as Call  # noqa: E402

from qodec.circuits import Program  # noqa: E402
from qdk.ec.targets import StimEmitter  # noqa: E402


def _physical_isa() -> qodec.InstructionSet:
    phys_qubit = qodec.instructions.Block("phys_qubit", encodes=1)
    target = qodec.instructions.BlockOperand("phys_qubit")
    control = qodec.instructions.BlockOperand("phys_qubit")
    return qodec.InstructionSet(
        name="RepPhysical",
        blocks=[phys_qubit],
        instructions=[
            qodec.Instruction(mnemonic="R", outputs=[target], action=[Stabilize(["Z_0"])]),
            qodec.Instruction(
                mnemonic="CX", inputs=[control, target], outputs=[control, target],
                action=[Clifford({"X_0": "X_0 X_1", "Z_1": "Z_0 Z_1"})],
            ),
            qodec.Instruction(mnemonic="M", inputs=[target], action=[Observe(["Z_0"])]),
        ],
    )


def _logical_isa() -> qodec.InstructionSet:
    mem = qodec.instructions.BlockOperand("mem")
    return qodec.InstructionSet(
        name="RepLogical",
        blocks=[qodec.instructions.Block("mem", encodes=1)],
        instructions=[
            qodec.Instruction(mnemonic="prepare_ref", outputs=[mem], action=[Stabilize(["Z_0"])]),
            qodec.Instruction(mnemonic="syndrome_a", inputs=[mem], outputs=[mem]),
            qodec.Instruction(mnemonic="syndrome_b", inputs=[mem], outputs=[mem]),
            qodec.Instruction(mnemonic="measure", inputs=[mem], action=[Observe(["Z_0"])]),
        ],
    )


def _top_isa() -> qodec.InstructionSet:
    log = qodec.instructions.BlockOperand("log")
    return qodec.InstructionSet(
        name="RepTop",
        blocks=[qodec.instructions.Block("log", encodes=1)],
        instructions=[
            qodec.Instruction(mnemonic="prepare", outputs=[log], action=[Stabilize(["Z_0"])]),
            qodec.Instruction(mnemonic="idle", inputs=[log], outputs=[log]),
            qodec.Instruction(mnemonic="measure", inputs=[log], action=[Observe(["Z_0"])]),
        ],
    )


def _mid_gadgets(
    logical_isa: qodec.InstructionSet,
    physical_isa: qodec.InstructionSet,
    code: qodec.Code,
) -> list[qodec.Gadget]:
    def enc() -> qodec.gadgets.Encoding:
        return qodec.gadgets.Encoding(code=code, support=["0", "1", "2"])

    def body(source: str) -> qodec.gadgets.Circuit:
        return qodec.gadgets.Circuit(physical_isa, source, format="stim")

    return [
        qodec.Gadget(
            implements=logical_isa.instruction("prepare_ref"),
            circuit=body("R 0 1 2 3 4\nCX 0 3 1 3\nCX 1 4 2 4\nM 3 4\n"),
            outputs=[enc()],
            checks=[
                ["circuit.readouts[0]", "out[0].stabilizers[0]"],
                ["circuit.readouts[1]", "out[0].stabilizers[1]"],
            ],
        ),
        qodec.Gadget(
            implements=logical_isa.instruction("syndrome_a"),
            circuit=body("R 3\nCX 0 3 1 3\nM 3\n"),
            inputs=[enc()], outputs=[enc()],
            checks=[
                ["circuit.readouts[0]", "in[0].stabilizers[0]"],
                ["circuit.readouts[0]", "out[0].stabilizers[0]"],
                ["in[0].stabilizers[1]", "out[0].stabilizers[1]"],
            ],
        ),
        qodec.Gadget(
            implements=logical_isa.instruction("syndrome_b"),
            circuit=body("R 3\nCX 1 3 2 3\nM 3\n"),
            inputs=[enc()], outputs=[enc()],
            checks=[
                ["circuit.readouts[0]", "in[0].stabilizers[1]"],
                ["circuit.readouts[0]", "out[0].stabilizers[1]"],
                ["in[0].stabilizers[0]", "out[0].stabilizers[0]"],
            ],
        ),
        qodec.Gadget(
            implements=logical_isa.instruction("measure"),
            circuit=body("M 0 1 2\n"),
            inputs=[enc()],
            checks=[
                ["circuit.readouts[0]", "circuit.readouts[1]", "in[0].stabilizers[0]"],
                ["circuit.readouts[1]", "circuit.readouts[2]", "in[0].stabilizers[1]"],
            ],
            readouts=[["circuit.readouts[0]", "in[0].z[0]"]],
        ),
    ]


def _build_two_layer_codec() -> qodec.Qodec:
    """The ``logical -> physical`` oracle codec (the cross-gadget fixture)."""
    physical_isa = _physical_isa()
    logical_isa = _logical_isa()
    rep3 = qodec.Code(
        name="Rep3", stabilizers=["Z_0 Z_1", "Z_1 Z_2"], x=["X_0 X_1 X_2"], z=["Z_0"]
    )
    return qodec.Qodec(
        layers=[
            qodec.Layer(logical_isa, gadgets=_mid_gadgets(logical_isa, physical_isa, rep3)),
            qodec.Layer(physical_isa),
        ],
        name="rep3-split",
    )


def _build_three_layer_codec() -> qodec.Qodec:
    """The ``top -> logical -> physical`` wrapper codec.

    The top layer's gadgets use inline-program (``format="yaml"``) bodies that
    expand each top instruction into a small program in the middle (logical)
    ISA. The top code is trivial (no stabilizers), so the entire decoding
    surface lives on the intermediate logical->physical lowering.
    """
    physical_isa = _physical_isa()
    logical_isa = _logical_isa()
    top_isa = _top_isa()
    rep3 = qodec.Code(
        name="Rep3", stabilizers=["Z_0 Z_1", "Z_1 Z_2"], x=["X_0 X_1 X_2"], z=["Z_0"]
    )
    trivial = qodec.Code(name="Trivial1", stabilizers=[], x=["X_0"], z=["Z_0"])

    def tenc() -> qodec.gadgets.Encoding:
        return qodec.gadgets.Encoding(code=trivial, support=["0"])

    def tbody(source: str) -> qodec.gadgets.Circuit:
        return qodec.gadgets.Circuit(logical_isa, source, format="yaml")

    top_prepare = qodec.Gadget(
        implements=top_isa.instruction("prepare"),
        circuit=tbody("- prepare_ref:\n    state: 0\n"),
        outputs=[tenc()],
        checks=[],
    )
    top_idle = qodec.Gadget(
        implements=top_isa.instruction("idle"),
        circuit=tbody("- syndrome_a:\n    state: 0\n- syndrome_b:\n    state: 0\n"),
        inputs=[tenc()], outputs=[tenc()],
        checks=[],
    )
    top_measure = qodec.Gadget(
        implements=top_isa.instruction("measure"),
        circuit=tbody("- measure:\n    state: 0\n"),
        inputs=[tenc()],
        readouts=[["circuit.readouts[0]", "in[0].z[0]"]],
    )

    return qodec.Qodec(
        layers=[
            qodec.Layer(top_isa, gadgets=[top_prepare, top_idle, top_measure]),
            qodec.Layer(logical_isa, gadgets=_mid_gadgets(logical_isa, physical_isa, rep3)),
            qodec.Layer(physical_isa),
        ],
        name="rep3-wrapped",
    )


def _two_layer_program(isa: qodec.InstructionSet, rounds: int) -> Program:
    calls = [Call("prepare_ref", outputs={"state": "M"})]
    for _ in range(rounds):
        calls.append(Call("syndrome_a", inputs={"state": "M"}, outputs={"state": "M"}))
        calls.append(Call("syndrome_b", inputs={"state": "M"}, outputs={"state": "M"}))
    calls.append(Call("measure", inputs={"state": "M"}))
    return Program(calls, isa)


def _three_layer_program(isa: qodec.InstructionSet, rounds: int) -> Program:
    calls = [Call("prepare", outputs={"state": "log"})]
    for _ in range(rounds):
        calls.append(Call("idle", inputs={"state": "log"}, outputs={"state": "log"}))
    calls.append(Call("measure", inputs={"state": "log"}))
    return Program(calls, isa)


def _detector_record_offsets(circuit_text: str) -> list[list[int]]:
    offsets: list[list[int]] = []
    for line in circuit_text.splitlines():
        if line.strip().startswith("DETECTOR"):
            recs = [int(match) for match in re.findall(r"rec\[(-\d+)\]", line)]
            offsets.append(recs)
    return offsets


def test_recursive_emit_matches_two_layer_oracle() -> None:
    rounds = 2

    two_codec = _build_two_layer_codec()
    two_circuit = StimEmitter(two_codec).build_circuit(
        _two_layer_program(two_codec.layers[0].isa, rounds)
    )

    three_codec = _build_three_layer_codec()
    three_circuit = StimEmitter(three_codec).build_circuit(
        _three_layer_program(three_codec.layers[0].isa, rounds)
    )

    # The recursive path composes the intermediate surface without the flat
    # path's MPAD virtual-record padding, so the two circuits are not byte
    # identical; instead they must agree structurally and both be a valid,
    # deterministic encoding of the same logical schedule.
    assert three_circuit.num_detectors == two_circuit.num_detectors
    assert three_circuit.num_observables == two_circuit.num_observables
    # The recursive path emits no MPAD virtual records, so it has no more
    # measurement records than the flat path (which pads absent prior gadgets).
    assert three_circuit.num_measurements <= two_circuit.num_measurements

    for circuit in (two_circuit, three_circuit):
        detectors, _ = circuit.compile_detector_sampler().sample(
            4000, separate_observables=True
        )
        assert bool(np.all(detectors.mean(axis=0) == 0.0))

    # The wrapped circuit actually carries the intermediate decoding surface
    # (the old bottom-only emitter would have produced zero of each).
    assert three_circuit.num_detectors > 0
    assert three_circuit.num_observables == 1

    # ...and the recursive path does not pad with virtual MPAD records.
    assert "MPAD" not in str(three_circuit)


def test_recursive_emit_is_deterministic_and_cross_gadget() -> None:
    rounds = 3
    codec = _build_three_layer_codec()
    circuit = StimEmitter(codec).build_circuit(
        _three_layer_program(codec.layers[0].isa, rounds)
    )

    detectors, _ = circuit.compile_detector_sampler().sample(
        4000, separate_observables=True
    )
    means = detectors.mean(axis=0)
    assert bool(np.all(means == 0.0)), "all detectors must be deterministic when noiseless"

    offsets = _detector_record_offsets(str(circuit))
    non_adjacent = [
        recs for recs in offsets if len(recs) == 2 and abs(recs[0] - recs[1]) > 1
    ]
    assert len(non_adjacent) >= rounds - 1, (
        "expected cross-gadget detectors composed through the top layer; "
        f"got offsets {offsets}"
    )


def test_recursive_emit_detects_injected_faults() -> None:
    codec = _build_three_layer_codec()
    program = _three_layer_program(codec.layers[0].isa, rounds=3)

    noisy = StimEmitter(codec, noise={"p_meas": 0.1, "p_data": 0.1})
    circuit = noisy.build_circuit(program)
    detectors = circuit.compile_detector_sampler().sample(4000)
    means = detectors.mean(axis=0)
    assert bool(np.any(means > 0.0)), "injected noise must make some detector fire"
