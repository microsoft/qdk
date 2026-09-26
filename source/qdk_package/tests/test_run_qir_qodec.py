import sys
from types import ModuleType
from typing import TYPE_CHECKING, cast

import pytest
from qdk.simulation import NoiseConfig, run_qir

if TYPE_CHECKING:
    from qodec import Layer, Qodec


@pytest.mark.parametrize("simulator_type", [None, "clifford", "cpu"])
@pytest.mark.parametrize("custom_decoder", [False, True])
@pytest.mark.parametrize(
    "policy, max_retries", [("raise", 3), ("discard", 0), ("retry", 2)]
)
def test_qodec_selects_encoded_runner(
    monkeypatch, simulator_type, custom_decoder, policy, max_retries
):
    qodec = cast("Qodec", object())
    noise = NoiseConfig()
    expected = [True, False, True]

    def prepare_decoder(layer):
        pytest.fail("Decoder preparation belongs to the encoded runner")

    selected_decoder = prepare_decoder if custom_decoder else None

    def run_encoded(
        qir,
        selected_qodec,
        selected_noise,
        shots,
        seed,
        *,
        type,
        on_shot_failure,
        max_retries: int,
        decoder=None,
    ):
        assert (qir, selected_qodec, selected_noise, shots, seed, type) == (
            "qir",
            qodec,
            noise,
            3,
            42,
            simulator_type,
        )
        assert on_shot_failure == policy
        assert max_retries == expected_retries
        assert decoder is selected_decoder
        return expected

    expected_retries = max_retries
    runner = ModuleType("qdk.simulation._qodec._run")
    monkeypatch.setattr(runner, "run_qir_with_qodec", run_encoded, raising=False)
    monkeypatch.setitem(sys.modules, runner.__name__, runner)

    def unexpected_gpu_probe():
        pytest.fail("Encoded execution must not probe for a GPU")

    monkeypatch.setattr(
        "qdk.simulation._simulation.try_create_gpu_adapter", unexpected_gpu_probe
    )
    assert (
        run_qir(
            "qir",
            3,
            noise,
            42,
            simulator_type,
            qodec=qodec,
            decoder=selected_decoder,
            on_shot_failure=policy,
            max_retries=max_retries,
        )
        is expected
    )


@pytest.mark.parametrize("options", [{"on_shot_failure": "raise"}, {"max_retries": 0}])
def test_shot_failure_options_require_a_qodec(options):
    with pytest.raises(ValueError, match="require a Qodec"):
        run_qir("", **options)


def test_decoder_requires_a_qodec():
    def prepare_decoder(layer):
        pytest.fail("Decoder must not be prepared without a Qodec")

    with pytest.raises(ValueError, match="decoder requires a Qodec"):
        run_qir("", decoder=prepare_decoder)


@pytest.mark.parametrize("missing_dependency", ["deq", "deq_runtime"])
def test_deq_missing_dependency_has_install_instructions(
    monkeypatch, missing_dependency
):
    pytest.importorskip("qodec")
    from qdk.simulation.decoders import prepare_deq_decoder

    monkeypatch.delitem(
        sys.modules, "qdk.simulation._qodec.deq_decoding", raising=False
    )
    for name in tuple(sys.modules):
        if name == "deq" or name.startswith("deq."):
            monkeypatch.delitem(sys.modules, name)
    monkeypatch.setitem(sys.modules, missing_dependency, None)
    with pytest.raises(ImportError, match="pip install deq deq-runtime"):
        prepare_deq_decoder(cast("Layer", object()))


