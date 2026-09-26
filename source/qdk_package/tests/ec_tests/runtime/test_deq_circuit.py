from functools import partial
from itertools import product

import pytest
import qodec

from qdk import Result
from qdk.simulation import NoiseConfig
from qdk.simulation._qodec.decoding import prepare_deq_decoder
from qdk.simulation._qodec.native_batch import prepare_batch
from . import FIXTURES
from .test_execution_pipeline import compile_qasm
from .test_native_batch import make_factory


@pytest.fixture(params=[32, 1024])
def circuit_decoder(request, monkeypatch):
    pytest.importorskip("deq")
    pytest.importorskip("deq_runtime")
    from qdk.simulation._qodec import deq_composition

    monkeypatch.setattr(deq_composition, "_COMPOSITE_SIZE", request.param)
    return partial(prepare_deq_decoder, circuit_level=True)


def circuit_batch(circuit_decoder, noise=None, codec=None, program=None):
    if program is None:
        program = compile_qasm(
            'include "stdgates.inc"; qubit data; bit result = measure data;'
        )
    batch = prepare_batch(program, make_factory(noise, circuit_decoder, codec))
    assert batch is not None
    return batch


def physical_faults(trace, noise):
    from qdk._native import QirInstructionId
    from qdk.simulation._qodec.deq_circuit import _channel
    from qdk.simulation._qodec.native_batch import _FrameMasks, _GATES

    masks = _FrameMasks(trace.instructions)
    for position, (opcode, *operands) in enumerate(trace.instructions):
        if opcode in (QirInstructionId.MZ, QirInstructionId.RESET):
            name, targets = "mresetz", operands[:1]
        else:
            name = next(name for name, value in _GATES.items() if value == opcode)
            targets = operands
        for axes, probability in _channel(noise, name, len(targets)):
            effect = 0
            for target, axis in zip(targets, axes):
                if axis != "I":
                    effect ^= masks.mask(position + 1, target, axis.lower())
            if effect:
                yield effect, probability


def test_circuit_deq_never_constructs_or_replays_syndrome_decoder(
    circuit_decoder, monkeypatch
):
    from qdk.simulation._qodec.decoding import SyndromeModel, SyndromeSession
    from qdk.simulation._qodec.readout_equations import BinarySystem

    def forbidden(*args, **kwargs):
        pytest.fail("Circuit-level deq must not use the QDK syndrome/parity decoder")

    monkeypatch.setattr(SyndromeModel, "__init__", forbidden)
    monkeypatch.setattr(SyndromeSession, "decode", forbidden)
    monkeypatch.setattr(BinarySystem, "reduce", forbidden)
    program = compile_qasm(
        'include "stdgates.inc"; qubit[2] data; x data[1]; bit[2] result = measure data;'
    )
    batch = circuit_batch(circuit_decoder, program=program)
    assert batch.run(8, None, seed=9) == [[Result.Zero, Result.One]] * 8


def test_circuit_deq_connects_reusable_local_gadget_types(circuit_decoder):
    from qdk.simulation._qodec.deq_circuit import _connected_library

    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; x data; bit r = measure data;'
    )
    batch = circuit_batch(circuit_decoder, program=program)
    local_library, _ = _connected_library(batch.trace, None)
    assert len(local_library.gadget_types) == 3
    assert len(batch.gadgets) == 4
    assert batch.gadgets[1].gtype == batch.gadgets[2].gtype
    assert [gadget.connectors for gadget in batch.gadgets] == [
        (),
        ((1, 0),),
        ((2, 0),),
        ((3, 0),),
    ]
    assert [gadget.width for gadget in batch.gadgets] == [0, 0, 0, 3]
    assert batch.run(3, None, seed=9) == [[Result.Zero]] * 3


