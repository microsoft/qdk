# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""``tensornetwork_qir`` and ``check_tensornetwork_setup``: the arguments they
take, the rows and report a run returns, its seeds, its warning, the error
each cause raises, and that a run leaves the GIL and stops on Ctrl-C.

The tests of what a run returns and raises hold on every engine, so they
run on each (the ``engine`` fixture): the CPU engine everywhere, the GPU
engine where the setup check is ready. The GIL and Ctrl-C tests pin how the
binding runs any engine off the Python thread, so they run on the CPU
engine alone.
"""

import _thread
import copy
import pickle
import platform
import threading
import time
import warnings

import pytest

import qdk
import qdk.openqasm
from qdk import Result
from qdk.simulation import (
    MpsOptions,
    Sample,
    TensorNetworkSetup,
    check_tensornetwork_setup,
    tensornetwork_qir,
)
from qdk.simulation import _tensor_network
from qdk import _native


gpu = pytest.mark.skipif(
    not check_tensornetwork_setup().ready, reason="the GPU engine is not set up"
)


@pytest.fixture(params=["cpu", pytest.param("gpu", marks=gpu)])
def engine(request):
    """Each engine a user can pick: the CPU engine, and the GPU engine where
    the setup check is ready."""
    return request.param


def run(qir, queries, engine="cpu", **kwargs):
    """``tensornetwork_qir`` on the engine named ``engine``, with seed 7
    unless ``seed`` is given."""
    kwargs.setdefault("seed", 7)
    return tensornetwork_qir(qir, queries, engine=engine, **kwargs)


def qasm(source: str, profile=qdk.TargetProfile.Base) -> str:
    """QIR of the OpenQASM `source`, its outputs recorded as OpenQASM does."""
    return qdk.openqasm.compile(
        source,
        output_semantics=qdk.openqasm.OutputSemantics.OpenQasm,
        target_profile=profile,
    )


def bits(row) -> str:
    """A shot's results as a string of 0s and 1s, in recording order."""
    if isinstance(row, (list, tuple)):
        return "".join(bits(value) for value in row)
    return {Result.Zero: "0", Result.One: "1"}[row]


BELL = qasm("""
    OPENQASM 3.0;
    include "stdgates.inc";
    qubit[2] q;
    bit[2] c;
    h q[0];
    cx q[0], q[1];
    c = measure q;
    """)


def ghz(qubits: int) -> str:
    gates = "\n".join(f"cx q[{k - 1}], q[{k}];" for k in range(1, qubits))
    return qasm(f"""
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[{qubits}] q;
        bit[{qubits}] c;
        h q[0];
        {gates}
        c = measure q;
        """)


def coins(qubits: int) -> str:
    """`qubits` coins, entangled pairwise by `cz`, all read at the end."""
    gates = "\n".join(f"h q[{k}];" for k in range(qubits))
    gates += "\n".join(f"cz q[{k - 1}], q[{k}];" for k in range(1, qubits))
    return qasm(f"""
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[{qubits}] q;
        bit[{qubits}] c;
        {gates}
        c = measure q;
        """)


# ---------------------------------------------------------------------------
# Arguments
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("shots", [0, -1, 2**32, True, 1.5])
def test_sample_refuses_a_count_out_of_range(shots):
    with pytest.raises(ValueError, match="shots"):
        Sample(shots)


@pytest.mark.parametrize("bond", [0, 2**32, True, 2.0])
def test_mps_options_refuse_a_max_bond_dimension_out_of_range(bond):
    with pytest.raises(ValueError, match="max_bond_dimension"):
        MpsOptions(max_bond_dimension=bond)


def test_the_default_max_bond_dimension_is_128():
    assert MpsOptions().max_bond_dimension == 128


def test_an_unknown_method_is_refused():
    with pytest.raises(ValueError, match="method"):
        run(BELL, [Sample(1)], method="contraction")


def test_options_of_another_type_are_refused():
    with pytest.raises(TypeError, match="MpsOptions"):
        run(BELL, [Sample(1)], options={"max_bond_dimension": 4})


@pytest.mark.parametrize("seed", [-1, 2**32, True, 1.5])
def test_a_seed_out_of_range_is_refused(seed):
    with pytest.raises(ValueError, match="seed"):
        run(BELL, [Sample(1)], seed=seed)


