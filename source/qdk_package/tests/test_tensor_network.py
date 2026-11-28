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
import contextlib
import copy
import dataclasses
import pickle
import random
import sys
import threading
import time
import warnings

import pyqir
import pytest

import qdk
import qdk.openqasm
from qdk import Result
from qdk.simulation import (
    MpsOptions,
    MpsReport,
    Sample,
    TensorNetworkOptions,
    TensorNetworkResults,
    check_tensornetwork_setup,
    tensornetwork_qir,
)
from qdk.simulation import _tensor_network
from qdk import _native

# Marks a test that runs on the GPU engine: skipped unless the setup check
# finds it ready.
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


# A Base profile program: a Bell pair, both qubits read at the end.
BELL = qasm("""
    OPENQASM 3.0;
    include "stdgates.inc";
    qubit[2] q;
    bit[2] c;
    h q[0];
    cx q[0], q[1];
    c = measure q;
    """)


# An Adaptive profile program: a coin, whose value a branch copies to a
# second qubit.
COPIED_COIN = qasm(
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


def ghz(qubits: int) -> str:
    """A GHZ state on `qubits` qubits, by a chain of `cx`, all read at the end."""
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
    gates = "\n".join(
        [
            *(f"h q[{k}];" for k in range(qubits)),
            *(f"cz q[{k - 1}], q[{k}];" for k in range(1, qubits)),
        ]
    )
    return qasm(f"""
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[{qubits}] q;
        bit[{qubits}] c;
        {gates}
        c = measure q;
        """)


# The declarations `adaptive_qir` writes into every program: the gates,
# measurements, readout-noise call, loss check, output records and noise
# intrinsic (`#2`) the tests' programs call.
_QIR_DECLS = """\
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__rx__body(double, %Qubit*)
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__qis__read_result__body(%Result*)
declare void @__quantum__rt__readout_noise(double, double, %Result*)
declare void @__quantum__qis__peek_loss__body(%Qubit*, %Result*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)
declare void @__quantum__rt__int_record_output(i64, i8*)
declare void @__quantum__rt__bool_record_output(i1, i8*)
declare void @__quantum__rt__double_record_output(double, i8*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)
declare void @__quantum__rt__array_record_output(i64, i8*)
declare void @bit_flip_noise(%Qubit*) #2
"""


def adaptive_qir(body: str, qubits: int, results: int) -> str:
    """An Adaptive profile program whose entry point runs `body`, on
    `qubits` qubits and `results` results, with `_QIR_DECLS` declared."""
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


# ---------------------------------------------------------------------------
# Arguments
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "make, name, lowest",
    [
        pytest.param(Sample, "shots", 1, id="shots"),
        pytest.param(
            lambda bond: MpsOptions(max_bond_dimension=bond),
            "max_bond_dimension",
            1,
            id="max bond dimension",
        ),
        pytest.param(
            lambda seed: run(BELL, [Sample(1)], seed=seed), "seed", 0, id="seed"
        ),
    ],
)
@pytest.mark.parametrize(
    "value",
    [
        pytest.param(lambda lowest: lowest - 1, id="one below its range"),
        pytest.param(lambda lowest: 2**32, id="past its range"),
        pytest.param(lambda lowest: True, id="a bool"),
        pytest.param(lambda lowest: 1.5, id="a float"),
        pytest.param(lambda lowest: 2.0, id="an integral float"),
        pytest.param(lambda lowest: "7", id="a str"),
    ],
)
def test_a_number_out_of_its_range_is_refused_naming_it(make, name, lowest, value):
    """A shot count, a max bond dimension or a seed that is not an integer
    in its range ([1, 2**32), [1, 2**32), [0, 2**32)) is refused before any
    run, naming the argument: one below the range, past it, a ``bool``, a
    ``float`` (integral or not), a ``str``. Each `value` is a function of
    the range's lowest value."""
    with pytest.raises(ValueError, match=name):
        make(value(lowest))


def test_the_default_max_bond_dimension_is_128():
    """Options made without a max bond dimension take χ = 128."""
    assert MpsOptions().max_bond_dimension == 128