def memory_batch(circuit_decoder, noise=None):
    from qdk.simulation._simulation import preprocess_simulation_input
    from qdk.simulation._qodec.bytecode import compile

    rounds = "\n".join(
        f"call void @idle(%Qubit* inttoptr (i64 {block} to %Qubit*))"
        for _ in range(8)
        for block in range(2)
    )
    qir = f"""
        %Qubit = type opaque
        %Result = type opaque
        define void @main() #0 {{
          call void @prepare_z(%Qubit* null)
          call void @prepare_z(%Qubit* inttoptr (i64 1 to %Qubit*))
          {rounds}
          call void @__quantum__qis__m__body(%Qubit* null, %Result* null)
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__array_record_output(i64 2, i8* null)
          call void @__quantum__rt__result_record_output(%Result* null, i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* null)
          ret void
        }}
        declare void @prepare_z(%Qubit*)
        declare void @idle(%Qubit*)
        declare void @__quantum__qis__m__body(%Qubit*, %Result*)
        declare void @__quantum__rt__array_record_output(i64, i8*)
        declare void @__quantum__rt__result_record_output(%Result*, i8*)
        attributes #0 = {{ "entry_point" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }}
    """
    module, _, _, _ = preprocess_simulation_input(qir)
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    program = compile(module, codec.layers[0].instruction_set.instructions)
    return circuit_batch(circuit_decoder, noise, codec, program)


def test_composition_reuses_open_port_types_and_resets_completed_shots(
    circuit_decoder, monkeypatch
):
    from contextlib import asynccontextmanager
    from types import SimpleNamespace
    from qdk.simulation._qodec import deq_circuit, deq_composition

    monkeypatch.setattr(deq_composition, "_COMPOSITE_SIZE", 32)
    batch = memory_batch(circuit_decoder)
    assert 1 < len(batch.composites) < len(batch.gadgets)
    assert len(batch.library.gadget_types) < len(batch.composites)
    assert any(chunk.connectors for chunk in batch.composites)
    assert all(kind.base.is_free_hop is False for kind in batch.library.gadget_types)
    assert sorted(
        index for chunk in batch.composites for index in chunk.measurements
    ) == list(range(batch.trace.num_measurements))
    assert sorted(
        original for chunk in batch.composites for original, _ in chunk.readouts
    ) == list(range(len(batch.gadgets)))
    resets = []
    original_runtime = deq_circuit.Runtime

    @asynccontextmanager
    async def runtime(**kwargs):
        assert kwargs["coordinator"] == "window"
        assert kwargs["coordinator_config"] == {
            "buffer_radius": 1,
            "lookahead_radius": 1,
        }
        async with original_runtime(**kwargs) as runtime:
            service = runtime.jit_controller

            async def reset(**flags):
                resets.append(flags)
                await service.reset(**flags)

            yield SimpleNamespace(
                jit_controller=SimpleNamespace(
                    load_library=service.load_library,
                    batch_execute=service.batch_execute,
                    batch_decode=service.batch_decode,
                    reset=reset,
                )
            )

    monkeypatch.setattr(deq_circuit, "Runtime", runtime)
    assert batch.run(257, None, seed=9) == [[Result.Zero, Result.Zero]] * 257
    assert resets == [{"reset_library": False, "reset_decoder_service": False}]


def test_composition_corrects_single_faults_across_chunks(circuit_decoder, monkeypatch):
    from contextlib import closing
    from qdk.simulation._qodec import deq_composition
    from qdk.simulation._qodec.deq_decoding import _DeqTransport

    monkeypatch.setattr(deq_composition, "_COMPOSITE_SIZE", 32)
    noise = NoiseConfig()
    noise.cx.xi = 0.01
    batch = memory_batch(circuit_decoder, noise)
    assert len(batch.composites) > 2
    masks = {mask for mask, _ in physical_faults(batch.trace, noise)}
    assert len(masks) > 10
    rows = [
        [
            Result.One if mask >> index & 1 else Result.Zero
            for index in range(batch.trace.num_measurements)
        ]
        for mask in sorted(masks)
    ]
    with closing(_DeqTransport()) as transport:
        results = transport.run(batch._decode(batch.library, rows, 9, "raise"))
    assert results == [[Result.Zero, Result.Zero]] * len(rows)


def test_composition_keeps_individually_oversized_gadgets_intact(
    circuit_decoder, monkeypatch
):
    from qdk.simulation._qodec import deq_composition

    monkeypatch.setattr(deq_composition, "_COMPOSITE_SIZE", 1)
    batch = memory_batch(circuit_decoder)
    assert len(batch.composites) == len(batch.gadgets)
    assert all(len(chunk.readouts) == 1 for chunk in batch.composites)
    assert batch.run(3, None, seed=9) == [[Result.Zero, Result.Zero]] * 3