@pytest.mark.parametrize("failure", [None, "start", "execute"])
def test_pipeline_routes_readouts_and_closes_each_component(failure):
    pytest.importorskip("qodec")
    from qdk.simulation._qodec.execution_pipeline import ExecutionPipeline
    from qdk.simulation._qodec.protocols import Readouts, Requests, Resources
    from qdk.simulation._qodec.quantum_operations import Operation

    closed = []

    class Runtime:
        def required_resources(self, program: str) -> Resources:
            return Resources(qubits=1)

        def run(self, program: str) -> Requests[Readouts]:
            readouts = yield Operation("measure", (0,))
            assert readouts is not None
            return readouts

        def close(self):
            closed.append("runtime")

    class Layer:
        def required_resources(self, upper: Resources) -> Resources:
            return Resources(qubits=upper.qubits * 3)

        def handle(self, request) -> Requests[Readouts]:
            readouts = yield request
            assert readouts is not None
            return tuple(not bit for bit in readouts)

        def close(self):
            closed.append("layer")

    class Backend:
        def start(self, resources: Resources):
            assert resources == Resources(qubits=3)
            if failure == "start":
                raise RuntimeError("start")

        def execute(self, request: Operation) -> Readouts:
            assert request == Operation("measure", (0,))
            if failure == "execute":
                raise RuntimeError("execute")
            return (False,)

        def close(self):
            closed.append("backend")

    pipeline = ExecutionPipeline(Runtime(), [Layer()], Backend())
    if failure:
        with pytest.raises(RuntimeError, match=failure):
            pipeline.run("program")
    else:
        assert pipeline.run("program") == (True,)
    assert closed == ["runtime", "layer", "backend"]
    with pytest.raises(RuntimeError, match="closed"):
        pipeline.run("program")


@pytest.mark.parametrize("measurement", [False, True])
def test_adaptive_runtime_branches_on_returned_readouts(measurement):
    pytest.importorskip("qodec")
    from qdk import Result
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.bytecode import compile
    from qdk.simulation._qodec.execution_pipeline import ExecutionPipeline
    from qdk.simulation._simulation import preprocess_simulation_input
    from test_adaptive_cpu_quantum_ops import MEASURE_AND_CORRECT_QIR

    operations = []

    class Backend:
        def execute(self, request):
            operations.append(request.name)
            return (measurement,) if request.name == "measure" else ()

    module, _, _, _ = preprocess_simulation_input(MEASURE_AND_CORRECT_QIR)
    pipeline = ExecutionPipeline(AdaptiveRuntime(), [], Backend())
    assert pipeline.run(compile(module)) == [Result.One if measurement else Result.Zero]
    assert operations == ["prepare", "h", "measure", "prepare"] + (
        ["x"] if measurement else []
    )


@pytest.mark.parametrize("simulator_type", [None, "clifford", "cpu"])
@pytest.mark.parametrize("decoder_name", [None, "syndrome", "frame", "deq"])
def test_public_qodec_runner_matches_adaptive_physical_results(
    simulator_type, decoder_name
):
    qodec = pytest.importorskip("qodec")
    pytest.importorskip("stim")
    import qdk
    import qdk.openqasm
    from ec_tests.runtime import FIXTURES

    decoder = None
    if decoder_name is not None:
        from qdk.simulation import decoders

        if decoder_name == "deq":
            pytest.importorskip("deq")
            pytest.importorskip("deq_runtime")
        decoder = getattr(decoders, f"prepare_{decoder_name}_decoder")

    qir = qdk.openqasm.compile(
        """
        include "stdgates.inc";
        qubit target;
        x target;
        bit first = measure target;
        if (first) { x target; }
        bit second = measure target;
        """,
        target_profile=qdk.TargetProfile.Adaptive,
    )
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    expected = run_qir(qir, shots=3, seed=7, type="cpu")
    assert (
        run_qir(qir, shots=3, seed=7, type=simulator_type, qodec=codec, decoder=decoder)
        == expected
    )


@pytest.mark.xfail(
    raises=ValueError,
    strict=True,
    reason="build_qodec does not yet name its instructions after QIR gates",
)
@pytest.mark.parametrize("complete", [False, True])
def test_generated_qodec_runs_without_serialization(complete):
    qodec = pytest.importorskip("qodec")
    pytest.importorskip("stim")
    ec = pytest.importorskip("qdk.ec")
    from qdk import Result, TargetProfile, qsharp

    code = qodec.Code(
        "repetition3",
        stabilizers=["Z_0 Z_1", "Z_1 Z_2"],
        x=["X_0 X_1 X_2"],
        z=["Z_0"],
    )
    codec = ec.build_qodec(code, strategy="bare-css/v1", strict=False)
    if complete:
        codec = ec.filled(codec)
    qsharp.init(target_profile=TargetProfile.Base)
    qir = qsharp.compile("{ use q = Qubit(); M(q) }")

    assert (
        run_qir(qir, shots=3, seed=7, qodec=codec, on_shot_failure="raise")
        == [Result.Zero] * 3
    )


