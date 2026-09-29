# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import FrozenInstanceError
import json
import os
from pathlib import Path
import subprocess
import sys
from typing import Any, Literal, cast

import pytest

import qdk.simulation
from qdk import _native, stim
from qdk._native import Result
from qdk.simulation import (
    ContractionOptions,
    Cost,
    Expectation,
    MpsOptions,
    Probability,
    tensornetwork_qir,
)
from qdk.simulation._tensor_network import _contraction_options_dict
from test_cpu_simulator import (
    BELL_BASE_QIR,
    NVIDIA_MPS_AVAILABLE,
    NVIDIA_MPS_SKIP_REASON,
    SX_CZ_MRESETZ_BASE_QIR,
    X_RESET_MEASURE_BASE_QIR,
    _adaptive_program,
)

# Arguments are validated before the program is parsed, so these tests need no
# valid program.
QIR = "not parsed"

# Rx(π/2) on qubit 0 and H on qubit 1: ⟨Y₀⟩ = -sin(π/2) = -1 and ⟨X₁⟩ = 1.
# Y is the only non-symmetric Pauli, so a transposed Y would read ⟨Y₀⟩ = +1.
RX_H_BASE_QIR = """\
%Result = type opaque
%Qubit = type opaque

define void @ENTRYPOINT__main() #0 {
entry:
    call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
    call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
    call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
    call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
    call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
    call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* null)
    ret void
}

declare void @__quantum__qis__rx__body(double, %Qubit*)
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }
"""


def test_queries_are_publicly_exported():
    for name in ("tensornetwork_qir", "Expectation", "Probability", "Cost", "ContractionOptions"):
        assert name in qdk.simulation.__all__


def test_expectation_normalizes_terms():
    query = Expectation([("ZZ", [0, 1], 0.5), ("X", (2,), 1)])
    assert query.terms == (("ZZ", (0, 1), 0.5 + 0j), ("X", (2,), 1 + 0j))


@pytest.mark.parametrize(
    "term",
    [
        ("ZA", [0, 1], 1.0),
        ("", [], 1.0),
        ("ZZ", [0], 1.0),
        ("ZZ", [1, 1], 1.0),
        ("Z", [-1], 1.0),
        ("Z", [True], 1.0),
    ],
)
def test_expectation_rejects_invalid_terms(term: Any):
    with pytest.raises(ValueError):
        Expectation([term])


@pytest.mark.parametrize("term", [("Z", [0]), ("Z", [0], "1"), ("Z", [0], True)])
def test_expectation_rejects_malformed_terms(term: Any):
    with pytest.raises(TypeError):
        Expectation([term])


def test_expectation_requires_a_term():
    with pytest.raises(ValueError, match="at least one term"):
        Expectation([])


def test_rejects_unknown_method():
    with pytest.raises(ValueError, match="Invalid method"):
        tensornetwork_qir(QIR, [Cost()], method=cast(Any, "dense"))


def test_rejects_options_with_contraction():
    with pytest.raises(TypeError, match='ContractionOptions .* method="contraction"'):
        tensornetwork_qir(
            QIR, [Cost()], method="contraction", options=MpsOptions(), outcomes=[0]
        )


def test_rejects_options_of_another_type():
    with pytest.raises(TypeError, match="MpsOptions"):
        tensornetwork_qir(QIR, [Cost()], method="mps", options=cast(Any, {}))


def test_rejects_contraction_options_with_mps():
    with pytest.raises(TypeError, match='MpsOptions .* method="mps"'):
        tensornetwork_qir(QIR, [Cost()], method="mps", options=ContractionOptions())


def test_contraction_rejects_options_of_another_type():
    with pytest.raises(TypeError, match="ContractionOptions"):
        tensornetwork_qir(
            QIR, [Cost()], method="contraction", options=cast(Any, {}), outcomes=[0]
        )


def test_contraction_rejects_unsupported_device():
    with pytest.raises(ValueError, match="Unsupported contraction device"):
        tensornetwork_qir(
            QIR, [Cost()], method="contraction",
            options=ContractionOptions(device=cast(Any, "cpu")), outcomes=[0],
        )