@pytest.mark.parametrize("source", ["x data;", "x data; reset data; x data;"])
def test_circuit_deq_closes_discarded_and_live_outputs(circuit_decoder, source):
    batch = circuit_batch(
        circuit_decoder,
        program=compile_qasm(f'include "stdgates.inc"; qubit data; {source}'),
    )
    assert any(gadget.event is None for gadget in batch.gadgets)
    assert batch.run(3, None, seed=9) == [[]] * 3


def test_circuit_deq_does_not_add_undeclared_detection_checks(circuit_decoder):
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    codec.layers[0].gadgets["__quantum__qis__m__body"].checks = []
    batch = circuit_batch(circuit_decoder, codec=codec)
    assert all(not gadget.finished_checks for gadget in batch.library.gadget_types)


def test_circuit_deq_trusts_authored_check_sign_until_decoding(circuit_decoder):
    from qdk.simulation._qodec.protocols import ExecutionUnresolved

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    codec.layers[0].gadgets["__quantum__qis__m__body"].checks = [
        ["circuit.readouts[0]", "circuit.readouts[1]", 1]
    ]
    batch = circuit_batch(circuit_decoder, codec=codec)
    with pytest.raises(ExecutionUnresolved, match="explain"):
        batch.run(1, None, seed=9, on_shot_failure="raise")


def test_circuit_deq_preserves_a_pauli_before_a_logical_readout(circuit_decoder):
    from qodec.gadgets import Circuit

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    measure = codec.layers[0].gadgets["__quantum__qis__m__body"]
    measure.circuit = Circuit(
        codec.layers[-1].instruction_set, "X 0 1 2\nM 0 1 2", format="stim"
    )
    batch = circuit_batch(circuit_decoder, codec=codec)
    assert batch.run(3, None, seed=9) == [[Result.One]] * 3


def test_circuit_deq_corrects_all_single_bit_errors(circuit_decoder):
    from qdk.simulation._qodec.deq_decoding import _DeqTransport
    from contextlib import closing

    noise = NoiseConfig()
    noise.mresetz.x = 0.01
    batch = circuit_batch(circuit_decoder, noise)
    rows = [
        [Result.One if index == fault else Result.Zero for index in range(3)]
        for fault in range(3)
    ]
    with closing(_DeqTransport()) as transport:
        results = transport.run(batch._decode(batch.library, rows, 9, "raise"))
    assert results == [[Result.Zero]] * 3


def test_circuit_deq_preserves_raw_rejection_flags(circuit_decoder):
    from qdk.simulation._qodec.deq_decoding import _DeqTransport
    from qdk.simulation._qodec.protocols import ExecutionRejected
    from contextlib import closing

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    gadget = codec.layers[0].gadgets["__quantum__qis__m__body"]
    gadget.implements.flags = ["reject"]
    gadget.readouts.append({"reject": ["circuit.readouts[0]", "circuit.readouts[1]"]})
    noise = NoiseConfig()
    noise.mresetz.x = 0.01
    batch = circuit_batch(circuit_decoder, noise, codec)
    rows = [[Result.One, Result.Zero, Result.Zero], [Result.Zero] * 3]
    library = batch.library
    with closing(_DeqTransport()) as transport:
        assert transport.run(batch._decode(library, rows, 9, "discard")) == [
            [Result.Zero]
        ]
        with pytest.raises(ExecutionRejected):
            transport.run(batch._decode(library, rows, 9, "raise"))


def test_circuit_deq_skips_selected_raw_flags_only_when_discarding(
    circuit_decoder, monkeypatch
):
    from contextlib import closing
    from qdk.simulation._qodec import deq_circuit
    from qdk.simulation._qodec.deq_decoding import _DeqTransport
    from qdk.simulation._qodec.protocols import ExecutionUnresolved

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    measure = codec.layers[0].gadgets["__quantum__qis__m__body"]
    measure.implements.flags = ["reject"]
    measure.readouts.append({"reject": ["circuit.readouts[0]", "circuit.readouts[1]"]})
    batch = circuit_batch(circuit_decoder, codec=codec)
    rows = [[Result.One, Result.Zero, Result.Zero]]
    with closing(_DeqTransport()) as transport:
        with pytest.raises(ExecutionUnresolved, match="explain"):
            transport.run(batch._decode(batch.library, rows, 9, "raise"))

        def forbidden(**kwargs):
            pytest.fail("Selected-out raw flags must not reach deq under discard")

        monkeypatch.setattr(deq_circuit, "Runtime", forbidden)
        assert transport.run(batch._decode(batch.library, rows, 9, "discard")) == []