@pytest.mark.parametrize("queries", [[], [Sample(1), Sample(2)]])
def test_queries_other_than_one_sample_are_refused(queries):
    with pytest.raises(ValueError, match="exactly one Sample"):
        run(BELL, queries)


def test_a_query_other_than_sample_is_refused():
    with pytest.raises(TypeError, match="Sample"):
        run(BELL, ["Expectation"])


def test_an_unknown_engine_is_refused():
    """An engine name other than ``"gpu"`` and ``"cpu"``, here the library's
    name, is a `ValueError` naming the engine."""
    with pytest.raises(ValueError, match="Invalid engine: 'cutensornet'"):
        run(BELL, [Sample(1)], engine="cutensornet")


# ---------------------------------------------------------------------------
# Rows and report
# ---------------------------------------------------------------------------


def test_each_row_is_a_shots_results_in_recording_order(engine):
    """A Bell pair's 32 shots: one row each, a list of the two results in
    the order the program records them, the two agreeing, both values
    occurring."""
    (rows,) = run(BELL, [Sample(32)], engine=engine)
    assert len(rows) == 32
    zeros, ones = [Result.Zero, Result.Zero], [Result.One, Result.One]
    assert all(row in (zeros, ones) for row in rows)
    assert zeros in rows and ones in rows


def test_an_adaptive_program_steers_its_shots_later_gates(engine):
    """A coin whose value a branch copies to a second qubit, in every shot,
    both values occurring."""
    qir = qasm(
        """
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[2] q;
        bit[2] c;
        h q[0];
        c[0] = measure q[0];
        if (c[0]) x q[1];
        c[1] = measure q[1];
        """,
        profile=qdk.TargetProfile.Adaptive_RIF,
    )
    (rows,) = run(qir, [Sample(32)], engine=engine)
    assert {bits(row) for row in rows} == {"00", "11"}


def test_the_report_comes_with_the_results(engine):
    """A GHZ state's largest bond is 2; a run whose bonds all fit discards
    nothing; the report gives the max bond dimension the run used."""
    results = run(
        ghz(3), [Sample(4)], engine=engine, options=MpsOptions(max_bond_dimension=16)
    )
    (rows,) = results
    assert len(rows) == 4
    report = results.report
    assert report.largest_bond == 2
    assert report.discarded_weight == pytest.approx(0.0, abs=1e-9)
    assert report.max_bond_dimension == 16


def test_a_seed_reproduces_each_shots_rows_whatever_the_count(engine):
    """Exactly on the CPU engine; on the GPU engine to rounding, which
    almost never decides a draw."""
    (five,) = run(coins(4), [Sample(5)], engine=engine, seed=11)
    (ten,) = run(coins(4), [Sample(10)], engine=engine, seed=11)
    assert five == ten[:5]


def test_another_seed_gives_other_rows(engine):
    (one,) = run(coins(8), [Sample(16)], engine=engine, seed=1)
    (other,) = run(coins(8), [Sample(16)], engine=engine, seed=2)
    assert one != other


def test_a_run_that_lost_weight_warns(engine):
    """A Bell pair at max bond dimension 1: the bond the `cx` makes is cut
    to 1, losing about half the state's weight, so the run warns with
    `RuntimeWarning`, pointing at the caller's line, and its report gives
    the loss."""
    with pytest.warns(RuntimeWarning, match="lost up to") as warned:
        results = tensornetwork_qir(
            BELL,
            [Sample(2)],
            engine=engine,
            seed=7,
            options=MpsOptions(max_bond_dimension=1),
        )
    assert warned[0].filename == __file__
    assert results.report.discarded_weight > 1e-3
    assert results.report.largest_bond == 1


@pytest.mark.parametrize(
    "duplicate",
    [
        pytest.param(lambda results: pickle.loads(pickle.dumps(results)), id="pickle"),
        pytest.param(copy.deepcopy, id="deepcopy"),
    ],
)
def test_results_survive_pickling_and_copying(duplicate):
    """A user saves an hours-long run's results: the copy keeps the rows and
    the report."""
    results = run(ghz(3), [Sample(4)])
    saved = duplicate(results)
    assert type(saved) is type(results)
    assert tuple(saved) == tuple(results)
    assert saved.report == results.report


def test_an_exact_run_does_not_warn(engine):
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        run(ghz(3), [Sample(2)], engine=engine)