def test_rejects_unsupported_device():
    with pytest.raises(ValueError, match="Unsupported MPS device"):
        tensornetwork_qir(
            QIR, [Cost()], method="mps", options=MpsOptions(device=cast(Any, "cpu"))
        )


def test_requires_a_query():
    with pytest.raises(ValueError, match="at least one query"):
        tensornetwork_qir(QIR, [], method="mps")


def test_rejects_unknown_queries():
    with pytest.raises(TypeError, match="Unsupported query"):
        tensornetwork_qir(QIR, [cast(Any, "cost")], method="mps")


@pytest.mark.parametrize("method", ["mps", "contraction"])
def test_probability_requires_outcomes(method: Literal["mps", "contraction"]):
    with pytest.raises(ValueError, match="Probability .* requires outcomes"):
        tensornetwork_qir(QIR, [Probability()], method=method)


def test_contraction_cost_requires_outcomes():
    with pytest.raises(ValueError, match="Cost .* requires outcomes"):
        tensornetwork_qir(QIR, [Cost()], method="contraction")


@pytest.mark.parametrize("outcome", [Result.Loss, 2, "0"])
def test_rejects_invalid_outcomes(outcome: Any):
    with pytest.raises(ValueError, match=r"outcomes\[1\]"):
        tensornetwork_qir(
            QIR, [Probability()], method="mps", outcomes=[0, cast(Any, outcome)]
        )


@pytest.mark.parametrize("samples", [0, -1, True, False, 2.5, "8"])
def test_contraction_options_reject_invalid_hyper_samples(samples: Any):
    with pytest.raises(ValueError, match="hyper_samples"):
        ContractionOptions(hyper_samples=samples)


@pytest.mark.parametrize("seed", [-1, 2**31, True, False, 2.5, "17"])
def test_contraction_options_reject_invalid_seeds(seed: Any):
    with pytest.raises(ValueError, match="seed"):
        ContractionOptions(seed=seed)


@pytest.mark.parametrize(
    "options, expected",
    [
        (None, {"hyper_samples": 8, "seed": 17}),
        (ContractionOptions(), {"hyper_samples": 8, "seed": 17}),
        (ContractionOptions(hyper_samples=12), {"hyper_samples": 12, "seed": 17}),
        (ContractionOptions(seed=0), {"hyper_samples": 8, "seed": 0}),
        (ContractionOptions(hyper_samples=1, seed=2**31 - 1),
         {"hyper_samples": 1, "seed": 2**31 - 1}),
    ],
)
def test_contraction_options_fill_defaults(options, expected):
    assert _contraction_options_dict(options) == expected


def test_contraction_options_are_frozen():
    options = ContractionOptions(device="nvidia", hyper_samples=2, seed=3)
    assert options.device == "nvidia"
    with pytest.raises(FrozenInstanceError):
        setattr(options, "seed", 4)


@pytest.mark.parametrize("queries", [[Probability()], [Cost()]])
def test_contraction_rejects_open_network_before_discovery(queries):
    qir = SX_CZ_MRESETZ_BASE_QIR.replace(
        "    ret void",
        "    call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 1 to %Qubit*))\n    ret void",
    )
    with pytest.raises(ValueError, match=r"open on qubits \[1\]"):
        tensornetwork_qir(qir, queries, method="contraction", outcomes=[0, 0])


@pytest.mark.parametrize("queries", [[Probability()], [Cost()]])
def test_contraction_rejects_failing_selection_record_before_discovery(queries):
    qir, _ = stim.compile("SELECT {\n M 0\n REQUIRE rec[-1]\n}\n")
    with pytest.raises(ValueError, match="result 0"):
        tensornetwork_qir(qir, queries, method="contraction", outcomes=[1])


def test_contraction_rejects_unsupported_gate_before_discovery():
    with pytest.raises(ValueError, match="unsupported.*H"):
        tensornetwork_qir(
            BELL_BASE_QIR, [Probability()], method="contraction", outcomes=[0, 0]
        )


def test_contraction_options_reach_native_validation():
    with pytest.raises(ValueError, match="hyper_samples"):
        tensornetwork_qir(
            SX_CZ_MRESETZ_BASE_QIR, [Cost()], method="contraction", outcomes=[0, 0],
            options=ContractionOptions(hyper_samples=2**31),
        )