@pytest.mark.parametrize(
    "argument, name",
    [
        pytest.param("method", "contraction", id="method"),
        pytest.param("engine", "cutensornet", id="engine, the library's name"),
    ],
)
def test_an_unknown_method_or_engine_is_refused_naming_it(argument, name):
    """A method other than ``"mps"``, or an engine other than ``"gpu"`` and
    ``"cpu"``, is a `ValueError` naming the argument and the value given."""
    with pytest.raises(ValueError, match=argument) as raised:
        run(BELL, [Sample(1)], **{argument: name})
    assert repr(name) in str(raised.value)


@pytest.mark.parametrize("seed", [0, 2**32 - 1])
def test_a_seed_at_either_end_of_its_range_runs(seed):
    """A seed of 0 or 2**32 − 1, the ends of its documented range, runs."""
    (rows,) = run(BELL, [Sample(4)], seed=seed)
    assert len(rows) == 4


@pytest.mark.parametrize("queries", [[], [Sample(1), Sample(2)]])
def test_queries_other_than_one_sample_are_refused(queries):
    """No query, or more than one, is refused: a run answers one Sample."""
    with pytest.raises(ValueError, match="exactly one Sample"):
        run(BELL, queries)


@pytest.mark.parametrize(
    "arguments, message",
    [
        pytest.param({"options": {"max_bond_dimension": 4}}, "options", id="options"),
        pytest.param(
            {"options": TensorNetworkOptions()},
            "MpsOptions",
            id="options not the method's",
        ),
        pytest.param({"queries": ["Expectation"]}, "'Expectation'", id="a query"),
        pytest.param({"queries": Sample(1)}, "queries", id="a bare Sample"),
        pytest.param({"engine": 0}, "str", id="engine"),
        pytest.param({"input": None}, "bytes", id="input"),
    ],
)
def test_an_argument_of_another_type_raises_type_error(arguments, message):
    """An argument whose type the entry does not take raises `TypeError`
    whose message names the argument or the query at fault where the entry
    checks it (``options``, ``queries`` and each query). The ``engine`` and
    ``input`` rows match the type expected: their messages are pyo3's and
    the QIR parser's."""
    arguments = {"input": BELL, "queries": [Sample(1)], **arguments}
    with pytest.raises(TypeError, match=message):
        run(arguments.pop("input"), **arguments)


def test_a_type_error_of_the_callers_queries_reaches_the_caller():
    """Queries whose iteration fails with a `TypeError` of their own (a
    caller's generator with a defect, say) raise that error, not the entry's
    message about what ``queries`` must be."""

    def queries():
        """A generator whose first step raises its own `TypeError`."""
        raise TypeError("the caller's generator failed")
        yield Sample(1)

    with pytest.raises(TypeError, match="the caller's generator failed"):
        run(BELL, queries())


@pytest.mark.parametrize(
    "program",
    [
        pytest.param(BELL, id="Base profile"),
        pytest.param(COPIED_COIN, id="Adaptive profile"),
    ],
)
def test_a_program_as_bitcode_runs_as_its_text(program):
    """A program given as bitcode (``pyqir.Module.bitcode``, as a user's
    own pyqir module gives it) runs as the same program given as text:
    with the same seed, the same rows and the same report."""
    # The programs are `QirInputData`; pyqir parses only a `str`.
    text = str(program)
    bitcode = pyqir.Module.from_ir(pyqir.Context(), text).bitcode
    from_text = run(text, [Sample(16)])
    from_bitcode = run(bitcode, [Sample(16)])
    assert from_bitcode == from_text
    assert from_bitcode.report == from_text.report


# ---------------------------------------------------------------------------
# Rows and report
# ---------------------------------------------------------------------------


def test_a_run_gives_one_row_per_shot_with_the_programs_correlations(engine):
    """A Bell pair's 32 shots: one row each, its two results agreeing, both
    values occurring."""
    (rows,) = run(BELL, [Sample(32)], engine=engine)
    assert len(rows) == 32
    zeros, ones = [Result.Zero, Result.Zero], [Result.One, Result.One]
    assert all(row in (zeros, ones) for row in rows)
    assert zeros in rows and ones in rows


