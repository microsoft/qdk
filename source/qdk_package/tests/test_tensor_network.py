# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""``tensornetwork_qir`` and ``check_tensornetwork_setup``: the arguments they
take, the rows and report a run returns, its seeds, its warning, the error
each cause raises, and that a run leaves the GIL and stops on Ctrl-C.

The rows run on the exact reference engine (the private
``engine="reference"``), so they need no GPU; the tests marked ``gpu`` run
on cuTensorNet and are skipped where the setup check is not ready.
"""

import _thread
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


def run(qir, queries, **kwargs):
    """``tensornetwork_qir`` on the exact reference engine."""
    kwargs.setdefault("method", "mps")
    kwargs.setdefault("options", None)
    kwargs.setdefault("seed", 7)
    return _tensor_network._run(qir, queries, engine="reference", **kwargs)


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
def test_exactly_one_sample_is_asked(queries):
    with pytest.raises(ValueError, match="exactly one Sample"):
        run(BELL, queries)


def test_a_query_other_than_sample_is_refused():
    with pytest.raises(TypeError, match="Sample"):
        run(BELL, ["Expectation"])


# ---------------------------------------------------------------------------
# Rows and report
# ---------------------------------------------------------------------------


def test_rows_are_the_shots_in_run_qirs_format():
    """A Bell pair's 32 shots: one row each, formatted as `run_qir` formats
    them, the two readouts agreeing, both values occurring."""
    (rows,) = run(BELL, [Sample(32)])
    expected_shape = qdk.simulation.run_qir(BELL, shots=1, seed=7, type="cpu")[0]
    assert len(rows) == 32
    assert all(type(row) is type(expected_shape) for row in rows)
    readouts = {bits(row) for row in rows}
    assert readouts == {"00", "11"}


def test_an_adaptive_program_steers_its_shots_later_gates():
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
    (rows,) = run(qir, [Sample(32)])
    assert {bits(row) for row in rows} == {"00", "11"}


def test_the_report_comes_with_the_results():
    """A GHZ state's largest bond is 2; the exact reference discards nothing;
    the report gives the max bond dimension the run used."""
    results = run(ghz(3), [Sample(4)], options=MpsOptions(max_bond_dimension=16))
    (rows,) = results
    assert len(rows) == 4
    report = results.report
    assert report.largest_bond == 2
    assert report.discarded_weight == pytest.approx(0.0, abs=1e-9)
    assert report.max_bond_dimension == 16


def test_a_seed_reproduces_each_shots_rows_whatever_the_count():
    (five,) = run(coins(4), [Sample(5)], seed=11)
    (ten,) = run(coins(4), [Sample(10)], seed=11)
    assert five == ten[:5]


def test_another_seed_gives_other_rows():
    (one,) = run(coins(8), [Sample(16)], seed=1)
    (other,) = run(coins(8), [Sample(16)], seed=2)
    assert one != other


def test_a_run_that_lost_weight_warns(monkeypatch):
    """The run's truncation warning, from the native report, is raised as a
    `RuntimeWarning` with its text (the reference never truncates, so the
    report is altered after a real run)."""
    native = _native._tensor_network_sample
    text = "the matrix product states lost up to 1.000e-2 of their weight"

    def lossy(*args):
        records, report = native(*args)
        report["truncation_warning"] = text
        return records, report

    monkeypatch.setattr(_native, "_tensor_network_sample", lossy)
    with pytest.warns(RuntimeWarning, match="lost up to"):
        run(BELL, [Sample(2)])


def test_an_exact_run_does_not_warn():
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        run(ghz(3), [Sample(2)])


# ---------------------------------------------------------------------------
# Errors
# ---------------------------------------------------------------------------

_QIR_DECLS = """\
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__qis__read_result__body(%Result*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)
declare void @__quantum__rt__int_record_output(i64, i8*)
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
"""


def test_a_trap_raises_runtime_error_naming_the_shot():
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
        run(qir, [Sample(3)])


def test_a_call_the_method_cannot_run_raises_value_error():
    """A `cx` on one qubit twice, which no two-qubit matrix applies."""
    qir = adaptive_qir(
        """\
  call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)""",
        qubits=1,
        results=1,
    )
    with pytest.raises(ValueError, match="twice"):
        run(qir, [Sample(1)])


ON_LINUX_X86_64 = platform.system() == "Linux" and platform.machine() == "x86_64"


@pytest.fixture
def no_library(monkeypatch):
    """A setup that cannot work: an override that is not an absolute path."""
    monkeypatch.setenv("QDK_CUTENSORNET_LIBRARY", "libcutensornet.so.2")


def test_a_setup_problem_raises_os_error_with_the_setup_checks_report(no_library):
    with pytest.raises(OSError) as raised:
        tensornetwork_qir(BELL, [Sample(1)])
    setup = check_tensornetwork_setup()
    assert str(raised.value) == setup.report
    assert "to fix:" in str(raised.value)


def test_the_setup_check_reports_the_problem_and_its_fix_without_raising(no_library):
    setup = check_tensornetwork_setup()
    assert isinstance(setup, TensorNetworkSetup)
    assert not setup.ready
    assert setup.advice is not None
    assert str(setup) == setup.report
    if ON_LINUX_X86_64:
        assert "QDK_CUTENSORNET_LIBRARY" in setup.advice
    else:
        assert "Linux x86-64" in setup.advice


def test_the_setup_check_gives_the_native_warnings(monkeypatch):
    """The warnings the native check found (here, as a host with two copies
    of cuTensorNet would give them) reach `TensorNetworkSetup.warnings`."""
    warning = "another copy of cuTensorNet is at /env/libcutensornet.so.2"
    monkeypatch.setattr(
        _native,
        "_check_tensornetwork_setup",
        lambda packaged: (True, f"warning: {warning}", None, [warning]),
    )
    setup = check_tensornetwork_setup()
    assert setup.ready and setup.warnings == (warning,)


def test_the_wheels_libraries_are_found_in_the_environment(monkeypatch, tmp_path):
    """The files NVIDIA's wheels installed are found through the installed
    distributions, each library from its own wheel; a wheel not installed,
    or a file it lacks, gives none."""
    (tmp_path / "cuquantum" / "lib").mkdir(parents=True)
    (tmp_path / "cuquantum" / "lib" / "libcutensornet.so.2").touch()

    class Installed:
        def locate_file(self, file):
            return tmp_path / file

    def distribution(name):
        if name == "cutensornet-cu12":
            return Installed()
        raise _tensor_network.metadata.PackageNotFoundError(name)

    monkeypatch.setattr(_tensor_network.metadata, "distribution", distribution)
    assert _tensor_network._packaged_libraries() == (
        str(tmp_path / "cuquantum" / "lib" / "libcutensornet.so.2"),
        None,
    )
    monkeypatch.setattr(
        _tensor_network.metadata, "distribution", lambda name: Installed()
    )
    cutensornet, cuda_runtime = _tensor_network._packaged_libraries()
    assert cutensornet is not None and cuda_runtime is None, "libcudart.so.12 is absent"


# ---------------------------------------------------------------------------
# The GIL and Ctrl-C
# ---------------------------------------------------------------------------

# Long enough on the reference engine (tens of milliseconds at least) for
# another thread to run during the run.
LONG_PROGRAM, LONG_SHOTS = coins(12), 4096


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


def _interruptible_run():
    """A program and shots whose run lasts at least a second, and how long it
    lasted. A run checks for signals only every 0.1 s (`SIGNAL_CHECK` in
    `tensor_network.rs`), so the run must last many times longer for Ctrl-C
    to be able to stop it early; the shots are doubled until it does,
    whatever the machine's speed. The rows Python converts after a run, which
    an interrupt skips even when the run ignored it, stay a small part."""
    program, shots = _measured_rounds(12, 16), 64
    while True:
        started = time.perf_counter()
        run(program, [Sample(shots)])
        whole = time.perf_counter() - started
        if whole >= 1.0:
            return program, shots, whole
        shots *= 2


def test_a_run_releases_the_gil():
    """A Python thread keeps counting while a run is under way."""
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
    run(LONG_PROGRAM, [Sample(LONG_SHOTS)])
    during = counted - before
    done.set()
    thread.join()
    assert during > 5, f"the thread counted {during} times during the run"


def test_ctrl_c_stops_a_run_with_keyboard_interrupt():
    """Ctrl-C during a run raises `KeyboardInterrupt` and ends the run
    early: in less than half the time the whole run takes (Python would
    also raise it once a run that ignored it had ended)."""
    program, shots, whole = _interruptible_run()
    timer = threading.Timer(whole / 10, _thread.interrupt_main)
    started = time.perf_counter()
    timer.start()
    try:
        with pytest.raises(KeyboardInterrupt):
            run(program, [Sample(shots)])
    finally:
        timer.cancel()
    interrupted = time.perf_counter() - started
    assert interrupted < whole / 2, f"{interrupted:.2f} s of {whole:.2f} s"


# ---------------------------------------------------------------------------
# On the GPU (cuTensorNet), where the setup check is ready
# ---------------------------------------------------------------------------

gpu = pytest.mark.skipif(
    not check_tensornetwork_setup().ready, reason="cuTensorNet is not set up"
)


@gpu
def test_on_the_gpu_a_bell_pairs_readouts_agree():
    results = tensornetwork_qir(BELL, [Sample(32)], seed=7)
    (rows,) = results
    assert {bits(row) for row in rows} == {"00", "11"}
    assert results.report.largest_bond == 2
