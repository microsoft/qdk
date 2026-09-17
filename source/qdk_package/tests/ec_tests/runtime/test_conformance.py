from . import FIXTURES
from collections.abc import Sequence
from pathlib import Path

import pytest
import qodec
from qodec.instructions import InstructionCall

from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
from qdk.simulation._qodec.execution_pipeline import Executor
from qdk.simulation._qodec.protocols import Readouts, Requests, Resources
from qdk.simulation._qodec.quantum_backend import (
    QuantumBackend,
    stabilizer_backend,
    tableau_backend,
)


def run_calls(
    codec,
    calls,
    blocks,
    seed=7,
    decoder=prepare_syndrome_decoder,
    backend_factory=stabilizer_backend,
):
    class Runtime:
        def required_resources(self, program) -> Resources:
            return Resources(blocks=blocks)

        def run(self, program) -> Requests[tuple[Readouts, ...]]:
            records = []
            for call in program:
                reply = yield call
                if reply is None:
                    raise TypeError("Missing instruction reply")
                records.append(reply)
            return tuple(records)

    executor = Executor(codec, decoder, None, Runtime, backend_factory)
    executor.set_seed(seed)
    return executor.run(calls)


@pytest.mark.parametrize("basis", ["x", "z"])
def test_published_steane_preparation_and_measurement(basis):
    codec = qodec.Qodec.load(Path(__file__).parent / "fixtures/steane/qodec.yaml")
    for seed in range(3):
        records = run_calls(
            codec,
            [
                InstructionCall(f"prepare_{basis}", operands=["data"]),
                InstructionCall(f"measure_{basis}", operands=["data"]),
            ],
            {"steane": 1},
            seed,
        )
        assert records == ((), (False,))


def test_published_steane_flagged_round_and_basis_transport():
    codec = qodec.Qodec.load(Path(__file__).parent / "fixtures/steane/qodec.yaml")
    records = run_calls(
        codec,
        [
            InstructionCall("prepare_z", operands=["data"]),
            InstructionCall("idle_ft", operands=["data"]),
            InstructionCall("h", operands=["data"]),
            InstructionCall("measure_x", operands=["data"]),
        ],
        {"steane": 1},
    )
    assert records == ((), (False,) * 6, (), (False,))


@pytest.mark.parametrize("basis", ["x", "z"])
def test_published_c4_preparation_with_explicit_frame(basis):
    codec = qodec.Qodec.load(Path(__file__).parent / "fixtures/c4c6/qodec.yaml").slice(
        1, 3
    )
    for seed in range(3):
        records = run_calls(
            codec,
            [
                InstructionCall(f"prepare_{basis}_all", operands=["data"]),
                InstructionCall(f"measure_{basis}_all", operands=["data"]),
            ],
            {"c4c6_block": 1},
            seed,
        )
        assert records == ((False,), (False, False))


@pytest.mark.parametrize("basis", ["x", "z"])
def test_published_c4c6_preparation_and_readout(basis):
    from qdk.simulation._qodec.frame_runtime import prepare_frame_decoder

    codec = qodec.Qodec.load(Path(__file__).parent / "fixtures/c4c6/qodec.yaml")
    records = run_calls(
        codec,
        [
            InstructionCall(f"prepare_{basis}_all", operands=["data"]),
            InstructionCall(f"measure_{basis}_all", operands=["data"]),
        ],
        {"c4c6_block": 1},
        decoder=prepare_frame_decoder,
        backend_factory=tableau_backend,
    )
    assert records == ((False,), (False, False))


@pytest.mark.parametrize("gate", ["mul_u", "mul_u_sq"])
def test_c4_clifford_frame_transport_includes_stabilizer_signs(gate):
    from qdk.simulation._qodec.frame_runtime import prepare_frame_decoder

    codec = qodec.Qodec.load(Path(__file__).parent / "fixtures/c4c6/qodec.yaml").slice(
        1, 3
    )
    records = run_calls(
        codec,
        [
            InstructionCall("prepare_x_all", operands=["data"]),
            InstructionCall(gate, operands=["data"]),
            InstructionCall("measure_x_all", operands=["data"]),
        ],
        {"c4c6_block": 1},
        seed=0,
        decoder=prepare_frame_decoder,
    )
    assert records == ((False,), (), (False, False))


def test_published_c4c6_teleportation_preserves_the_prepared_state():
    from qdk.simulation._qodec.frame_runtime import prepare_frame_decoder

    codec = qodec.Qodec.load(Path(__file__).parent / "fixtures/c4c6/qodec.yaml")
    records = run_calls(
        codec,
        [
            InstructionCall("prepare_z_all", operands=["data"]),
            InstructionCall("idle", operands=["data"]),
            InstructionCall("measure_z_all", operands=["data"]),
        ],
        {"c4c6_block": 1},
        decoder=prepare_frame_decoder,
        backend_factory=tableau_backend,
    )
    assert records == ((False,), (), (False, False))