# An `adaptive_qir` body that flips qubit 0, then measures qubits 0 and 1
# into results 0 and 1: each shot's results are 1, then 0.
_X_AND_MEASURE = """\
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))"""


@pytest.mark.parametrize(
    "records, row",
    [
        pytest.param(
            "  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)",
            Result.One,
            id="one value alone",
        ),
        pytest.param(
            """\
  call void @__quantum__rt__tuple_record_output(i64 3, i8* null)
  call void @__quantum__rt__int_record_output(i64 -3, i8* null)
  call void @__quantum__rt__bool_record_output(i1 true, i8* null)
  call void @__quantum__rt__double_record_output(double 1.25, i8* null)""",
            (-3, True, 1.25),
            id="a tuple of classical values",
        ),
        pytest.param(
            """\
  call void @__quantum__rt__array_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* null)""",
            [Result.One, Result.Zero],
            id="an array of results",
        ),
        pytest.param("", "10", id="no record"),
    ],
)
def test_a_row_has_the_structure_the_program_records(engine, records, row):
    """Two qubits, the first flipped, both measured: each shot's row is
    what the program records, in the structure it records it: one value
    alone, a tuple, a list, or with no record its measurements as a
    string."""
    qir = adaptive_qir(f"{_X_AND_MEASURE}\n{records}", qubits=2, results=2)
    (rows,) = run(qir, [Sample(2)], engine=engine)
    assert rows == [row, row]


def test_an_adaptive_program_steers_its_shots_later_gates(engine):
    """A coin whose value a branch copies to a second qubit, in every shot,
    both values occurring."""
    (rows,) = run(COPIED_COIN, [Sample(32)], engine=engine)
    assert {bits(row) for row in rows} == {"00", "11"}


@pytest.mark.parametrize(
    "start, recorded",
    [
        ("110", "111"),
        ("111", "110"),
        ("100", "100"),
    ],
)
def test_a_ccx_flips_its_target_when_both_controls_are_one(engine, start, recorded):
    """A Base profile program's `ccx` (the Toffoli gate), on two controls and
    a target set to `start`: each shot records the target flipped when both
    controls are 1, and every qubit unchanged otherwise. The run lowers the
    `ccx` into gates the method runs; the lowering's other rows are its
    own rule."""
    flips = "\n".join(f"x q[{k}];" for k, bit in enumerate(start) if bit == "1")
    qir = qasm(f"""
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[3] q;
        bit[3] c;
        {flips}
        ccx q[0], q[1], q[2];
        c = measure q;
        """)
    (rows,) = run(qir, [Sample(2)], engine=engine)
    assert [bits(row) for row in rows] == [recorded, recorded]


def test_an_mps_runs_report_gives_the_largest_bond_dimension_the_discarded_weight_and_the_max(
    engine,
):
    """A GHZ state at max bond dimension 16, by the MPS simulation method:
    its report is the method's, an `MpsReport`, which gives the state's
    largest bond dimension, 2; nothing discarded, as every bond fits; and
    the max bond dimension the run used."""
    results = run(
        ghz(3),
        [Sample(4)],
        engine=engine,
        method="mps",
        options=MpsOptions(max_bond_dimension=16),
    )
    report = results.report
    assert isinstance(report, MpsReport)
    assert report.largest_bond_dimension == 2
    assert report.discarded_weight == pytest.approx(0.0, abs=1e-9)
    assert report.max_bond_dimension == 16


def test_a_seed_reproduces_each_shots_rows_whatever_the_count(engine):
    """Exactly on the CPU engine; on the GPU engine to rounding, which
    almost never decides a draw."""
    (five,) = run(coins(4), [Sample(5)], engine=engine, seed=11)
    (ten,) = run(coins(4), [Sample(10)], engine=engine, seed=11)
    assert five == ten[:5]