@pytest.mark.parametrize("missing", ["hyper_samples", "seed"])
def test_contraction_native_options_require_every_key(missing: str):
    options = _contraction_options_dict(None)
    del options[missing]
    with pytest.raises(ValueError, match=f"options must include {missing}"):
        _native._tensor_network_contraction_query(
            _adaptive_program(SX_CZ_MRESETZ_BASE_QIR), [{"kind": "cost"}],
            [False, False], options,
        )


def test_contraction_probe_rejects_unknown_cap():
    with pytest.raises(ValueError, match="result 2"):
        _native._fixed_outcome_contraction_probe(
            _adaptive_program(SX_CZ_MRESETZ_BASE_QIR), [False, False], 2,
            _contraction_options_dict(None),
        )


def test_closed_contraction_reports_unavailable_libraries_as_oserror(tmp_path: Path):
    # Process construction injects the supported discovery override without
    # mutating this process's environment or requiring any particular GPU host.
    script = """
import platform
import sys
from qdk.simulation import Cost, Probability, tensornetwork_qir
qir = sys.stdin.read()
for queries in ([Cost()], [Probability()], [Probability(), Cost()]):
    try:
        tensornetwork_qir(qir, queries, method="contraction", outcomes=[0, 0])
    except OSError as error:
        expected = ("QDK_CUTENSORNET_LIBRARY"
                    if sys.platform == "linux" and platform.machine() == "x86_64"
                    else "unsupported on")
        assert expected in str(error), str(error)
    else:
        raise AssertionError("missing library must not be a successful query")
"""
    result = subprocess.run(
        [sys.executable, "-c", script],
        input=SX_CZ_MRESETZ_BASE_QIR,
        text=True,
        capture_output=True,
        env={**os.environ, "QDK_CUTENSORNET_LIBRARY": str(tmp_path / "not-installed.so")},
    )
    assert result.returncode == 0, result.stderr


def test_state_query_rejects_invalid_native_terms_before_discovery():
    with pytest.raises(ValueError, match="unknown Pauli label"):
        _native._tensor_network_state_query(
            _adaptive_program(BELL_BASE_QIR),
            [{"kind": "expectation", "terms": [("ZQ", [0, 1], 1 + 0j)]}],
            None, {"max_bond_dimension": None},
        )


def test_state_native_cost_requires_mps():
    with pytest.raises(ValueError, match="requires an MPS"):
        _native._tensor_network_state_query(
            _adaptive_program(BELL_BASE_QIR), [{"kind": "cost"}], None, None
        )


def test_state_native_probability_requires_mps():
    with pytest.raises(ValueError, match="Probability on a cuTensorNet state requires an MPS"):
        _native._tensor_network_state_query(
            _adaptive_program(BELL_BASE_QIR), [{"kind": "probability"}], [False, False], None
        )


def test_state_native_probability_requires_outcomes():
    with pytest.raises(ValueError, match="Probability requires outcomes"):
        _native._tensor_network_state_query(
            _adaptive_program(BELL_BASE_QIR), [{"kind": "probability"}], None,
            {"max_bond_dimension": None},
        )


@pytest.mark.parametrize(
    "queries",
    [
        [Probability(), Expectation([("Z", [0], 1)])],
        [Expectation([("Z", [0], 1)]), Cost(), Probability()],
    ],
)
def test_mps_probability_rejects_expectation_in_the_same_call(queries):
    with pytest.raises(ValueError, match="Probability and Expectation read different MPS states"):
        tensornetwork_qir(BELL_BASE_QIR, queries, method="mps", outcomes=[0, 0])


@pytest.mark.parametrize("queries", [[Probability()], [Probability(), Cost()]])
def test_mps_probability_rejects_failing_selection_record_before_discovery(queries):
    qir, _ = stim.compile("SELECT {\n M 0\n REQUIRE rec[-1]\n}\n")
    with pytest.raises(ValueError, match="result 0"):
        tensornetwork_qir(qir, queries, method="mps", outcomes=[1])


def test_mps_probability_rejects_unsupported_gate_before_discovery():
    qir = BELL_BASE_QIR.replace("__quantum__qis__h__body", "__quantum__qis__y__body")
    with pytest.raises(ValueError, match="unitary operation Y is not supported"):
        tensornetwork_qir(qir, [Probability()], method="mps", outcomes=[0, 0])