@pytest.mark.parametrize("probability", [0, 0.01, 0.75, 1])
@pytest.mark.parametrize("shots", [20, 257])
def test_circuit_deq_seeded_runs_are_reproducible(circuit_decoder, probability, shots):
    noise = NoiseConfig()
    noise.mresetz.x = probability
    batch = circuit_batch(circuit_decoder, noise)
    first = batch.run(shots, noise, seed=42)
    assert first == batch.run(shots, noise, seed=42)
    assert len(first) == shots
    if probability in (0, 1):
        assert first == [[Result.Zero]] * shots


def test_circuit_deq_requires_explicit_supported_execution(circuit_decoder):
    from qdk.simulation import run_qir
    import qdk.openqasm
    from qdk import TargetProfile

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit data; bit result = measure data;',
        target_profile=TargetProfile.Adaptive,
    )
    for options in ({"type": "cpu"}, {"on_shot_failure": "retry"}):
        with pytest.raises(NotImplementedError, match="Circuit-level deq"):
            run_qir(qir, qodec=codec, decoder=circuit_decoder, **options)
    with pytest.raises(ValueError, match="not error_probability"):
        circuit_decoder(codec.layers[0], error_probability=0.01)
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; bit r = measure data; if (r) { x data; }'
    )
    with pytest.raises(NotImplementedError, match="Circuit-level deq"):
        circuit_batch(circuit_decoder, program=program)


def test_circuit_deq_rejects_general_pauli_channels(circuit_decoder):
    noise = NoiseConfig()
    noise.mresetz.x = 0.02
    noise.mresetz.z = 0.01
    with pytest.raises(NotImplementedError, match="general Pauli channels"):
        circuit_batch(circuit_decoder, noise)


@pytest.mark.parametrize("width", [1, 2])
def test_circuit_deq_depolarizing_mechanisms_reproduce_the_channel(
    circuit_decoder, width
):
    from qdk.simulation._qodec.deq_circuit import _channel

    noise = NoiseConfig()
    name = "x" if width == 1 else "cx"
    getattr(noise, name).set_depolarizing(0.12)
    mechanisms = _channel(noise, name, width)
    # Convolve the independent mechanisms over the Pauli group, ignoring phase.
    distribution = {("I",) * width: 1.0}
    for axes, probability in mechanisms:
        updated = {}
        for previous, weight in distribution.items():
            following = tuple(
                "IXYZ"["IXYZ".index(a) ^ "IXYZ".index(b)]
                for a, b in zip(previous, axes)
            )
            updated[previous] = updated.get(previous, 0.0) + weight * (1 - probability)
            updated[following] = updated.get(following, 0.0) + weight * probability
        distribution = updated
    for axes in product("IXYZ", repeat=width):
        expected = 0.88 if axes == ("I",) * width else 0.12 / (4**width - 1)
        assert distribution[axes] == pytest.approx(expected)


def test_circuit_deq_aliases_and_constants_are_compiled_once(circuit_decoder):
    from qdk.simulation._qodec.deq_circuit import _Parity, _eliminate_aliases
    from qdk.simulation._qodec.protocols import ExecutionUnresolved

    # r0 = r1; r1 = m0 XOR 1.
    readouts, checks = _eliminate_aliases([_Parity(0b110), _Parity(0b101, True)], 1, 2)
    assert readouts == [_Parity(1, True), _Parity(1, True)]
    assert checks == []
    with pytest.raises(ExecutionUnresolved, match="underdetermined"):
        _eliminate_aliases([_Parity(0b110)], 1, 2)


@pytest.mark.parametrize("record_dependent", [False, True])
def test_circuit_deq_applies_declared_frames_once(circuit_decoder, record_dependent):
    from qodec.gadgets import Circuit

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    prepare = codec.layers[0].gadgets["prepare_z"]
    if record_dependent:
        prepare.circuit = Circuit(
            codec.layers[-1].instruction_set,
            "R 0 1 2 3\nX 3\nM 3",
            format="stim",
        )
    prepare.frames = {
        "out[0].z[0]": ["circuit.readouts[0]"] if record_dependent else [1]
    }
    batch = circuit_batch(circuit_decoder, codec=codec)
    assert batch.run(10, None, seed=42) == [[Result.One]] * 10