def test_another_seed_gives_other_rows(engine):
    """Eight entangled coins, 16 shots: seeds 1 and 2 give other rows."""
    (one,) = run(coins(8), [Sample(16)], engine=engine, seed=1)
    (other,) = run(coins(8), [Sample(16)], engine=engine, seed=2)
    assert one != other


def test_a_run_without_a_seed_draws_a_fresh_one(monkeypatch):
    """Two runs without a seed, whose draws are made to give 1, then 2:
    each gives the rows of the seed it drew. Seeds 1 and 2 give other rows
    on this program (the test above), so a run that kept a fixed seed, or
    reused its first draw, fails one of them."""
    program = coins(8)
    seeded = [run(program, [Sample(16)], seed=seed) for seed in (1, 2)]
    _draw_seeds(monkeypatch, 1, 2)
    unseeded = [run(program, [Sample(16)], seed=None) for _ in range(2)]
    assert unseeded == seeded


def _draw_seeds(monkeypatch, *seeds):
    """Makes the draw of a run without a seed give `seeds`, in turn. The run
    draws it with Python's `random.randint`, as `run_qir` does."""
    draws = iter(seeds)
    monkeypatch.setattr(random, "randint", lambda lowest, highest: next(draws))


# A pair whose `cx` makes a bond with Schmidt weights 3/4 and 1/4: `ry(pi /
# 3)` leaves qubit 0 with amplitudes cos(pi / 6) and sin(pi / 6).
UNEVEN_PAIR = qasm("""
    OPENQASM 3.0;
    include "stdgates.inc";
    qubit[2] q;
    bit[2] c;
    ry(pi / 3) q[0];
    cx q[0], q[1];
    c = measure q;
    """)


def test_a_run_that_lost_weight_warns(engine):
    """An uneven pair at max bond dimension 1: the bond the `cx` makes,
    with Schmidt weights 3/4 and 1/4, is cut to 1, losing a quarter of the
    state's weight, so the run warns: a `RuntimeWarning` pointing at the
    caller's line, with the method's warning text, which names the max bond
    dimension to raise."""
    with pytest.warns(RuntimeWarning, match=r"max bond dimension 1\b") as warned:
        results = tensornetwork_qir(
            UNEVEN_PAIR,
            [Sample(2)],
            engine=engine,
            seed=7,
            options=MpsOptions(max_bond_dimension=1),
        )
    assert warned[0].filename == __file__
    assert results.report.discarded_weight == pytest.approx(0.25, rel=1e-3)


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


def test_results_with_equal_rows_are_equal_whatever_their_reports():
    """Results compare as the tuple of their per-query results: a user
    comparing two runs' rows (an exact run and a truncated one that sampled
    the same, say) finds them equal though their reports differ."""
    rows = [[Result.Zero], [Result.One]]
    exact = MpsReport(
        largest_bond_dimension=2, discarded_weight=0.0, max_bond_dimension=2
    )
    truncated = dataclasses.replace(
        exact, largest_bond_dimension=1, discarded_weight=0.25, max_bond_dimension=1
    )
    assert TensorNetworkResults([rows], exact) == TensorNetworkResults(
        [rows], truncated
    )


def test_an_exact_run_does_not_warn(engine):
    """A GHZ state at the default max bond dimension loses nothing."""
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        run(ghz(3), [Sample(2)], engine=engine)


# ---------------------------------------------------------------------------
# Errors
# ---------------------------------------------------------------------------


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
    with pytest.raises(RuntimeError, match=r"shot 0\b"):
        run(qir, [Sample(3)], engine=engine)


# An `adaptive_qir` body's ending: qubit 0 measured into result 0, which is
# recorded.
_MEASURE_AND_RECORD = """
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)"""

# A readout-noise call on result 0, which the MPS simulation method refuses.
_READOUT_NOISE = "call void @__quantum__rt__readout_noise(double 0.1, double 0.1, %Result* inttoptr (i64 0 to %Result*))"