@pytest.mark.parametrize(
    "program, callee",
    [
        ("H(q); MResetZ(q)", "__quantum__qis__h__body"),
        ("S(q); MResetZ(q)", "__quantum__qis__s__body"),
        ("Reset(q)", "__quantum__qis__reset__body"),
    ],
)
def test_quantum_calls_require_an_instruction_of_the_same_name(program, callee):
    qodec = pytest.importorskip("qodec")
    from ec_tests.runtime import FIXTURES
    from qdk import TargetProfile, qsharp
    from qdk.simulation._qodec.bytecode import UnknownInstruction

    qsharp.init(target_profile=TargetProfile.Adaptive)
    qir = qsharp.compile(f"{{ use q = Qubit(); {program} }}")
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))

    with pytest.raises(
        UnknownInstruction,
        match=f"QIR call '{callee}' requires an instruction of that name",
    ):
        run_qir(qir, qodec=codec)


_REPETITION3_X = (
    "x.gadget.yaml:\n  circuit:\n    source: [{X: [0]}, {X: [1]}, {X: [2]}]\n"
)

_REPETITION3_FRAME_X = (
    "x.gadget.yaml:\n"
    "  circuit: {source: []}\n"
    "  checks:\n"
    '    - ["in[0].stabilizers[0]", "out[0].stabilizers[0]"]\n'
    '    - ["in[0].stabilizers[1]", "out[0].stabilizers[1]"]\n'
    "  frames:\n"
    "    out[0].z[0]: [1]\n"
)


def test_program_intrinsics_invoke_instructions_by_name(tmp_path):
    pytest.importorskip("qodec")
    from qdk import Result, TargetProfile, qsharp

    # ``flip`` is the fixture's logical X under a name only a Q# intrinsic uses.
    codec = _repetition3_variant(
        tmp_path,
        ("    - mnemonic: __quantum__qis__x__body\n", "    - mnemonic: flip\n"),
        (
            "        __quantum__qis__x__body: x.gadget.yaml\n",
            "        flip: x.gadget.yaml\n",
        ),
    )
    qsharp.init(target_profile=TargetProfile.Adaptive)
    qsharp.eval("operation flip(q : Qubit) : Unit { body intrinsic; }")
    qir = qsharp.compile(
        "{ use q = Qubit(); flip(q); let r = M(q); flip(q); [r, M(q)] }"
    )

    assert (
        run_qir(qir, shots=3, seed=7, qodec=codec, on_shot_failure="raise")
        == [[Result.One, Result.Zero]] * 3
    )


def test_measurement_intrinsics_return_instruction_outcomes(tmp_path):
    pytest.importorskip("qodec")
    from qdk import Result, TargetProfile, qsharp

    codec = _repetition3_variant(
        tmp_path,
        (
            "    - mnemonic: __quantum__qis__m__body\n",
            "    - mnemonic: MyCustomMeasurement\n",
        ),
        (
            "        __quantum__qis__m__body: m.gadget.yaml\n",
            "        MyCustomMeasurement: m.gadget.yaml\n",
        ),
    )
    qsharp.init(target_profile=TargetProfile.Adaptive)
    qsharp.eval("""
        @Measurement()
        operation MyCustomMeasurement(q : Qubit) : Result { body intrinsic; }
    """)
    qir = qsharp.compile("""{
        use (a, b) = (Qubit(), Qubit());
        X(a);
        [MyCustomMeasurement(a), MyCustomMeasurement(b)]
    }""")

    assert (
        run_qir(qir, shots=3, seed=7, qodec=codec, on_shot_failure="raise")
        == [[Result.One, Result.Zero]] * 3
    )


