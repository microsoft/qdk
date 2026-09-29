# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""run.py measure/render on the host (preview qdk build; no GPU).

``TestDoubleTensorNetwork`` stands in for ``tensornetwork_qir``: it evaluates
the requested ``Expectation`` terms on the frozen 4×4 Case A state, so the
exact values it returns are only right if run.py asks for the right
observables, and it adds a known truncation error 0.1/χ² for method="mps".
"""

import json

import numpy as np
import pytest
from qdk.simulation import Cost, Expectation, MpsOptions

import run
from ising import CASE_A, z_expectation


CASE_A_DISTRIBUTION = np.abs(np.load(CASE_A / "amplitudes.npy", allow_pickle=False)) ** 2
CASE_A_DISTRIBUTION /= CASE_A_DISTRIBUTION.sum()


class TestDoubleTensorNetwork:
    __test__ = False

    def __init__(self, fail_exact=None, fail_chis=()):
        self.calls = []
        self.fail_exact = fail_exact
        self.fail_chis = set(fail_chis)

    @staticmethod
    def truncation(chi):
        return 0.1 / chi**2

    def __call__(self, qir, queries, *, method, options=None, outcomes=None):
        self.calls.append({"qir": qir, "queries": list(queries), "method": method, "options": options})
        if method == "contraction" and self.fail_exact is not None:
            raise self.fail_exact
        chi = None
        if method == "mps":
            assert options is not None and options.max_bond_dimension is not None
            chi = options.max_bond_dimension
        if chi in self.fail_chis:
            raise OSError(f"out of device memory at χ={chi}")
        error = 0.0 if chi is None else self.truncation(chi)
        results = []
        for query in queries:
            if isinstance(query, Expectation):
                results.append(complex(z_expectation(CASE_A_DISTRIBUTION, query.terms)) + error)
            elif isinstance(query, Cost) and chi is not None:
                results.append({"max_bond_dimension": min(chi, 64), "state_bytes": 32 * chi * chi, "workspace_bytes": 3 << 20})
            else:
                raise AssertionError(f"unexpected query {query!r}")
        return results


def measure_case_a(**kwargs):
    double = kwargs.pop("double", TestDoubleTensorNetwork())
    runs = list(run.measure([(4, 0.5)], kwargs.pop("chis", [2, 4, 8, 16, 32]), tensornetwork_qir=double, **kwargs))
    assert len(runs) == 1
    return runs[0], double


def test_measure_asks_for_the_observables_that_reproduce_the_oracle():
    result, double = measure_case_a()
    assert round(result["exact"]["m_z"], 7) == 0.9508364
    assert round(result["exact"]["c_zz"], 7) == 0.9326830
    assert result["cpu_reference"]["m_z"] == pytest.approx(result["exact"]["m_z"], abs=1e-12)
    assert result["circuit"] == "fixtures/case_a_4x4/measured.ll"
    assert double.calls[0]["qir"] == (CASE_A / "measured.ll").read_text()


def test_measure_runs_exact_once_then_mps_with_cost_per_chi():
    result, double = measure_case_a(chis=[4, 16])
    assert [(call["method"], call["options"]) for call in double.calls] == [
        ("contraction", None),
        ("mps", MpsOptions(max_bond_dimension=4)),
        ("mps", MpsOptions(max_bond_dimension=16)),
    ]
    assert [len(call["queries"]) for call in double.calls] == [2, 3, 3]
    assert isinstance(double.calls[1]["queries"][2], Cost)
    assert [entry["chi"] for entry in result["mps"]] == [4, 16]
    assert result["mps"][1]["cost"] == {"max_bond_dimension": 16, "state_bytes": 8192, "workspace_bytes": 3 << 20}
    assert result["mps"][0]["m_z"] - result["exact"]["m_z"] == pytest.approx(0.1 / 16)


def test_measure_records_timing_with_the_injected_clock():
    ticks = iter(range(100))
    result, _ = measure_case_a(chis=[2], clock=lambda: float(next(ticks)))
    assert result["exact"]["seconds"] == 1.0
    assert result["mps"][0]["seconds"] == 1.0


def test_measure_without_exact_uses_only_mps():
    result, double = measure_case_a(exact=False)
    assert result["exact"] is None
    assert {call["method"] for call in double.calls} == {"mps"}
    assert run.reference(result)[0] == "MPS χ=32"


def test_chi_needed_is_the_smallest_chi_within_the_threshold():
    result, _ = measure_case_a()
    # ε(χ) = 0.1/χ²: 1.6e-3 at χ=8, 3.9e-4 at χ=16.
    assert run.chi_needed(result) == 16
    assert run.chi_needed(result, threshold=1e-5) is None


def test_exact_out_of_resources_is_recorded_and_the_largest_chi_becomes_the_reference():
    result, double = measure_case_a(double=TestDoubleTensorNetwork(fail_exact=OSError("workspace exceeds the device")))
    assert result["exact"]["error"] == "OSError: workspace exceeds the device"
    assert len(result["mps"]) == 5
    label, ref = run.reference(result)
    assert label == "MPS χ=32"
    assert ref is result["mps"][-1]
    # Against χ=32 (ε=9.8e-5), χ=16 differs by 3.9e-4 − 9.8e-5 ≈ 2.9e-4.
    assert run.chi_needed(result) == 16
    text = run.render_run(result)
    assert "reference: MPS χ=32" in text
    assert "reference: exact" not in text
    assert "exact reference did not fit: OSError: workspace exceeds the device" in text


def test_a_failed_chi_is_reported_but_not_used():
    result, _ = measure_case_a(double=TestDoubleTensorNetwork(fail_chis=[32]))
    assert result["mps"][-1]["error"] == "OSError: out of device memory at χ=32"
    text = run.render_run(result)
    assert "32    error: OSError: out of device memory at χ=32" in text
    assert run.chi_needed(result) == 16


def test_program_errors_propagate():
    double = TestDoubleTensorNetwork(fail_exact=ValueError("unsupported gate"))
    with pytest.raises(ValueError, match="unsupported gate"):
        measure_case_a(double=double)


def test_render_shows_errors_cost_and_the_cpu_check():
    result, _ = measure_case_a(chis=[8, 16], clock=iter(np.arange(0, 10, 0.25)).__next__)
    text = run.render_run(result)
    lines = text.splitlines()
    assert lines[0] == "ising2d | size=4 J=1 h=0.5 | reference: exact (0.25 s)"
    assert lines[1] == "CPU check: |exact − CPU| m_z 0.0e+00, C_ZZ 0.0e+00 (limit 1e-08): ok"
    assert lines[2].split() == ["χ", "ε_m", "ε_C", "max", "bond", "time", "state", "workspace"]
    assert lines[3].split() == ["8", "1.6e-03", "1.6e-03", "8", "0.25", "s", "2.0", "KiB", "3.0", "MiB"]
    assert lines[4].split() == ["16", "3.9e-04", "3.9e-04", "16", "0.25", "s", "8.0", "KiB", "3.0", "MiB"]


def test_render_flags_an_exact_result_that_disagrees_with_the_cpu_reference():
    result, _ = measure_case_a(chis=[16])
    result["cpu_reference"] = {"m_z": result["exact"]["m_z"] + 2e-8, "c_zz": result["exact"]["c_zz"]}
    assert run.render_run(result).splitlines()[1].endswith("(limit 1e-08): MISMATCH")


def test_summary_marks_unreached_and_missing_cases():
    reached, _ = measure_case_a()
    unreached, _ = measure_case_a(chis=[2, 4])
    unreached = {**unreached, "field": 3.03}
    lines = run.render_summary([reached, unreached]).splitlines()
    assert lines[0] == "χ needed: smallest χ with ε_m and ε_C ≤ 0.001"
    assert lines[2].split() == ["4", "16", "exact", "16", ">", "4"]
    only_one_field = run.render_summary([reached]).splitlines()[2].split()
    assert only_one_field == ["4", "16", "exact", "16", "—"]


def test_measure_command_keeps_other_sections_and_replaces_a_rerun(tmp_path, capsys):
    output = tmp_path / "results.json"
    output.write_text(json.dumps({"qec": {"rows": [1, 2, 3]}}))
    arguments = ["measure", "--size", "4", "--field", "0.5", "--chi", "16", "4", "4", "--output", str(output)]
    assert run.main(arguments, tensornetwork_qir=TestDoubleTensorNetwork()) == 0
    assert run.main(arguments, tensornetwork_qir=TestDoubleTensorNetwork()) == 0
    document = json.loads(output.read_text())
    assert document["qec"] == {"rows": [1, 2, 3]}
    assert document["ising2d"]["schema"] == run.SCHEMA
    assert len(document["ising2d"]["runs"]) == 1
    assert [entry["chi"] for entry in document["ising2d"]["runs"][0]["mps"]] == [4, 16]
    assert "wrote 1 run(s)" in capsys.readouterr().out


def test_render_command_prints_tables_from_the_results_file(tmp_path, capsys):
    output = tmp_path / "results.json"
    run.main(["measure", "--size", "4", "--field", "0.5", "--chi", "8", "16", "--output", str(output)],
             tensornetwork_qir=TestDoubleTensorNetwork())
    capsys.readouterr()
    assert run.main(["render", str(output), "--no-plots"]) == 0
    out = capsys.readouterr().out
    assert "ising2d | size=4 J=1 h=0.5 | reference: exact" in out
    assert "χ needed: smallest χ with ε_m and ε_C ≤ 0.001" in out


def test_render_command_writes_plots(tmp_path, capsys):
    pytest.importorskip("matplotlib")
    output = tmp_path / "results.json"
    run.main(["measure", "--size", "4", "--field", "0.5", "--chi", "8", "16", "--output", str(output)],
             tensornetwork_qir=TestDoubleTensorNetwork())
    assert run.main(["render", str(output)]) == 0
    assert (tmp_path / "results.ising2d-error.png").stat().st_size > 0
    assert (tmp_path / "results.ising2d-chi-needed.png").stat().st_size > 0


def test_render_rejects_a_file_without_results(tmp_path):
    output = tmp_path / "results.json"
    output.write_text(json.dumps({"qec": {}}))
    with pytest.raises(ValueError, match="No 'ising2d' results"):
        run.main(["render", str(output), "--no-plots"])


@pytest.mark.parametrize("arguments,message", [
    (["--size", "7", "--field", "0.5"], "no frozen circuit for --size 7"),
    (["--size", "4", "--field", "1"], "no frozen circuit for --field 1"),
    (["--size", "4", "--field", "0.5", "--chi", "0"], "χ must be a positive integer: '0'"),
    (["--size", "4", "--field", "0.5", "--chi", "x"], "χ must be a positive integer: 'x'"),
    (["--field", "0.5"], "--size"),
])
def test_measure_rejects_bad_arguments_before_any_evaluation(tmp_path, capsys, arguments, message):
    double = TestDoubleTensorNetwork()
    output = tmp_path / "results.json"
    with pytest.raises(SystemExit) as exit_info:
        run.main(["measure", *arguments, "--output", str(output)], tensornetwork_qir=double)
    assert exit_info.value.code == 2
    assert message in capsys.readouterr().err
    assert double.calls == []
    assert not output.exists()


def test_measure_rejects_an_incompatible_results_file_before_any_evaluation(tmp_path):
    output = tmp_path / "results.json"
    original = json.dumps({"ising2d": {"schema": 99, "runs": []}})
    output.write_text(original)
    double = TestDoubleTensorNetwork()
    with pytest.raises(ValueError, match="unsupported 'ising2d' schema"):
        run.main(["measure", "--size", "4", "--field", "0.5", "--output", str(output)], tensornetwork_qir=double)
    assert double.calls == []
    assert output.read_text() == original