def test_circuit_deq_handles_random_preparation_syndromes(circuit_decoder):
    from qodec.actions import Clifford
    from qodec.gadgets import Circuit
    from qodec.instructions import BlockOperand

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    physical = codec.layers[-1].instruction_set
    physical.instructions["H"] = qodec.Instruction(
        "H",
        inputs=[BlockOperand("qubit")],
        outputs=[BlockOperand("qubit")],
        action=[Clifford({"X_0": "Z_0", "Z_0": "X_0"})],
    )
    prepare = codec.layers[0].gadgets["prepare_z"]
    prepare.circuit = Circuit(physical, "R 0 1 2 3\nH 1\nCX 1 3\nM 3", format="stim")
    prepare.checks = [
        ["out[0].stabilizers[0]", "circuit.readouts[0]"],
        ["out[0].stabilizers[1]", "circuit.readouts[0]"],
        ["out[0].z[0]"],
    ]
    measure = codec.layers[0].gadgets["__quantum__qis__m__body"]
    measure.implements.flags = ["reject"]
    measure.readouts.append({"reject": ["circuit.readouts[0]", "circuit.readouts[1]"]})
    batch = circuit_batch(circuit_decoder, codec=codec)
    assert (
        batch.run(100, None, seed=42, on_shot_failure="raise") == [[Result.Zero]] * 100
    )


def test_circuit_deq_logical_sign_equation_matches_an_authored_frame(circuit_decoder):
    from qodec.actions import Clifford
    from qodec.gadgets import Circuit
    from qodec.instructions import BlockOperand

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    physical = codec.layers[-1].instruction_set
    physical.instructions["H"] = qodec.Instruction(
        "H",
        inputs=[BlockOperand("qubit")],
        outputs=[BlockOperand("qubit")],
        action=[Clifford({"X_0": "Z_0", "Z_0": "X_0"})],
    )
    prepare = codec.layers[0].gadgets["prepare_z"]
    prepare.circuit = Circuit(physical, "R 0 1 2 3\nH 3\nM 3", format="stim")
    prepare.frames = {"out[0].z[0]": ["circuit.readouts[0]"]}
    expected = circuit_batch(circuit_decoder, codec=codec).run(30, None, seed=42)
    assert {row[0] for row in expected} == {Result.Zero, Result.One}
    prepare.frames = {}
    prepare.checks = [["out[0].z[0]", "circuit.readouts[0]"]]
    assert (
        circuit_batch(circuit_decoder, codec=codec).run(30, None, seed=42) == expected
    )


@pytest.mark.parametrize("aliased", [False, True])
def test_circuit_deq_resolves_coupled_and_aliased_logical_signs(
    circuit_decoder, aliased
):
    from qdk.simulation import run_qir

    codec = qodec.Qodec.load(str(FIXTURES / "c4.qodec.yaml"))
    prepare = codec.layers[0].gadgets["prepare_zz"]
    if aliased:
        prepare.readouts = [{"reject": ["out[0].z[1]", 1]}]
        prepare.checks += [["out[0].z[0]", 1], ["readouts[0]"]]
    else:
        prepare.checks += [
            ["out[0].z[0]", "out[0].z[1]"],
            ["out[0].z[1]", 1],
        ]
    qir = """
        %Qubit = type opaque
        %Result = type opaque
        define void @main() #0 {
          call void @prepare_zz(%Qubit* null, %Result* null)
          call void @measure_zz(%Qubit* null, %Result* inttoptr (i64 1 to %Result*), %Result* inttoptr (i64 2 to %Result*))
          call void @__quantum__rt__array_record_output(i64 2, i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* null)
          ret void
        }
        declare void @prepare_zz(%Qubit*, %Result*)
        declare void @measure_zz(%Qubit*, %Result*, %Result*)
        declare void @__quantum__rt__array_record_output(i64, i8*)
        declare void @__quantum__rt__result_record_output(%Result*, i8*)
        attributes #0 = { "entry_point" "qir_profiles"="base_profile" "required_num_qubits"="1" "required_num_results"="3" }
    """
    assert (
        run_qir(qir, qodec=codec, decoder=circuit_decoder, shots=3, seed=42)
        == [[Result.One, Result.One]] * 3
    )