def test_program_intrinsic_arguments_bind_operands_and_parameters():
    qodec = pytest.importorskip("qodec")
    from ec_tests.runtime import FIXTURES
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.bytecode import compile
    from qdk.simulation._simulation import preprocess_simulation_input
    from qodec.instructions import InstructionCall

    module, _, _, _ = preprocess_simulation_input("""
        %Qubit = type opaque
        define i64 @main() #0 {
          call void @turn(double 0.5, %Qubit* inttoptr (i64 1 to %Qubit*))
          ret i64 0
        }
        declare void @turn(double, %Qubit*)
        attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
    """)
    isa = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    declarations = dict(isa.instruction_set.instructions)
    rotation = declarations["__quantum__qis__rz__body"]
    declarations["turn"] = qodec.Instruction(
        "turn",
        inputs=rotation.inputs,
        outputs=rotation.outputs,
        parameters=rotation.parameters,
        action=rotation.action,
    )
    requests = AdaptiveRuntime(initialize=False).run(compile(module, declarations))

    assert next(requests) == InstructionCall(
        "turn", operands=[1], arguments={"theta": 0.5}
    )


@pytest.mark.parametrize(
    "callee, signature, arguments, message",
    [
        (
            "__quantum__qis__rz__body",
            "%Qubit*, %Qubit*",
            "%Qubit* null, %Qubit* null",
            "takes 1 block",
        ),
        ("__quantum__qis__rz__body", "%Qubit*", "%Qubit* null", "takes 1 parameters"),
        ("__quantum__qis__rz__body", "i64, %Qubit*", "i64 1, %Qubit* null", None),
        (
            "__quantum__qis__rz__body",
            "i1, %Qubit*",
            "i1 true, %Qubit* null",
            "expects number, but .* bool",
        ),
        (
            "__quantum__qis__m__body",
            "%Qubit*",
            "%Qubit* null",
            "takes 1 block operands and reports 1",
        ),
        ("__quantum__qis__m__body", "", "", "reports 1 outcomes, but .* only 0"),
    ],
)
def test_program_intrinsics_must_match_the_instruction_signature(
    callee, signature, arguments, message
):
    qodec = pytest.importorskip("qodec")
    from ec_tests.runtime import FIXTURES
    from qdk.simulation._qodec.bytecode import compile
    from qdk.simulation._simulation import preprocess_simulation_input

    module, _, _, _ = preprocess_simulation_input(f"""
        %Qubit = type opaque
        define i64 @main() #0 {{
          call void @{callee}({arguments})
          ret i64 0
        }}
        declare void @{callee}({signature})
        attributes #0 = {{ "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }}
    """)
    declarations = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[0]
        .instruction_set.instructions
    )
    if message is None:
        assert compile(module, declarations).instruction_calls[0].mnemonic == callee
    else:
        with pytest.raises((TypeError, ValueError), match=message):
            compile(module, declarations)


def test_single_qubits_run_on_blocks_that_encode_two_logical_qubits():
    qodec = pytest.importorskip("qodec")
    pytest.importorskip("stim")
    from ec_tests.runtime import FIXTURES
    from qdk import Result, TargetProfile, qsharp

    qsharp.init(target_profile=TargetProfile.Adaptive)
    qir = qsharp.compile("""{
            use (a, b) = (Qubit(), Qubit());
            X(a);
            let first = [MResetZ(a), MResetZ(b)];
            X(b);
            first + [MResetZ(a), MResetZ(b)]
        }""")

    assert (
        run_qir(
            qir,
            shots=3,
            seed=7,
            type="clifford",
            qodec=qodec.Qodec.load(str(FIXTURES / "c4.qodec.yaml")),
            on_shot_failure="raise",
        )
        == [[Result.One, Result.Zero, Result.Zero, Result.One]] * 3
    )