def test_supplied_code_switch_preserves_a_logical_superposition():
    from qodec.actions import Clifford, Observe, Stabilize
    from qodec.gadgets import Circuit, Encoding
    from qodec.instructions import Block, BlockOperand

    target = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    declarations = target.instructions
    wire = BlockOperand("qubit")
    declarations["H"] = qodec.Instruction(
        "H",
        inputs=[wire],
        outputs=[wire],
        action=[Clifford({"X_0": "Z_0", "Z_0": "X_0"})],
    )
    target.instructions = declarations
    small = qodec.Code("small", ["Z_0 Z_1"], ["X_0 X_1"], ["Z_0"])
    large = qodec.Code("large", ["Z_0 Z_1", "Z_1 Z_2"], ["X_0 X_1 X_2"], ["Z_0"])
    prepare = qodec.Instruction(
        "prepare_plus", outputs=[BlockOperand("small")], action=[Stabilize(["X_0"])]
    )
    grow = qodec.Instruction(
        "grow", inputs=[BlockOperand("small")], outputs=[BlockOperand("large")]
    )
    measure = qodec.Instruction(
        "measure_x", inputs=[BlockOperand("large")], action=[Observe(["X_0"])]
    )
    source = qodec.InstructionSet(
        "switching",
        blocks=[Block("small", 1), Block("large", 1)],
        instructions=[prepare, grow, measure],
    )
    initial = Encoding(small, support=["0", "1"])
    expanded = Encoding(large, support=["0", "1", "2"])
    codec = qodec.Qodec(
        [
            qodec.Layer(
                source,
                codes={"small": small, "large": large},
                gadgets=[
                    qodec.Gadget(
                        prepare,
                        Circuit(target, "R 0 1\nH 0\nCX 0 1", format="stim"),
                        outputs=[initial],
                        checks=[["out[0].stabilizers[0]"]],
                    ),
                    qodec.Gadget(
                        grow,
                        Circuit(target, "R 2\nCX 1 2", format="stim"),
                        inputs=[initial],
                        outputs=[expanded],
                        checks=[
                            ["in[0].stabilizers[0]", "out[0].stabilizers[0]"],
                            ["out[0].stabilizers[1]"],
                        ],
                    ),
                    qodec.Gadget(
                        measure,
                        Circuit(target, "H 0 1 2\nM 0 1 2", format="stim"),
                        inputs=[expanded],
                        readouts=[["circuit.readouts[0:3]", "in[0].x[0]"]],
                    ),
                ],
            ),
            qodec.Layer(target),
        ]
    )
    records = run_calls(
        codec,
        [
            InstructionCall("prepare_plus", operands=["data"]),
            InstructionCall("grow", operands=["data"]),
            InstructionCall("measure_x", operands=["data"]),
        ],
        {"small": 1, "large": 1},
    )
    assert records == ((), (), (False,))


def test_temporary_ancilla_channel_preserves_the_unmeasured_subspace():
    from math import pi, sqrt

    import numpy as np
    from qodec.actions import Observe, Rotate, Stabilize
    from qodec.instructions import Block, BlockOperand

    from qdk.simulation._qodec.full_state_engine import FullStateEngine
    from .test_quantum_instruments import assert_same_state

    operand = BlockOperand("pair")
    isa = qodec.InstructionSet(
        "joint_measurement",
        blocks=[Block("pair", 2)],
        instructions=[
            qodec.Instruction(
                "prepare_plus", outputs=[operand], action=[Stabilize(["X_0", "X_1"])]
            ),
            qodec.Instruction(
                "both_one",
                inputs=[operand],
                outputs=[operand],
                action=[
                    Stabilize(["Z_7"]),
                    Rotate("Y_7", pi / 4),
                    Rotate("Z_0 Y_7", -pi / 4),
                    Rotate("Z_1 Y_7", -pi / 4),
                    Rotate("Z_0 Z_1 Y_7", pi / 4),
                    Observe(["Z_7"]),
                ],
            ),
        ],
    )
    states = []

    class Engine(FullStateEngine):
        def close(self):
            states.append(self.state())
            super().close()

    def backend(noise, seed):
        return QuantumBackend(
            noise,
            seed,
            engine_factory=lambda capacity, engine_seed: Engine(
                capacity, seed=engine_seed
            ),
        )

    for seed in range(8):
        records = run_calls(
            qodec.Qodec([qodec.Layer(isa)]),
            [
                InstructionCall("prepare_plus", operands=["data"]),
                InstructionCall("both_one", operands=["data"]),
            ],
            {"pair": 1},
            seed=seed,
            backend_factory=backend,
        )
        (outcome,) = records[1]
        expected = np.zeros(8, dtype=complex)
        if outcome:
            expected[3] = 1
        else:
            expected[:3] = 1 / sqrt(3)
        assert_same_state(states[-1], expected)