# ---------------------------------------------------------------------------
# Errors
# ---------------------------------------------------------------------------

_QIR_DECLS = """\
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__qis__read_result__body(%Result*)
declare void @__quantum__qis__peek_loss__body(%Qubit*, %Result*)
declare void @__quantum__rt__readout_noise(double, double, %Result*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)
declare void @__quantum__rt__int_record_output(i64, i8*)
declare void @bit_flip_noise(%Qubit*) #2
"""


def adaptive_qir(body: str, qubits: int, results: int) -> str:
    return f"""\
%Result = type opaque
%Qubit = type opaque

define i64 @ENTRYPOINT__main() #0 {{
entry:
{body}
  ret i64 0
}}

{_QIR_DECLS}
attributes #0 = {{ "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="{qubits}" "required_num_results"="{results}" }}
attributes #1 = {{ "irreversible" }}
attributes #2 = {{ "qdk_noise" }}
"""


def test_a_trap_raises_runtime_error_naming_the_shot(engine):
    """1 divided by a qubit's readout, which is 0: every shot traps, and the
    first is named."""
    qir = adaptive_qir(
        """\
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  %bit = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  %value = zext i1 %bit to i64
  %quotient = sdiv i64 1, %value
  call void @__quantum__rt__int_record_output(i64 %quotient, i8* null)""",
        qubits=1,
        results=1,
    )
    with pytest.raises(RuntimeError, match="shot 0 trapped"):
        run(qir, [Sample(3)], engine=engine)


_MEASURE_AND_RECORD = """
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)"""


@pytest.mark.parametrize(
    "call, message",
    [
        pytest.param(
            "call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))",
            "twice",
            id="a gate on one qubit twice",
        ),
        pytest.param(
            "call void @__quantum__rt__readout_noise(double 0.1, double 0.1, %Result* inttoptr (i64 0 to %Result*))",
            "not run on matrix product states",
            id="readout noise",
        ),
        pytest.param(
            "call void @__quantum__qis__peek_loss__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))",
            "not run on matrix product states",
            id="a loss check",
        ),
    ],
)
def test_a_call_the_method_cannot_run_raises_value_error(engine, call, message):
    """A call the MPS simulation method does not run: a `cx` on one qubit
    twice, which no two-qubit matrix applies; readout noise; a loss check."""
    qir = adaptive_qir(f"  {call}{_MEASURE_AND_RECORD}", qubits=1, results=1)
    with pytest.raises(ValueError, match=message):
        run(qir, [Sample(1)], engine=engine)


@pytest.mark.parametrize("flipped", [False, True])
def test_a_noise_intrinsic_runs_as_identity(engine, flipped):
    """A bit-flip noise intrinsic (a function with the ``qdk_noise``
    attribute) after a qubit is set: no noise model is given, so every shot
    reads the value set, 1 after an `x`, 0 without."""
    set_qubit = (
        "  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))\n"
        if flipped
        else ""
    )
    qir = adaptive_qir(
        set_qubit
        + "  call void @bit_flip_noise(%Qubit* inttoptr (i64 0 to %Qubit*))"
        + _MEASURE_AND_RECORD,
        qubits=1,
        results=1,
    )
    (rows,) = run(qir, [Sample(8)], engine=engine)
    assert {bits(row) for row in rows} == {"1" if flipped else "0"}


def test_a_max_bond_dimension_too_large_for_the_qubits_raises_value_error(engine):
    """70 qubits with bonds of up to 2**32 - 1: the middle sites would hold
    more elements than a machine word counts, so no chain is made."""
    qir = adaptive_qir(
        """\
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 69 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)""",
        qubits=70,
        results=1,
    )
    options = MpsOptions(max_bond_dimension=2**32 - 1)
    with pytest.raises(ValueError, match="make no chain"):
        run(qir, [Sample(1)], engine=engine, options=options)


ON_LINUX_X86_64 = platform.system() == "Linux" and platform.machine() == "x86_64"


@pytest.fixture
def unusable_setup(monkeypatch):
    """A setup that cannot work: an override that is not an absolute path."""
    monkeypatch.setenv("QDK_CUTENSORNET_LIBRARY", "libcutensornet.so.2")