def test_raised_preparation_flags_follow_the_shot_failure_policy():
    qodec = pytest.importorskip("qodec")
    pytest.importorskip("stim")
    from ec_tests.runtime import FIXTURES
    from qdk import TargetProfile, qsharp
    from qdk.simulation.decoders import ExecutionRejected

    qsharp.init(target_profile=TargetProfile.Adaptive)
    qir = qsharp.compile("{ use q = Qubit(); MResetZ(q) }")
    noise = NoiseConfig()
    noise.cx.set_depolarizing(0.2)
    codec = qodec.Qodec.load(str(FIXTURES / "c4.qodec.yaml"))

    def run(policy):
        return run_qir(
            qir,
            shots=40,
            seed=7,
            type="clifford",
            noise=noise,
            qodec=codec,
            on_shot_failure=policy,
            max_retries=50,
        )

    with pytest.raises(ExecutionRejected):
        run("raise")
    assert len(run("discard")) < 40
    assert len(run("retry")) == 40


_C4_BLOCK_PROGRAM = """
    define void @main() #0 {
      call void @prepare_zz(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
      call void @measure_zz(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
      call void @__quantum__rt__array_record_output(i64 3, ptr null)
      call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
      call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
      call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr null)
      ret void
    }
    declare void @prepare_zz(ptr, ptr)
    declare void @measure_zz(ptr, ptr, ptr) #1
    declare void @__quantum__rt__array_record_output(i64, ptr)
    declare void @__quantum__rt__result_record_output(ptr, ptr)
    attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="3" }
    attributes #1 = { "irreversible" }
"""


@pytest.mark.parametrize("options", [{}, {"on_shot_failure": "retry"}])
def test_calls_with_a_result_per_flag_return_raised_flags(options):
    qodec = pytest.importorskip("qodec")
    pytest.importorskip("stim")
    from ec_tests.runtime import FIXTURES
    from qdk import Result

    noise = NoiseConfig()
    noise.cx.set_depolarizing(0.2)
    codec = qodec.Qodec.load(str(FIXTURES / "c4.qodec.yaml"))

    results = run_qir(
        _C4_BLOCK_PROGRAM,
        shots=40,
        seed=7,
        type="clifford",
        noise=noise,
        qodec=codec,
        **options,
    )

    # A returned flag is the program's to act on, so no shot is rejected.
    assert len(results) == 40
    assert {result[0] for result in results} == {Result.Zero, Result.One}


def test_block_creating_calls_replace_implicit_preparation():
    qodec = pytest.importorskip("qodec")
    pytest.importorskip("stim")
    from ec_tests.runtime import FIXTURES
    from qdk import Result
    from qdk.simulation.decoders import prepare_syndrome_decoder

    invoked = []

    def prepare_decoder(layer):
        create = prepare_syndrome_decoder(layer)

        def session(seed):
            inner = create(seed)

            class Recorder:
                def decode(self, invocation, readouts):
                    invoked.append(invocation.gadget.implements.mnemonic)
                    return (yield from inner.decode(invocation, readouts))

                def close(self):
                    inner.close()

            return Recorder()

        return session

    codec = qodec.Qodec.load(str(FIXTURES / "c4.qodec.yaml"))

    assert run_qir(
        _C4_BLOCK_PROGRAM,
        seed=7,
        type="clifford",
        qodec=codec,
        decoder=prepare_decoder,
        on_shot_failure="retry",
    ) == [[Result.Zero] * 3]
    assert invoked == ["prepare_zz", "measure_zz"]


def _repetition3_variant(tmp_path, *replacements):
    import qodec
    from ec_tests.runtime import FIXTURES

    source = (FIXTURES / "repetition3.qodec.yaml").read_text(encoding="utf-8")
    for old, new in replacements:
        assert old in source
        source = source.replace(old, new)
    path = tmp_path / "variant.qodec.yaml"
    path.write_text(source, encoding="utf-8")
    return qodec.Qodec.load(str(path))


_REPETITION3_PREPARE = (
    'prepare_z.gadget.yaml:\n  circuit: {format: stim, source: "R 0 1 2"}\n'
)


