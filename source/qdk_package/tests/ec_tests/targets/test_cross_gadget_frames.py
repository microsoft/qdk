"""Regression test for cross-gadget (non-adjacent) stabilizer frame resolution.

The :mod:`qdk.ec.targets.stim` emitter resolves an ``in[<entry>].stabilizers[i]``
atom by looking up the absolute measurement records that last refreshed that
stabilizer frame (``frame_map``), rather than assuming the records sit at a fixed
positional offset from the end of the gadget body. This matters when a stabilizer
is re-measured *across* an intervening gadget that measured a different
stabilizer: the cross-round detector must reach back past the intervening gadget
to the previous same-stabilizer measurement.

This test builds a minimal distance-3 repetition memory whose syndrome rounds are
split into two single-stabilizer half-gadgets (``syndrome_a`` measures Z0Z1,
``syndrome_b`` measures Z1Z2). A measuring reference preparation seeds the frame
map. The schedule ``prepare_ref, a, b, a, b, measure`` forces the second
``syndrome_a`` detector to compare its outcome against the first ``syndrome_a``
outcome across the intervening ``syndrome_b`` record.

Assertions:
  * Noiseless: every detector is deterministic (never fires).
  * At least one detector references two records whose offsets differ by more
    than one, proving non-adjacent (cross-gadget) resolution rather than a
    positional fallback (which would compare against the wrong, adjacent record
    and fire ~50% of the time).

The codec is built directly through the qodec Python API (rather than loaded
from on-disk YAML) so the fixture stays a single self-contained module.
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


def _build_codec() -> qodec.Qodec:
    """Build the distance-3 split-syndrome repetition memory codec.

    A single ``logical -> physical`` lowering: the ``RepLogical`` ISA's four
    instructions (``prepare_ref``, ``syndrome_a``, ``syndrome_b``,
    ``measure``) lower to small ``RepPhysical`` (Stim) circuits. The
    half-syndrome gadgets carry the cross-round detector declarations that
    exercise non-adjacent frame resolution.
    """
    phys_qubit = qodec.instructions.Block("phys_qubit", encodes=1)
    target = qodec.instructions.BlockOperand("phys_qubit")
    control = qodec.instructions.BlockOperand("phys_qubit")
    physical_isa = qodec.InstructionSet(
        name="RepPhysical",
        blocks=[phys_qubit],
        instructions=[
            qodec.Instruction(
                mnemonic="R", outputs=[target], action=[Stabilize(["Z_0"])]
            ),
            qodec.Instruction(
                mnemonic="CX",
                inputs=[control, target], outputs=[control, target],
                action=[Clifford({"X_0": "X_0 X_1", "Z_1": "Z_0 Z_1"})],
            ),
            qodec.Instruction(
                mnemonic="M", inputs=[target], action=[Observe(["Z_0"])]
            ),
        ],
    )

    mem = qodec.instructions.BlockOperand("mem")
    logical_isa = qodec.InstructionSet(
        name="RepLogical",
        blocks=[qodec.instructions.Block("mem", encodes=1)],
        instructions=[
            qodec.Instruction(
                mnemonic="prepare_ref", outputs=[mem], action=[Stabilize(["Z_0"])]
            ),
            qodec.Instruction(mnemonic="syndrome_a", inputs=[mem], outputs=[mem]),
            qodec.Instruction(mnemonic="syndrome_b", inputs=[mem], outputs=[mem]),
            qodec.Instruction(
                mnemonic="measure", inputs=[mem], action=[Observe(["Z_0"])]
            ),
        ],
    )

    code = qodec.Code(
        name="Rep3",
        description="Distance-3 repetition code.",
        stabilizers=["Z_0 Z_1", "Z_1 Z_2"],
        x=["X_0 X_1 X_2"],
        z=["Z_0"],
    )

    def enc() -> qodec.gadgets.Encoding:
        return qodec.gadgets.Encoding(code=code, support=["0", "1", "2"])

    def body(source: str) -> qodec.gadgets.Circuit:
        return qodec.gadgets.Circuit(physical_isa, source, format="stim")

    prepare_ref = qodec.Gadget(
        implements=logical_isa.instruction("prepare_ref"),
        circuit=body("R 0 1 2 3 4\nCX 0 3 1 3\nCX 1 4 2 4\nM 3 4\n"),
        outputs=[enc()],
        checks=[
            ["circuit.readouts[0]", "out[0].stabilizers[0]"],
            ["circuit.readouts[1]", "out[0].stabilizers[1]"],
        ],
    )
    syndrome_a = qodec.Gadget(
        implements=logical_isa.instruction("syndrome_a"),
        circuit=body("R 3\nCX 0 3 1 3\nM 3\n"),
        inputs=[enc()], outputs=[enc()],
        checks=[
            ["circuit.readouts[0]", "in[0].stabilizers[0]"],
            ["circuit.readouts[0]", "out[0].stabilizers[0]"],
            ["in[0].stabilizers[1]", "out[0].stabilizers[1]"],
        ],
    )
    syndrome_b = qodec.Gadget(
        implements=logical_isa.instruction("syndrome_b"),
        circuit=body("R 3\nCX 1 3 2 3\nM 3\n"),
        inputs=[enc()], outputs=[enc()],
        checks=[
            ["circuit.readouts[0]", "in[0].stabilizers[1]"],
            ["circuit.readouts[0]", "out[0].stabilizers[1]"],
            ["in[0].stabilizers[0]", "out[0].stabilizers[0]"],
        ],
    )
    measure = qodec.Gadget(
        implements=logical_isa.instruction("measure"),
        circuit=body("M 0 1 2\n"),
        inputs=[enc()],
        checks=[
            ["circuit.readouts[0]", "circuit.readouts[1]", "in[0].stabilizers[0]"],
            ["circuit.readouts[1]", "circuit.readouts[2]", "in[0].stabilizers[1]"],
        ],
        readouts=[["circuit.readouts[0]", "in[0].z[0]"]],
    )

    return qodec.Qodec(
        layers=[
            qodec.Layer(
                logical_isa,
                gadgets=[prepare_ref, syndrome_a, syndrome_b, measure],
            ),
            qodec.Layer(physical_isa),
        ],
        name="rep3-split",
    )


def _detector_record_offsets(circuit_text: str) -> list[list[int]]:
    offsets: list[list[int]] = []
    for line in circuit_text.splitlines():
        if line.strip().startswith("DETECTOR"):
            recs = [int(match) for match in re.findall(r"rec\[(-\d+)\]", line)]
            offsets.append(recs)
    return offsets


def test_cross_gadget_frame_resolution_is_deterministic() -> None:
    codec = _build_codec()
    isa = codec.layers[0].isa

    calls = [Call("prepare_ref", outputs={"state": "M"})]
    for _ in range(2):
        calls.append(Call("syndrome_a", inputs={"state": "M"}, outputs={"state": "M"}))
        calls.append(Call("syndrome_b", inputs={"state": "M"}, outputs={"state": "M"}))
    calls.append(Call("measure", inputs={"state": "M"}))
    program = Program(calls, isa)

    circuit = StimEmitter(codec).build_circuit(program)
    detectors, _ = circuit.compile_detector_sampler().sample(4000, separate_observables=True)
    means = detectors.mean(axis=0)
    assert bool(np.all(means == 0.0)), "all detectors must be deterministic when noiseless"

    offsets = _detector_record_offsets(str(circuit))
    has_non_adjacent = any(
        len(recs) == 2 and abs(recs[0] - recs[1]) > 1 for recs in offsets
    )
    assert has_non_adjacent, (
        "expected a detector whose two records straddle an intervening gadget; "
        f"got offsets {offsets}"
    )


def test_cross_gadget_frame_resolution_deeper_schedule() -> None:
    """A longer split schedule keeps frames deterministic across many
    intervening gadgets.

    With ``rounds`` repetitions of ``(syndrome_a, syndrome_b)``, each
    ``syndrome_a`` detector must still reach back to the *previous*
    ``syndrome_a`` outcome — now separated by several ``syndrome_b``
    records and growing apart as the schedule lengthens. A positional
    fallback would compare against an adjacent (wrong) record and fire
    under the noiseless trajectory.
    """
    codec = _build_codec()
    isa = codec.layers[0].isa

    rounds = 4
    calls = [Call("prepare_ref", outputs={"state": "M"})]
    for _ in range(rounds):
        calls.append(Call("syndrome_a", inputs={"state": "M"}, outputs={"state": "M"}))
        calls.append(Call("syndrome_b", inputs={"state": "M"}, outputs={"state": "M"}))
    calls.append(Call("measure", inputs={"state": "M"}))
    program = Program(calls, isa)

    circuit = StimEmitter(codec).build_circuit(program)
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
        "expected one non-adjacent (cross-gadget) detector per re-measured round; "
        f"got offsets {offsets}"
    )