def test_circuit_faults_follow_measurement_and_reset_timing(circuit_decoder):
    from dataclasses import replace
    from qdk._native import QirInstructionId as Id

    batch = circuit_batch(circuit_decoder)
    trace = replace(
        batch.trace,
        instructions=(
            (Id.RESET, 0),
            (Id.MZ, 0, 0),
            (Id.MZ, 0, 1),
            (Id.RESET, 0),
            (Id.MZ, 0, 2),
        ),
    )
    noise = NoiseConfig()
    noise.mresetz.x = 0.1
    # Reset faults flip subsequent measurements; measurement faults never
    # flip the bit just recorded, and do not survive the next reset.
    assert list(physical_faults(trace, noise)) == [
        (0b011, 0.1),
        (0b010, 0.1),
        (0b100, 0.1),
    ]


@pytest.mark.parametrize("width", [1, 2])
def test_circuit_deq_handles_depolarizing_boundaries(circuit_decoder, width):
    from qdk.simulation._qodec.deq_circuit import _channel

    name = "x" if width == 1 else "cx"
    noise = NoiseConfig()
    table = getattr(noise, name)
    table.set_depolarizing((4**width - 1) / 4**width)
    assert all(probability == 0.5 for _, probability in _channel(noise, name, width))
    table.set_depolarizing(1e-20)
    assert all(probability > 0 for _, probability in _channel(noise, name, width))
    table.set_depolarizing(0.99)
    with pytest.raises(NotImplementedError, match="not approximated"):
        _channel(noise, name, width)


@pytest.mark.parametrize("field", ["loss", "x"])
def test_circuit_deq_rejects_changed_noise_before_sampling(circuit_decoder, field):
    noise = NoiseConfig()
    noise.mresetz.x = 0.01
    batch = circuit_batch(circuit_decoder, noise)
    setattr(noise.mresetz, field, 0.02)
    with pytest.raises(ValueError, match="prepare a new"):
        batch.run(1, noise, seed=42)


def test_circuit_deq_surfaces_constant_contradictions(circuit_decoder):
    from qdk.simulation._qodec.readout_equations import InconsistentParity

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    codec.layers[0].gadgets["prepare_z"].checks = [[1]]
    with pytest.raises(InconsistentParity):
        circuit_batch(circuit_decoder, codec=codec)


def test_circuit_deq_cannot_explain_impossible_evidence(circuit_decoder):
    from qdk.simulation._qodec.deq_decoding import _DeqTransport
    from qdk.simulation._qodec.protocols import ExecutionUnresolved
    from contextlib import closing

    batch = circuit_batch(circuit_decoder)
    library = batch.library
    rows = [[Result.One, Result.Zero, Result.Zero], [Result.Zero] * 3]
    with closing(_DeqTransport()) as transport:
        assert transport.run(batch._decode(library, rows, 1, "discard")) == [
            [Result.Zero]
        ]
        with pytest.raises(ExecutionUnresolved, match="explain"):
            transport.run(batch._decode(library, rows, 1, "raise"))


def test_circuit_deq_crosses_batch_boundaries_without_state_leaks(circuit_decoder):
    noise = NoiseConfig()
    noise.mresetz.x = 0.1
    batch = circuit_batch(circuit_decoder, noise)
    results = batch.run(513, noise, seed=42)
    assert len(results) == 513
    assert results == batch.run(513, noise, seed=42)


def test_circuit_deq_works_in_a_notebook_event_loop(circuit_decoder):
    import asyncio

    async def run():
        return circuit_batch(circuit_decoder).run(2, None, seed=42)

    assert asyncio.run(run()) == [[Result.Zero]] * 2


def test_circuit_deq_preserves_bit_order_across_bytes(circuit_decoder):
    program = compile_qasm(
        'include "stdgates.inc"; qubit[9] data; '
        "x data[0]; x data[3]; x data[8]; bit[9] result = measure data;"
    )
    batch = circuit_batch(circuit_decoder, program=program)
    expected = [Result.One if index in (0, 3, 8) else Result.Zero for index in range(9)]
    assert batch.run(3, None, seed=42) == [expected] * 3


def test_circuit_deq_preserves_reused_blocks_and_noiseless_discards(circuit_decoder):
    from .test_native_batch import _repetition_code_with_x_circuit

    codec = _repetition_code_with_x_circuit("X 0 1 2\nCX 3 0 3 1\nM 3")
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; x data; '
        "bit first = measure data; reset data; x data; bit last = measure data;"
    )
    noise = NoiseConfig()
    noise.mresetz.x = 1
    batch = circuit_batch(circuit_decoder, noise, codec, program)
    assert batch.run(4, noise, seed=42) == [[Result.Zero, Result.One]] * 4