def test_inconsistent_syndromes_on_dependent_stabilizers_fail_the_shot(tmp_path):
    pytest.importorskip("qodec")
    from qdk import TargetProfile, qsharp
    from qdk.simulation._qodec.readout_equations import InconsistentParity

    # Z_0 Z_2 is dependent on the other stabilizers; the preparation reads it
    # from an ancilla forced to One, a syndrome that no Pauli can produce.
    codec = _repetition3_variant(
        tmp_path,
        ("[Z_0 Z_1, Z_1 Z_2]", "[Z_0 Z_1, Z_1 Z_2, Z_0 Z_2]"),
        (
            _REPETITION3_PREPARE,
            "prepare_z.gadget.yaml:\n"
            '  circuit: {format: stim, source: "R 0 1 2 3\\nX 3\\nM 3"}\n'
            "  checks:\n"
            '    - ["out[0].stabilizers[0]"]\n'
            '    - ["out[0].stabilizers[1]"]\n'
            '    - ["circuit.readouts[0]", "out[0].stabilizers[2]"]\n',
        ),
        (
            '    - ["circuit.readouts[1]", "circuit.readouts[2]", "in[0].stabilizers[1]"]\n',
            '    - ["circuit.readouts[1]", "circuit.readouts[2]", "in[0].stabilizers[1]"]\n'
            '    - ["circuit.readouts[0]", "circuit.readouts[2]", "in[0].stabilizers[2]"]\n',
        ),
    )
    qsharp.init(target_profile=TargetProfile.Base)
    qir = qsharp.compile("{ use q = Qubit(); M(q) }")

    assert run_qir(qir, shots=3, seed=7, qodec=codec) == []
    with pytest.raises(InconsistentParity, match="matches syndrome"):
        run_qir(qir, shots=3, seed=7, qodec=codec, on_shot_failure="raise")


@pytest.mark.parametrize("fault", ["", "X 0\\n", "X 1\\n", "X 2\\n"])
def test_preparation_syndromes_correct_faults_on_unframed_logicals(tmp_path, fault):
    pytest.importorskip("qodec")
    from qdk import Result, TargetProfile, qsharp

    # The preparation measures its stabilizers after a possible data fault and
    # declares no logical frame, so the minimum-weight correction must stand.
    codec = _repetition3_variant(
        tmp_path,
        (
            _REPETITION3_PREPARE,
            "prepare_z.gadget.yaml:\n"
            '  circuit: {format: stim, source: "R 0 1 2 3 4\\n'
            f'{fault}CX 0 3 1 3\\nCX 1 4 2 4\\nM 3 4"}}\n'
            "  checks:\n"
            '    - ["circuit.readouts[0]", "out[0].stabilizers[0]"]\n'
            '    - ["circuit.readouts[1]", "out[0].stabilizers[1]"]\n',
        ),
    )
    qsharp.init(target_profile=TargetProfile.Base)
    qir = qsharp.compile("{ use q = Qubit(); M(q) }")

    assert (
        run_qir(qir, shots=5, seed=7, qodec=codec, on_shot_failure="raise")
        == [Result.Zero] * 5
    )


# "retry" always uses the interpreter; "raise" lets Clifford runs batch natively.
_EXECUTION_PATHS = [
    pytest.param({"on_shot_failure": "raise"}, id="batch"),
    pytest.param({"on_shot_failure": "retry"}, id="interpreter"),
    pytest.param({"on_shot_failure": "retry", "type": "cpu"}, id="state-vector"),
]


@pytest.mark.parametrize("options", _EXECUTION_PATHS)
def test_decoder_corrections_are_noiseless_frame_updates(tmp_path, options):
    pytest.importorskip("qodec")
    from qdk import Result, TargetProfile, qsharp

    # The preparation leaves a Y fault on qubit 0 that the decoder corrects with
    # X_0. Every physical X gate loses its qubit, so the correction would lose
    # data if it ran as a gate.
    codec = _repetition3_variant(
        tmp_path,
        (
            _REPETITION3_PREPARE,
            "prepare_z.gadget.yaml:\n"
            '  circuit: {format: stim, source: "R 0 1 2 3 4\\n'
            'Y 0\\nCX 0 3 1 3\\nCX 1 4 2 4\\nM 3 4"}\n'
            "  checks:\n"
            '    - ["circuit.readouts[0]", "out[0].stabilizers[0]"]\n'
            '    - ["circuit.readouts[1]", "out[0].stabilizers[1]"]\n',
        ),
    )
    noise = NoiseConfig()
    noise.x.loss = 1
    qsharp.init(target_profile=TargetProfile.Base)
    qir = qsharp.compile("{ use q = Qubit(); M(q) }")

    assert (
        run_qir(qir, shots=5, seed=7, noise=noise, qodec=codec, **options)
        == [Result.Zero] * 5
    )