@pytest.mark.parametrize("method", ["mps", "contraction"])
def test_expectation_rejects_an_absent_qubit_before_discovery(
    method: Literal["mps", "contraction"],
):
    with pytest.raises(ValueError, match="qubit 5, but the program has 2 qubits"):
        tensornetwork_qir(BELL_BASE_QIR, [Expectation([("Z", [5], 1)])], method=method)


@pytest.mark.parametrize("method", ["mps", "contraction"])
def test_expectation_rejects_reset_before_discovery(method: Literal["mps", "contraction"]):
    with pytest.raises(ValueError, match="reset"):
        tensornetwork_qir(
            X_RESET_MEASURE_BASE_QIR, [Expectation([("Z", [0], 1)])], method=method
        )


def test_state_queries_report_unavailable_libraries_as_oserror(tmp_path: Path):
    # Same discovery override as the contraction test above, in a child process.
    # MPS Probability also runs on a program with measure-and-reset and on one
    # with a selection branch, which the Expectation route rejects before
    # discovery.
    script = """
import json
import platform
import sys
from qdk import stim
from qdk.simulation import Cost, Expectation, Probability, tensornetwork_qir
programs = json.loads(sys.stdin.read())
select, _ = stim.compile("SELECT {\\n M 0\\n REQUIRE rec[-1]\\n}\\n")
bell, mresetz = programs["bell"], programs["mresetz"]
zz = Expectation([("ZZ", [0, 1], 1)])
cases = (
    (bell, [zz], "mps", None),
    (bell, [Cost()], "mps", None),
    (bell, [zz], "contraction", None),
    (bell, [Probability()], "mps", [0, 0]),
    (mresetz, [Probability(), Cost()], "mps", [0, 0]),
    (select, [Cost(), Probability()], "mps", [0]),
)
for qir, queries, method, outcomes in cases:
    try:
        tensornetwork_qir(qir, queries, method=method, outcomes=outcomes)
    except OSError as error:
        expected = ("QDK_CUTENSORNET_LIBRARY"
                    if sys.platform == "linux" and platform.machine() == "x86_64"
                    else "unsupported on")
        assert expected in str(error), str(error)
    else:
        raise AssertionError("missing library must not be a successful query")
"""
    result = subprocess.run(
        [sys.executable, "-c", script],
        input=json.dumps({"bell": BELL_BASE_QIR, "mresetz": SX_CZ_MRESETZ_BASE_QIR}),
        text=True,
        capture_output=True,
        env={**os.environ, "QDK_CUTENSORNET_LIBRARY": str(tmp_path / "not-installed.so")},
    )
    assert result.returncode == 0, result.stderr


@pytest.mark.skipif(not NVIDIA_MPS_AVAILABLE, reason=NVIDIA_MPS_SKIP_REASON)
@pytest.mark.parametrize("method", ["mps", "contraction"])
def test_expectations_match_bell_and_single_qubit_states(
    method: Literal["mps", "contraction"],
):
    zz, xx, yy, z0, weighted = tensornetwork_qir(
        BELL_BASE_QIR,
        [
            Expectation([("ZZ", [0, 1], 1)]),
            Expectation([("XX", [0, 1], 1)]),
            Expectation([("YY", [0, 1], 1)]),
            Expectation([("Z", [0], 1)]),
            Expectation([("II", [0, 1], 0.5), ("ZZ", [0, 1], 2j)]),
        ],
        method=method,
    )
    assert zz == pytest.approx(1)
    assert xx == pytest.approx(1)
    assert yy == pytest.approx(-1)
    assert z0 == pytest.approx(0, abs=1e-12)
    assert weighted == pytest.approx(0.5 + 2j)
    y0, x1 = tensornetwork_qir(
        RX_H_BASE_QIR,
        [Expectation([("Y", [0], 1)]), Expectation([("X", [1], 1)])],
        method=method,
    )
    assert y0 == pytest.approx(-1)
    assert x1 == pytest.approx(1)