def test_circuit_deq_accounts_for_deterministic_gate_faults(circuit_decoder):
    noise = NoiseConfig()
    noise.x.x = 1
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; bit r = measure data;'
    )
    batch = circuit_batch(circuit_decoder, noise, program=program)
    assert batch.run(4, noise, seed=42) == [[Result.One]] * 4


def test_circuit_deq_complements_a_fault_that_flips_a_port_stabilizer(circuit_decoder):
    from .test_native_batch import _repetition_code_with_x_circuit

    codec = _repetition_code_with_x_circuit("Y 0\nX 1 2")
    noise = NoiseConfig()
    noise.y.x = 1
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; bit r = measure data;'
    )
    batch = circuit_batch(circuit_decoder, noise, codec, program)
    assert batch.run(4, noise, seed=42, on_shot_failure="raise") == [[Result.One]] * 4


def test_circuit_deq_corrects_data_and_intermediate_syndrome_faults(circuit_decoder):
    from contextlib import closing
    from qodec.gadgets import Circuit
    from qdk.simulation._qodec.deq_decoding import _DeqTransport

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    gadget = codec.layers[0].gadgets["__quantum__qis__x__body"]
    gadget.circuit = Circuit(
        codec.layers[-1].instruction_set,
        "X 0 1 2\nR 3 4\nCX 0 3 1 3\nCX 1 4 2 4\nM 3 4",
        format="stim",
    )
    gadget.checks = codec.layers[0].gadgets["idle"].checks
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; bit r = measure data;'
    )
    noise = NoiseConfig()
    noise.mresetz.x = 0.01
    batch = circuit_batch(circuit_decoder, noise, codec, program)
    clean = [False, False, True, True, True]
    rows = [
        [
            Result.One if value ^ bool(fault >> index & 1) else Result.Zero
            for index, value in enumerate(clean)
        ]
        for fault, _ in physical_faults(batch.trace, noise)
    ]
    assert len(rows) == 5
    with closing(_DeqTransport()) as transport:
        assert transport.run(batch._decode(batch.library, rows, 42, "raise")) == [
            [Result.One]
        ] * len(rows)


@pytest.mark.parametrize("kind", ["reset_loss", "gate_loss", "rotation", "layers"])
def test_circuit_deq_rejects_unsupported_traces(circuit_decoder, kind):
    noise = NoiseConfig()
    source = 'include "stdgates.inc"; qubit data; x data; bit r = measure data;'
    codec = None
    if kind == "reset_loss":
        noise.mresetz.loss = 0.01
    elif kind == "gate_loss":
        noise.x.loss = 0.01
    elif kind == "rotation":
        source = source.replace("x data", "rz(0.1) data")
    else:
        from .test_execution_pipeline import nested_repetition_qodec

        source = 'include "stdgates.inc";'
        codec = nested_repetition_qodec()
    with pytest.raises(NotImplementedError, match="Circuit-level deq"):
        circuit_batch(circuit_decoder, noise, codec, compile_qasm(source))