@pytest.mark.parametrize("options", _EXECUTION_PATHS)
def test_frame_gadgets_apply_logical_paulis_noiselessly(tmp_path, options):
    pytest.importorskip("qodec")
    from qdk import Result, TargetProfile, qsharp

    # Every physical X gate loses its qubit, so only a frame update keeps the data.
    codec = _repetition3_variant(tmp_path, (_REPETITION3_X, _REPETITION3_FRAME_X))
    noise = NoiseConfig()
    noise.x.loss = 1
    qsharp.init(target_profile=TargetProfile.Base)
    qir = qsharp.compile("{ use q = Qubit(); X(q); M(q) }")

    assert (
        run_qir(qir, shots=5, seed=7, noise=noise, qodec=codec, **options)
        == [Result.One] * 5
    )


def test_custom_decoder_changes_public_results_and_closes_each_shot():
    qodec = pytest.importorskip("qodec")
    pytest.importorskip("stim")
    import qdk
    import qdk.openqasm
    from ec_tests.runtime import FIXTURES
    from qdk.simulation.decoders import Decoded, prepare_syndrome_decoder

    prepared = []
    seeds = []
    closed = []

    def prepare_decoder(layer):
        prepared.append(layer)
        create = prepare_syndrome_decoder(layer)

        class Decoder:
            def __init__(self, seed):
                seeds.append(seed)
                self.inner = create(seed)

            def decode(self, invocation, readouts):
                decoded = yield from self.inner.decode(invocation, readouts)
                return Decoded(
                    tuple(None if bit is None else not bit for bit in decoded.outcomes),
                    decoded.flags,
                )

            def close(self):
                self.inner.close()
                closed.append(self)

        return Decoder

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit target; bit result = measure target;',
        target_profile=qdk.TargetProfile.Adaptive,
    )
    assert (
        run_qir(qir, shots=3, seed=7, qodec=codec, decoder=prepare_decoder)
        == [qdk.Result.One] * 3
    )
    assert prepared == [codec.layers[0]]
    assert len(set(seeds)) == 3
    assert len({id(session) for session in closed}) == 3


def test_deq_decoder_works_inside_an_asyncio_loop():
    import asyncio

    qodec = pytest.importorskip("qodec")
    pytest.importorskip("deq")
    pytest.importorskip("deq_runtime")
    import qdk
    import qdk.openqasm
    from ec_tests.runtime import FIXTURES
    from qdk.simulation.decoders import prepare_deq_decoder

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit target; x target; bit result = measure target;',
        target_profile=qdk.TargetProfile.Adaptive,
    )

    async def simulate():
        return run_qir(qir, shots=2, seed=7, qodec=codec, decoder=prepare_deq_decoder)

    assert asyncio.run(simulate()) == [qdk.Result.One] * 2


def test_qir_return_value_does_not_replace_recorded_outputs():
    qodec = pytest.importorskip("qodec")
    pytest.importorskip("stim")
    from ec_tests.runtime import FIXTURES

    qir = """
    define i64 @main() #0 {
    entry:
      call void @__quantum__rt__int_record_output(i64 42, i8* null)
      ret i64 1
    }
    declare void @__quantum__rt__int_record_output(i64, i8*)
    attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    """
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    assert run_qir(qir, shots=2, type="cpu") == [42, 42]
    assert run_qir(qir, shots=2, type="cpu", qodec=codec) == [42, 42]