@pytest.mark.parametrize(
    "call, message",
    [
        # Repeated static qubits fail before the run, as in `run_qir`.
        pytest.param(
            "call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))",
            r"qubits in invocation.*cnot.*are not unique",
            id="a two-qubit gate given one qubit twice",
        ),
        pytest.param(
            "%index = add i64 0, 0\n"
            "  %qubit = inttoptr i64 %index to %Qubit*\n"
            "  call void @__quantum__qis__cnot__body(%Qubit* %qubit, %Qubit* %qubit)",
            r"`cx`.*qubit 0",
            id="a two-qubit gate given one qubit twice, the qubit computed at run time",
        ),
        pytest.param(_READOUT_NOISE, "readout_noise", id="readout noise"),
        pytest.param(
            "call void @__quantum__qis__peek_loss__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))",
            "peek_loss",
            id="a qubit's loss check",
        ),
        pytest.param(
            "call void @bit_flip_noise(%Qubit* inttoptr (i64 0 to %Qubit*))",
            "bit_flip_noise",
            id="a noise intrinsic",
        ),
        pytest.param(
            "call void @__quantum__qis__rx__body(double 0x7FF8000000000000, %Qubit* inttoptr (i64 0 to %Qubit*))",
            "not unitary",
            id="a rotation by NaN",
        ),
    ],
)
def test_a_call_the_method_cannot_run_raises_value_error(engine, call, message):
    """A call the MPS simulation method does not run, named by its message:
    a `cx` on one qubit twice, which no two-qubit matrix applies (refused
    before the run where the qubit is a constant, during it where the qubit
    is computed); readout noise and a qubit's loss check, as the method
    refuses every call it does not run; a noise intrinsic (a function with
    the ``qdk_noise`` attribute), whose noise the method does not run yet.
    And a rotation by an angle that is not finite, whose matrix the engine
    refuses: its message gives the refusal, not the call, which the
    engine's refusal does not carry."""
    qir = adaptive_qir(f"  {call}{_MEASURE_AND_RECORD}", qubits=1, results=1)
    with pytest.raises(ValueError, match=message):
        run(qir, [Sample(1)], engine=engine)


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
    with pytest.raises(ValueError, match=r"70 qubits.*4294967295"):
        run(qir, [Sample(1)], engine=engine, options=options)


@pytest.fixture
def unusable_setup(monkeypatch):
    """A setup that cannot work: an override that is not an absolute path."""
    monkeypatch.setenv("QDK_CUTENSORNET_LIBRARY", "libcutensornet.so.2")


@pytest.mark.parametrize(
    "program",
    [
        pytest.param(BELL, id="a program the method runs"),
        pytest.param(
            adaptive_qir(
                f"  {_READOUT_NOISE}{_MEASURE_AND_RECORD}", qubits=1, results=1
            ),
            id="a readout-noise call, found once the engine is made",
        ),
    ],
)
def test_a_setup_problem_raises_os_error_with_the_setup_checks_report(
    unusable_setup, program
):
    """A user whose setup cannot work (here, an override that is not an
    absolute path) runs on the default engine, the GPU's, and gets an
    `OSError` whose message is the report `check_tensornetwork_setup` gives,
    with the fix, then a last line offering the CPU engine. A fault of the
    program that is found once the engine is made (a readout-noise call)
    does not change that: no engine was made."""
    with pytest.raises(OSError) as raised:
        tensornetwork_qir(program, [Sample(1)])
    message = str(raised.value)
    setup = check_tensornetwork_setup()
    assert message.startswith(setup.report + "\n")
    assert setup.advice is not None and setup.advice in setup.report
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
    assert not setup.ready
    assert str(setup) == setup.report


def test_the_setup_check_gives_its_warnings(monkeypatch):
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


def _spy_check(received):
    """A native setup check that records the library files and the install
    command it is given."""

    def check(packaged, install_command):
        """Records its arguments, and finds the setup ready."""
        received.append((packaged, install_command))
        return True, "ready", None, []

    return "_check_tensornetwork_setup", check, check_tensornetwork_setup