def test_a_setup_problem_raises_os_error_with_the_setup_checks_report(
    unusable_setup,
):
    """A user whose setup cannot work (here, an override that is not an
    absolute path) runs on the default engine, the GPU's, and gets an
    `OSError` whose message is the report `check_tensornetwork_setup` gives,
    with the fix, then a last line offering the CPU engine."""
    with pytest.raises(OSError) as raised:
        tensornetwork_qir(BELL, [Sample(1)])
    message = str(raised.value)
    setup = check_tensornetwork_setup()
    assert message.startswith(setup.report + "\n")
    assert "to fix:" in setup.report
    assert 'engine="cpu"' in message.splitlines()[-1]


def test_the_cpu_engine_runs_whatever_the_gpu_setup(unusable_setup):
    """A setup that cannot work leaves the CPU engine running."""
    assert not check_tensornetwork_setup().ready, "the GPU setup cannot work"
    (rows,) = run(BELL, [Sample(8)], engine="cpu")
    assert {bits(row) for row in rows} <= {"00", "11"}


def test_the_setup_check_reports_a_problem_without_raising(unusable_setup):
    """A setup that cannot work is reported as not ready, and printing the
    result shows its report; the check raises nothing."""
    setup = check_tensornetwork_setup()
    assert isinstance(setup, TensorNetworkSetup)
    assert not setup.ready
    assert str(setup) == setup.report


def test_the_setup_checks_advice_names_the_fix(unusable_setup):
    """The advice names what to fix: the override, on Linux x86-64; the
    platform, elsewhere."""
    advice = check_tensornetwork_setup().advice
    assert advice is not None, "a setup that cannot work has advice"
    if ON_LINUX_X86_64:
        assert "QDK_CUTENSORNET_LIBRARY" in advice
    else:
        assert "Linux x86-64" in advice


def test_the_setup_check_gives_the_native_warnings(monkeypatch):
    """The warnings the native check found (here, as a host with two copies
    of cuTensorNet would give them) reach `TensorNetworkSetup.warnings`."""
    warning = "another copy of cuTensorNet is at /env/libcutensornet.so.2"
    monkeypatch.setattr(
        _native,
        "_check_tensornetwork_setup",
        lambda packaged, install_command: (
            True,
            f"warning: {warning}",
            None,
            [warning],
        ),
    )
    setup = check_tensornetwork_setup()
    assert setup.ready and setup.warnings == (warning,)


@pytest.fixture
def cutensornet_wheel(tmp_path):
    """An installed distribution whose files are under `tmp_path`, where
    only `libcutensornet.so.2` is; and that file's path."""
    (tmp_path / "cuquantum" / "lib").mkdir(parents=True)
    library = tmp_path / "cuquantum" / "lib" / "libcutensornet.so.2"
    library.touch()

    class Installed:
        def locate_file(self, file):
            return tmp_path / file

    return Installed(), str(library)


@pytest.fixture
def only_cutensornet_installed(monkeypatch, cutensornet_wheel):
    """cuTensorNet's wheel is installed, and no other; gives the library's
    path."""
    installed, library = cutensornet_wheel

    def distribution(name):
        if name == "cutensornet-cu12":
            return installed
        raise _tensor_network.metadata.PackageNotFoundError(name)

    monkeypatch.setattr(_tensor_network.metadata, "distribution", distribution)
    return library


def test_only_the_installed_wheels_file_is_found(only_cutensornet_installed):
    """The file cuTensorNet's wheel installed is found through the
    installed distribution; the CUDA Runtime, whose wheel is not installed,
    adds no file."""
    assert _tensor_network._packaged_libraries() == [only_cutensornet_installed]


def test_a_file_the_wheel_lacks_adds_no_file(monkeypatch, cutensornet_wheel):
    """Each library is looked for in its own wheel: a wheel installed
    without the file (here, every wheel holds only `libcutensornet.so.2`)
    adds no `libcudart.so.12`."""
    installed, library = cutensornet_wheel
    monkeypatch.setattr(
        _tensor_network.metadata, "distribution", lambda name: installed
    )
    assert _tensor_network._packaged_libraries() == [library]


def _spy_check(received):
    """A native setup check that records the install command it is given."""

    def check(packaged, install_command):
        received.append(install_command)
        return True, "ready", None, []

    return "_check_tensornetwork_setup", check, check_tensornetwork_setup


