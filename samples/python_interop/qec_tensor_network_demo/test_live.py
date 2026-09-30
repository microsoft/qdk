# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""live.py on the host (no GPU): tensornetwork_qir is a test double."""

import json

import pytest

import live

M = 3
RECORDS = {"r0": "0110", "r1": "0011", "r0_flip1": "0010", "r0_flip3": "0111"}
EXPECTED = {"r0": 2.0**-M, "r1": 2.0**-M, "r0_flip1": 0.0, "r0_flip3": 0.0}


class TestDoubleTensorNetwork:
    """Answers P(r) from a table keyed by the record bits; can fail on chosen records."""

    __test__ = False

    def __init__(self, probabilities=None, fail=None):
        self.calls = []
        self.probabilities = probabilities or {}
        self.fail = fail or {}

    def __call__(self, qir, queries, *, method, options=None, outcomes=None):
        bits = "".join("1" if bit else "0" for bit in outcomes)
        name = next(n for n, r in RECORDS.items() if r == bits)
        self.calls.append({"qir": qir, "queries": [type(q).__name__ for q in queries], "method": method,
                           "options": options, "record": name})
        if name in self.fail:
            raise self.fail[name]
        p = self.probabilities.get(name, EXPECTED[name])
        if method == "mps":
            cost = {"max_bond_dimension": options.max_bond_dimension, "state_bytes": 1 << 20, "workspace_bytes": 0}
        else:
            cost = {"width": 27.0, "flops": 1e10, "workspace_bytes": 32 * 2**27}
        return [p, cost]


class Clock:
    def __init__(self, step):
        self.now, self.step = 0.0, step

    def __call__(self):
        self.now += self.step
        return self.now


@pytest.fixture
def inputs(tmp_path):
    circuit = tmp_path / "n.ll"
    circuit.write_text("; qir")
    records = tmp_path / "host.json"
    records.write_text(json.dumps({"m": M, "records": RECORDS, "expected": EXPECTED}))
    return ["--circuit", str(circuit), "--records", str(records)]


def test_default_is_contraction_of_the_first_valid_and_flipped_records(inputs, capsys):
    double = TestDoubleTensorNetwork()
    assert live.main(inputs, tensornetwork_qir=double, clock=Clock(12.0)) == 0

    assert [call["record"] for call in double.calls] == ["r0", "r0_flip1"]
    call = double.calls[0]
    assert call["qir"] == "; qir"
    assert call["method"] == "contraction"
    assert call["queries"] == ["Probability", "Cost"]
    assert (call["options"].hyper_samples, call["options"].seed) == (8, 17)
    output = capsys.readouterr().out
    assert "m = 3 random outcomes, valid P(r) = 2^-3" in output
    assert "P(r) = 1.250000e-01 = 2^-3.00 ✓   width 27, workspace 4.0 GiB   12 s" in output
    assert "P(r) = 0 ✓" in output


def test_mps_uses_the_requested_chi_and_chosen_records(inputs, capsys):
    double = TestDoubleTensorNetwork()
    argv = [*inputs, "--method", "mps", "--chi", "64", "--record", "r1", "r0_flip3"]
    assert live.main(argv, tensornetwork_qir=double, clock=Clock(1.0)) == 0

    assert [call["record"] for call in double.calls] == ["r1", "r0_flip3"]
    assert all(call["options"].max_bond_dimension == 64 for call in double.calls)
    assert "bond 64, state 1.0 MiB" in capsys.readouterr().out


def test_inexact_probability_is_marked_and_fails(inputs, capsys):
    double = TestDoubleTensorNetwork(probabilities={"r0": 0.126, "r0_flip1": 1e-6})
    assert live.main(inputs, tensornetwork_qir=double, clock=Clock(1.0)) == 1
    output = capsys.readouterr().out
    assert "2^-2.99 ✗" in output
    assert "P(r) = 1.000000e-06 = 2^-19.93 ✗" in output


def test_a_failed_call_is_reported_and_the_next_record_still_runs(inputs, capsys):
    double = TestDoubleTensorNetwork(fail={"r0": OSError("out of device memory")})
    assert live.main(inputs, tensornetwork_qir=double, clock=Clock(3.0)) == 1

    assert [call["record"] for call in double.calls] == ["r0", "r0_flip1"]
    output = capsys.readouterr().out
    assert "failed after 3.0 s: OSError: out of device memory" in output
    assert "P(r) = 0 ✓" in output


def test_unknown_record_is_rejected_before_any_call(inputs, capsys):
    double = TestDoubleTensorNetwork()
    with pytest.raises(SystemExit):
        live.main([*inputs, "--record", "r7"], tensornetwork_qir=double)
    assert double.calls == []
    assert "no record 'r7'" in capsys.readouterr().err