def _spy_sample(received):
    """A native run that records the library files and the install command
    it is given, which its setup report gives when the GPU is not set up."""

    def sample(program, shots, seed, max_bond, packaged, install_command, engine):
        """Records the library files and the install command, and gives no
        rows with an exact run's report."""
        received.append((packaged, install_command))
        report = {
            "largest_bond_dimension": 1,
            "discarded_weight": 0.0,
            "max_bond_dimension": max_bond,
            "truncation_warning": None,
        }
        return [], report

    return (
        "_sample_mps",
        sample,
        lambda: tensornetwork_qir(BELL, [Sample(1)]),
    )


# The public entries that hand the native side the wheels' library files and
# the install command, each as the spy that records what it hands.
SPIED_ENTRIES = [
    pytest.param(_spy_check, id="check_tensornetwork_setup"),
    pytest.param(_spy_sample, id="tensornetwork_qir"),
]


def received_by_native(monkeypatch, spy):
    """Calls the public entry `spy` stands for, with its native function
    replaced by `spy`'s recorder, and gives what the entry handed the native
    side: the library files the wheels installed and the install command."""
    received = []
    native, fake, call = spy(received)
    monkeypatch.setattr(_native, native, fake)
    call()
    (arguments,) = received
    return arguments


# Where each NVIDIA wheel of `qdk[tensornetwork]` whose library the QDK
# opens installs that library, relative to its distribution's root.
WHEEL_LIBRARIES = {
    "cutensornet-cu12": "cuquantum/lib/libcutensornet.so.2",
    "nvidia-cuda-runtime-cu12": "nvidia/cuda_runtime/lib/libcudart.so.12",
}


@pytest.fixture
def install_wheels(monkeypatch, tmp_path):
    """A function that installs, as Python's package metadata sees them, the
    wheels it is given, each in a folder of its own under `tmp_path`: a dict
    from a wheel's name to whether the wheel holds its library file. It
    gives the paths of the library files installed."""

    class InstalledWheel:
        """An installed distribution whose files are under `root`."""

        def __init__(self, root):
            """A distribution installed under `root`."""
            self.root = root

        def locate_file(self, file):
            """The path of `file`, relative to the distribution's root."""
            return self.root / file

    def install(wheels):
        """Installs `wheels`, and gives the paths of the library files they
        hold."""
        installed, files = {}, []
        for name, holds_its_file in wheels.items():
            installed[name] = InstalledWheel(tmp_path / name)
            if holds_its_file:
                library = tmp_path / name / WHEEL_LIBRARIES[name]
                library.parent.mkdir(parents=True)
                library.touch()
                files.append(str(library))

        def distribution(name):
            """The wheel named `name`, if installed, as
            `importlib.metadata.distribution` finds it."""
            if name not in installed:
                raise _tensor_network.metadata.PackageNotFoundError(name)
            return installed[name]

        monkeypatch.setattr(_tensor_network.metadata, "distribution", distribution)
        return files

    return install


@pytest.mark.parametrize(
    "wheels",
    [
        pytest.param(
            {"cutensornet-cu12": True, "nvidia-cuda-runtime-cu12": True},
            id="both wheels holding their files",
        ),
        pytest.param({"cutensornet-cu12": True}, id="the CUDA Runtime's wheel missing"),
        pytest.param(
            {"cutensornet-cu12": True, "nvidia-cuda-runtime-cu12": False},
            id="the CUDA Runtime's wheel lacking its file",
        ),
    ],
)
@pytest.mark.parametrize("spy", SPIED_ENTRIES)
def test_exactly_the_files_the_installed_wheels_hold_are_found(
    monkeypatch, install_wheels, wheels, spy
):
    """The setup check and a run each hand the native side the library file
    of each NVIDIA wheel installed, cuTensorNet's and the CUDA Runtime's,
    found through the installed distributions. Each library is looked for
    in its own wheel, and a wheel that lacks its file (or is missing) adds
    no file."""
    files = install_wheels(wheels)
    packaged, _ = received_by_native(monkeypatch, spy)
    assert sorted(packaged) == sorted(files)