def test_qodec_gpu_selection_is_explicitly_unsupported():
    qodec = pytest.importorskip("qodec")
    from ec_tests.runtime import FIXTURES

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    with pytest.raises(NotImplementedError, match="GPU"):
        run_qir("", qodec=codec, type="gpu")


def test_physical_execution_does_not_import_ec_dependencies():
    import subprocess
    from textwrap import dedent

    code = dedent('''
        import sys
        for name in ("qodec", "paulimer", "numpy", "scipy", "stim", "deq", "deq_runtime"):
            sys.modules[name] = None
        from qdk.simulation import run_qir
        qir = """
        define void @main() #0 { ret void }
        attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
        """
        assert run_qir(qir, type="cpu") == [""]
        assert "qdk.simulation._qodec" not in sys.modules
    ''')
    result = subprocess.run(
        [sys.executable, "-c", code], capture_output=True, text=True, timeout=30
    )
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("missing_dependency", ["deq", "deq_runtime"])
def test_deq_dependency_is_only_required_when_selected(missing_dependency):
    import subprocess
    from textwrap import dedent

    pytest.importorskip("qodec")
    from ec_tests.runtime import FIXTURES

    code = dedent('''
        import sys
        sys.modules[sys.argv[1]] = None
        from qodec import Qodec
        from qdk.simulation import run_qir
        from qdk.simulation.decoders import prepare_deq_decoder, prepare_syndrome_decoder
        codec = Qodec.load(sys.argv[2])
        qir = """
        define void @main() #0 { ret void }
        attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
        """
        assert run_qir(qir, qodec=codec, decoder=prepare_syndrome_decoder) == [""]
        assert "qdk.simulation._qodec.deq_decoding" not in sys.modules
        try:
            run_qir(qir, qodec=codec, decoder=prepare_deq_decoder)
        except ImportError as error:
            assert 'pip install deq deq-runtime' in str(error), str(error)
        else:
            raise AssertionError("Missing deq dependency was not reported")
    ''')
    result = subprocess.run(
        [
            sys.executable,
            "-c",
            code,
            missing_dependency,
            str(FIXTURES / "repetition3.qodec.yaml"),
        ],
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("policy", ["raise", "discard", "retry"])
@pytest.mark.parametrize(
    "failure_type", ["ExecutionRejected", "ExecutionUnresolved", "InconsistentParity"]
)
def test_shot_failure_policy_preserves_order_and_attempt_count(policy, failure_type):
    pytest.importorskip("qodec")
    from qdk.simulation._qodec import _run

    failure = getattr(_run, failure_type)("failed shot")
    outcomes = iter(["first", failure, "second", "third"])
    attempts = []

    class Executor:
        def set_seed(self, _seed):
            pass

        def run(self, program):
            attempts.append(program)
            outcome = next(outcomes)
            if isinstance(outcome, Exception):
                raise outcome
            return outcome

    program = object()
    if policy == "raise":
        with pytest.raises(type(failure)) as raised:
            _run.run_qir_raw_records(program, Executor(), 3, on_shot_failure=policy)
        assert raised.value is failure
        assert len(attempts) == 2
    else:
        records = _run.run_qir_raw_records(
            program, Executor(), 3, on_shot_failure=policy
        )
        assert records == (
            ["first", "second"] if policy == "discard" else ["first", "second", "third"]
        )
        assert len(attempts) == (3 if policy == "discard" else 4)


@pytest.mark.parametrize("policy", ["raise", "discard", "retry"])
@pytest.mark.parametrize(
    "error_type", [ValueError, TypeError, RuntimeError, NotImplementedError]
)
def test_shot_failure_policy_does_not_catch_execution_errors(policy, error_type):
    pytest.importorskip("qodec")
    from qdk.simulation._qodec._run import run_qir_raw_records

    failure = error_type("backend failed")

    class Executor:
        def run(self, program):
            raise failure

    program = object()
    with pytest.raises(error_type) as raised:
        run_qir_raw_records(program, Executor(), 1, on_shot_failure=policy)
    assert raised.value is failure