@pytest.mark.parametrize("basis", ["zz", "xx"])
@pytest.mark.parametrize("flagged", [False, True])
def test_circuit_deq_c4_transport_syndromes_and_returned_flags(
    circuit_decoder, basis, flagged
):
    from qdk.simulation import run_qir

    codec = qodec.Qodec.load(str(FIXTURES / "c4.qodec.yaml"))
    if flagged:
        circuit = codec.layers[0].gadgets[f"prepare_{basis}"].circuit
        circuit.source = circuit.source.replace("M 4", "X 4\nM 4")
    flip = "x0" if basis == "zz" else "z0"
    qir = f"""
        %Qubit = type opaque
        %Result = type opaque
        define void @main() #0 {{
          call void @prepare_{basis}(%Qubit* null, %Result* null)
          call void @prepare_{basis}(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @{flip}(%Qubit* null)
          call void @transversal_cx(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @idle(%Qubit* null)
          call void @idle(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @measure_{basis}(%Qubit* null, %Result* inttoptr (i64 2 to %Result*), %Result* inttoptr (i64 3 to %Result*))
          call void @measure_{basis}(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 4 to %Result*), %Result* inttoptr (i64 5 to %Result*))
          call void @__quantum__rt__array_record_output(i64 6, i8* null)
          {" ".join(f"call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 {index} to %Result*), i8* null)" for index in range(6))}
          ret void
        }}
        declare void @prepare_{basis}(%Qubit*, %Result*)
        declare void @{flip}(%Qubit*)
        declare void @transversal_cx(%Qubit*, %Qubit*)
        declare void @idle(%Qubit*)
        declare void @measure_{basis}(%Qubit*, %Result*, %Result*)
        declare void @__quantum__rt__array_record_output(i64, i8*)
        declare void @__quantum__rt__result_record_output(%Result*, i8*)
        attributes #0 = {{ "entry_point" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="6" }}
    """
    flag = Result.One if flagged else Result.Zero
    expected = [
        flag,
        flag,
        Result.One,
        Result.Zero,
        Result.One if basis == "zz" else Result.Zero,
        Result.Zero,
    ]
    assert (
        run_qir(qir, qodec=codec, decoder=circuit_decoder, shots=20, seed=42)
        == [expected] * 20
    )


@pytest.mark.parametrize(
    "kind", ["identifier", "size", "bytes", "count", "execute", "failure"]
)
def test_circuit_deq_does_not_discard_runtime_protocol_errors(
    circuit_decoder, monkeypatch, kind
):
    from contextlib import asynccontextmanager, closing
    from types import SimpleNamespace
    from deq.proto import coordinator_pb2 as coordinator, util_pb2 as util
    from qdk.simulation._qodec import deq_circuit
    from qdk.simulation._qodec.deq_decoding import _DeqTransport

    batch = circuit_batch(circuit_decoder)
    closed = []

    async def load_library(library):
        pass

    async def batch_execute(instructions):
        return [
            99 if kind == "execute" else instruction.gadget.gid
            for instruction in instructions
        ]

    async def batch_decode(outcomes):
        if kind == "failure":
            raise RuntimeError("native decoder failure")
        if kind == "count":
            return []
        widths = {
            kind.base.gtype: len(kind.base.readouts)
            for kind in batch.library.gadget_types
        }
        return [
            coordinator.Readouts(
                gid=99 if kind == "identifier" else outcome.gid,
                readouts=util.BitVector(
                    size=99 if kind == "size" else widths[gadget.gtype],
                    data=(
                        b""
                        if kind == "bytes"
                        else bytes((widths[gadget.gtype] + 7) // 8)
                    ),
                ),
            )
            for outcome, gadget in zip(outcomes, batch.composites)
        ]

    @asynccontextmanager
    async def runtime(**kwargs):
        try:
            yield SimpleNamespace(
                jit_controller=SimpleNamespace(
                    load_library=load_library,
                    batch_execute=batch_execute,
                    batch_decode=batch_decode,
                )
            )
        finally:
            closed.append(True)

    monkeypatch.setattr(deq_circuit, "Runtime", runtime)
    with closing(_DeqTransport()) as transport:
        with pytest.raises(RuntimeError):
            transport.run(
                batch._decode(batch.library, [[Result.Zero] * 3], 42, "discard")
            )
    assert closed == [True]


def test_circuit_deq_empty_program_and_zero_shots(circuit_decoder):
    program = compile_qasm('include "stdgates.inc";')
    batch = circuit_batch(circuit_decoder, program=program)
    assert batch.run(0, None, seed=42) == []
    assert batch.run(3, None, seed=42) == [[], [], []]


def test_decoder_public_names_remain_unchanged():
    from qdk.simulation import decoders

    expected = {
        "BatchDecoderFactory",
        "BatchDecoderSession",
        "BatchUnsupported",
        "BlockReference",
        "Correction",
        "Corrections",
        "Decoded",
        "DecoderFactory",
        "DecoderSession",
        "ExecutionRejected",
        "ExecutionUnresolved",
        "Invocation",
        "LogicalSlot",
        "Operation",
        "PrepareDecoder",
        "ReadoutBatch",
        "ReadoutTable",
        "Readouts",
        "prepare_syndrome_decoder",
        "prepare_frame_decoder",
        "prepare_deq_decoder",
    }
    assert set(decoders.__all__) == expected
    assert {name for name in vars(decoders) if not name.startswith("_")} == expected