@pytest.mark.parametrize("spy", SPIED_ENTRIES)
def test_each_entry_passes_the_install_command(monkeypatch, spy):
    """The setup check and a run each hand the native side the command that
    installs the libraries, which the setup check's advice gives when a
    library is missing (a run's `OSError` holds that advice)."""
    _, install_command = received_by_native(monkeypatch, spy)
    assert install_command == 'pip install "qdk[tensornetwork]"'


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


def test_a_run_releases_the_gil(monkeypatch):
    """A Python thread that waits for a run to start gets to run before the
    run ends. The switch interval is set beyond the test, so the caller's
    thread gives the GIL up only when it waits: the thread runs during the
    run only if the run releases the GIL. The run lasts at least 50 ms, time
    for the OS to schedule the thread once the GIL is free."""
    program, shots, _ = _run_lasting(0.05)
    with _switch_interval(1000), _thread_waiting_for_the_run(
        monkeypatch
    ) as thread_ran_during_the_run:
        run(program, [Sample(shots)], engine="cpu")
    assert thread_ran_during_the_run == [True], "the thread ran only after the run"


@contextlib.contextmanager
def _switch_interval(seconds: float):
    """Sets how long Python lets a thread hold the GIL before switching
    threads (`sys.setswitchinterval`) to `seconds`, within the block."""
    previous = sys.getswitchinterval()
    sys.setswitchinterval(seconds)
    try:
        yield
    finally:
        sys.setswitchinterval(previous)


@contextlib.contextmanager
def _thread_waiting_for_the_run(monkeypatch):
    """A thread that waits for the native run to start, then marks that it
    ran, alive within the block. Gives a list that, once a run has ended,
    holds whether the thread had run by then."""
    run_started = threading.Event()
    thread_ran = threading.Event()
    thread_ran_during_the_run = []
    native_sample = _native._sample_mps

    def sample(*args, **kwargs):
        """The native run, which says it started, and records at its end
        whether the thread ran."""
        run_started.set()
        try:
            return native_sample(*args, **kwargs)
        finally:
            thread_ran_during_the_run.append(thread_ran.is_set())

    def wait_for_the_run():
        """Waits for the run to start, then marks that the thread ran."""
        run_started.wait()
        thread_ran.set()

    monkeypatch.setattr(_native, "_sample_mps", sample)
    thread = threading.Thread(target=wait_for_the_run)
    thread.start()
    try:
        yield thread_ran_during_the_run
    finally:
        run_started.set()
        thread.join()


def test_ctrl_c_stops_a_run_with_keyboard_interrupt():
    """Ctrl-C during a run reaches it: the run ends early and raises
    `KeyboardInterrupt`. This pins the wiring, Python's signal to the native
    wait, the wait's stop flag to the run, and the signal's exception back
    to Python; how the wait handles a signal is tested in Rust
    (`tensor_network/detached_run/tests.rs`), and how a run stops on its
    flag in the tests of `qdk_simulators`' MPS simulation method. The run
    ends at its next batch
    (the program has one per measured round), in less than half the time
    the whole run takes (Python would also raise it once a run that ignored
    it had ended). The wait checks for signals only every 0.1 s, so the
    whole run lasts at least a second, many times longer. The run's own
    duration is the measure: the run calls no Python and a stopped run
    raises as a finished one does, so no event shows that it stopped
    early."""
    program, shots, whole = _run_lasting(1.0)
    started = time.perf_counter()
    with pytest.raises(KeyboardInterrupt), _ctrl_c_after(whole / 10):
        run(program, [Sample(shots)], engine="cpu")
    interrupted = time.perf_counter() - started
    assert interrupted < whole / 2, f"{interrupted:.2f} s of {whole:.2f} s"


@contextlib.contextmanager
def _ctrl_c_after(seconds: float):
    """Presses Ctrl-C (`_thread.interrupt_main`) `seconds` into the block,
    unless the block has ended by then."""
    timer = threading.Timer(seconds, _thread.interrupt_main)
    timer.start()
    try:
        yield
    finally:
        timer.cancel()