@pytest.mark.skipif(not NVIDIA_MPS_AVAILABLE, reason=NVIDIA_MPS_SKIP_REASON)
def test_mps_cost_reports_the_realized_bond_and_preserves_query_order():
    cost, zz, cost_again = tensornetwork_qir(
        BELL_BASE_QIR,
        [Cost(), Expectation([("ZZ", [0, 1], 1)]), Cost()],
        method="mps", options=MpsOptions(max_bond_dimension=4),
    )
    assert zz == pytest.approx(1)
    assert cost == cost_again
    assert set(cost) == {"max_bond_dimension", "state_bytes", "workspace_bytes"}
    # Two sites of extents [2, 2] at 16 bytes per complex128 element.
    assert cost["max_bond_dimension"] == 2
    assert cost["state_bytes"] == 128
    assert cost["workspace_bytes"] > 0
    capped, = tensornetwork_qir(
        BELL_BASE_QIR, [Cost()], method="mps", options=MpsOptions(max_bond_dimension=1)
    )
    assert capped["max_bond_dimension"] == 1


@pytest.mark.skipif(not NVIDIA_MPS_AVAILABLE, reason=NVIDIA_MPS_SKIP_REASON)
@pytest.mark.parametrize("outcomes", [[0, 0], [0, 1], [1, 0], [1, 1]])
def test_mps_probability_and_cost_preserve_query_order(outcomes):
    # Sx on both qubits and Cz leave every record equally likely, and both
    # measurements reset: P = 1/4 through the |0⟩⟨b| operators.
    first_cost, probability, second_cost, second_probability = tensornetwork_qir(
        SX_CZ_MRESETZ_BASE_QIR,
        [Cost(), Probability(), Cost(), Probability()],
        method="mps", outcomes=outcomes, options=MpsOptions(max_bond_dimension=4),
    )
    assert probability == pytest.approx(0.25)
    assert second_probability == probability
    assert first_cost == second_cost
    assert set(first_cost) == {"max_bond_dimension", "state_bytes", "workspace_bytes"}
    assert 1 <= first_cost["max_bond_dimension"] <= 4


@pytest.mark.skipif(not NVIDIA_MPS_AVAILABLE, reason=NVIDIA_MPS_SKIP_REASON)
def test_mps_probability_follows_a_selection_branch():
    # The MPS route needs at least two qubits, so qubit 1 is measured too.
    qir, _ = stim.compile("SELECT {\n M 0\n REQUIRE rec[-1]\n}\nM 1\n")
    probability, = tensornetwork_qir(qir, [Probability()], method="mps", outcomes=[0, 0])
    assert probability == pytest.approx(1.0)


@pytest.mark.skipif(not NVIDIA_MPS_AVAILABLE, reason=NVIDIA_MPS_SKIP_REASON)
@pytest.mark.parametrize("outcomes", [[0, 0], [0, 1], [1, 0], [1, 1]])
def test_contraction_probability_and_cost_preserve_query_order(outcomes):
    options = ContractionOptions(hyper_samples=8, seed=17)
    first_cost, probability, second_cost, second_probability = tensornetwork_qir(
        SX_CZ_MRESETZ_BASE_QIR,
        [Cost(), Probability(), Cost(), Probability()],
        method="contraction", outcomes=outcomes, options=options,
    )
    assert probability == pytest.approx(0.25)
    assert second_probability == probability
    assert first_cost == second_cost
    assert set(first_cost) == {"width", "flops", "workspace_bytes"}
    assert first_cost["width"] >= 0
    assert first_cost["flops"] > 0
    assert first_cost["workspace_bytes"] >= 0
    cost_only, = tensornetwork_qir(
        SX_CZ_MRESETZ_BASE_QIR, [Cost()], method="contraction",
        outcomes=outcomes, options=options,
    )
    assert set(cost_only) == set(first_cost)


@pytest.mark.skipif(not NVIDIA_MPS_AVAILABLE, reason=NVIDIA_MPS_SKIP_REASON)
def test_contraction_probe_flips_a_deterministic_cap_without_restarting():
    qir, _ = stim.compile("SELECT {\n M 0\n REQUIRE rec[-1]\n}\n")
    probability, = tensornetwork_qir(
        qir, [Probability()], method="contraction", outcomes=[0]
    )
    assert probability == pytest.approx(1.0)
    assert _native._fixed_outcome_contraction_probe(
        _adaptive_program(qir), [False], 0, _contraction_options_dict(None)
    ) == 0.0