def _spy_sample(received):
    """A native run that records the install command it is given, which its
    setup report gives when the GPU is not set up."""

    def sample(program, shots, seed, max_bond, packaged, install_command, engine):
        received.append(install_command)
        report = {
            "largest_bond": 1,
            "discarded_weight": 0.0,
            "max_bond_dimension": max_bond,
            "truncation_warning": None,
        }
        return [], report

    return "_tensor_network_sample", sample, lambda: tensornetwork_qir(BELL, [Sample(1)])


@pytest.mark.parametrize(
    "spy",
    [
        pytest.param(_spy_check, id="check_tensornetwork_setup"),
        pytest.param(_spy_sample, id="tensornetwork_qir"),
    ],
)
def test_each_entry_passes_the_install_command(monkeypatch, spy):
    """The setup check and a run each hand the native side the command that
    installs the libraries, which the setup check's advice gives when a
    library is missing (a run's `OSError` holds that advice)."""
    received = []
    native, fake, call = spy(received)
    monkeypatch.setattr(_native, native, fake)
    call()
    assert received == ['pip install "qdk[tensornetwork]"']


# ---------------------------------------------------------------------------
# The GIL and Ctrl-C, on the CPU engine
# ---------------------------------------------------------------------------


def _measured_rounds(qubits: int, rounds: int) -> str:
    """`rounds` rounds on `qubits` qubits, each of rotations, `cz` between
    neighbours, and a measurement whose result flips the next qubit; then it
    reads one result. Nearly all of a run's time is spent in the engine, on
    the rounds, and little in the rows Python converts after the run, one
    result per shot."""
    body = []
    for r in range(rounds):
        body += [f"rx({0.3 + 0.1 * r:.1f}) q[{k}];" for k in range(qubits)]
        body += [f"cz q[{k - 1}], q[{k}];" for k in range(1, qubits)]
        body += [
            f"m[{r}] = measure q[{r % qubits}];",
            f"if (m[{r}]) x q[{(r + 1) % qubits}];",
        ]
    gates = "\n".join(body)
    return qasm(
        f"""
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[{qubits}] q;
        bit[{rounds}] m;
        bit c;
        {gates}
        c = measure q[0];
        """,
        profile=qdk.TargetProfile.Adaptive_RIF,
    )


def _run_lasting(at_least: float):
    """A program and shots whose run on the CPU engine lasts at least
    `at_least` seconds, and how long it lasted: the shots are doubled until
    it does, whatever the machine's speed. The rows Python converts after a
    run stay a small part of it."""
    program, shots = _measured_rounds(12, 16), 64
    while True:
        started = time.perf_counter()
        run(program, [Sample(shots)], engine="cpu")
        whole = time.perf_counter() - started
        if whole >= at_least:
            return program, shots, whole
        shots *= 2


def test_a_run_releases_the_gil():
    """A Python thread keeps counting while a run is under way (a run of at
    least 50 ms, so that the thread counts many times if it is let run)."""
    program, shots, _ = _run_lasting(0.05)
    counted = 0
    running = threading.Event()
    done = threading.Event()

    def count():
        nonlocal counted
        running.set()
        while not done.is_set():
            counted += 1
            time.sleep(0.001)

    thread = threading.Thread(target=count)
    thread.start()
    running.wait()
    before = counted
    run(program, [Sample(shots)], engine="cpu")
    during = counted - before
    done.set()
    thread.join()
    assert during > 5, f"the thread counted {during} times during the run"


def test_ctrl_c_stops_a_run_with_keyboard_interrupt():
    """Ctrl-C during a run raises `KeyboardInterrupt` and ends the run
    early, at the next batch (the program has one per measured round): in
    less than half the time the whole run takes (Python would also raise it
    once a run that ignored it had ended). A run checks for signals only
    every 0.1 s, so the whole run lasts at least a second, many times
    longer."""
    program, shots, whole = _run_lasting(1.0)
    timer = threading.Timer(whole / 10, _thread.interrupt_main)
    started = time.perf_counter()
    timer.start()
    try:
        with pytest.raises(KeyboardInterrupt):
            run(program, [Sample(shots)], engine="cpu")
    finally:
        timer.cancel()
    interrupted = time.perf_counter() - started
    assert interrupted < whole / 2, f"{interrupted:.2f} s of {whole:.2f} s"

