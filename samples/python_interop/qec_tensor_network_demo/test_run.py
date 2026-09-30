# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""run.py render on the host (no GPU): synthetic probe results in, table and plots out."""

import json

import pytest

import run

M = {1: 31, 2: 53, 3: 75}
GPU = "NVIDIA A100 80GB PCIe, 580.00, 81920 MiB"


def case(k, method, record="r0", *, status="ok", probability=None, chi=None, hyper=8, seed=17,
         queries=("probability", "cost"), cost=None, wall=1.0, peak=None):
    valid = 2.0 ** -M[k]
    expected = 0.0 if "flip" in record else valid
    row = {"input": f"fi_k{k}", "method": method, "record": record, "queries": list(queries),
           "expected": expected, "expected_valid": valid, "status": status, "wall_seconds": wall,
           "cost": cost or {}}
    if status == "ok" and "probability" in queries:
        row["probability"] = expected if probability is None else probability
    if method == "mps":
        row["chi"] = chi
    else:
        row["hyper_samples"], row["seed"] = hyper, seed
    if peak is not None:
        row["gpu_mem_mib_peak"] = peak
    return row


def write(tmp_path, name, cases, gpu=GPU):
    document = {"inputs": {f"fi_k{k}": {"qubits": 78 + 52 * (k - 1), "m": m} for k, m in M.items()},
                "cases": cases, "environment": {"gpu": gpu}}
    path = tmp_path / name
    path.write_text(json.dumps(document))
    return path


def contraction_cost(width):
    return {"width": float(width), "flops": 1e10, "workspace_bytes": 32 * 2**width}


def rows(output):
    """Rows of the first (per-round) table keyed by k, as lists of cells."""
    table = {}
    for line in output.split("\n\n", 1)[0].splitlines():
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        if cells[0].isdigit():
            table[int(cells[0])] = cells
    return table


@pytest.fixture
def results(tmp_path):
    first = write(tmp_path, "a.json", [
        case(1, "contraction", queries=["cost"], hyper=64, cost=contraction_cost(28)),
        case(1, "contraction", queries=["cost"], cost=contraction_cost(27)),
        case(1, "contraction", cost=contraction_cost(27), wall=3.0),
        case(1, "contraction", "r0_flip21", cost=contraction_cost(27)),
        case(2, "contraction", queries=["cost"], cost=contraction_cost(32)),
        case(1, "mps", chi=256, probability=2.0**-31 / 81, cost={"max_bond_dimension": 256}),
        case(1, "mps", chi=512, cost={"max_bond_dimension": 512, "state_bytes": 400 << 20}, wall=266, peak=1228),
    ])
    second = write(tmp_path, "b.json", [
        case(1, "mps", chi=2048, status="timeout", wall=900),
        case(2, "mps", chi=512, status="exit 1", wall=500),
        {**case(3, "contraction"), "status": "skipped"},
    ])
    return [first, second]


def test_table_reports_each_round_against_its_reference(results, capsys):
    assert run.main(["render", *map(str, results), "--no-plots"]) == 0
    table = rows(capsys.readouterr().out)

    assert sorted(table) == [1, 2, 3]
    k1 = table[1]
    assert k1[:6] == ["1", "78", "31", "2^-31", "27", "4.0 GiB"]
    assert k1[6] == "2^-31 ✓ (3.0 s)"
    assert k1[7] == "0 ✓ (1.0 s)"
    assert k1[8:12] == ["✓ rel. err. 0.0e+00", "400.0 MiB", "1.2 GiB", "266 s"]
    k2 = table[2]
    assert k2[4:8] == ["32", "128.0 GiB", "not run: 128.0 GiB > GPU", "not run: 128.0 GiB > GPU"]
    assert k2[8] == "error"
    assert table[3][6:9] == ["not run", "not run", "not run"]


def test_default_contraction_options_are_preferred_for_the_plan(results, capsys):
    run.main(["render", str(results[0]), "--no-plots"])
    assert rows(capsys.readouterr().out)[1][4] == "27"


def test_chi_sweep_is_judged_against_the_reference(results, capsys):
    run.main(["render", *map(str, results), "--no-plots"])
    output = capsys.readouterr().out

    assert "MPS χ sweep, k=1 (P(r) = 2^-31)" in output
    assert "| 256 | 256 | 0.0123 | wrong | 1.0 s |" in output
    assert "| 512 | 512 | 1 | exact | 266 s |" in output
    assert "| 2048 | — | timeout | timeout | 15 min |" in output


def test_search_effort_is_tabulated_per_hyper_samples_and_seed(tmp_path, capsys):
    def plan(hyper, seed, width, wall):
        row = case(1, "contraction", queries=["cost"], hyper=hyper, seed=seed, cost=contraction_cost(width), wall=wall)
        row["cost"]["flops"] = 9.5e12
        return row

    path = write(tmp_path, "s.json", [plan(8, 17, 35, 73), plan(8, 2, 36, 91), plan(512, 17, 35, 439)])
    run.main(["render", str(path), "--no-plots"])
    output = capsys.readouterr().out

    assert "Contraction path search, k=1: width w, flops, planning time" in output
    assert "| hyper_samples | seed 17 | seed 2 |" in output
    assert "| 8 | w 35, 9.5e+12 flops, 73 s | w 36, 9.5e+12 flops, 91 s |" in output
    assert "| 512 | w 35, 9.5e+12 flops, 439 s | — |" in output


def test_single_plan_has_no_search_table(results, capsys):
    run.main(["render", str(results[1]), "--no-plots"])
    assert "Contraction path search" not in capsys.readouterr().out


def test_wrong_exact_contraction_is_marked(tmp_path, capsys):
    path = write(tmp_path, "w.json", [
        case(1, "contraction", cost=contraction_cost(27), probability=2.0**-30),
        case(1, "contraction", "r0_flip21", cost=contraction_cost(27), probability=1e-12),
    ])
    run.main(["render", str(path), "--no-plots"])
    k1 = rows(capsys.readouterr().out)[1]
    assert k1[6].startswith("2^-30 ✗")
    assert k1[7].startswith("1e-12 ✗")


def test_cotengra_widths_are_added_per_objective(results, tmp_path, capsys):
    cotengra = tmp_path / "cotengra.jsonl"
    cotengra.write_text("\n".join(json.dumps(r) for r in [
        {"k": 1, "objective": "flops", "width": 21.0},
        {"k": 1, "objective": "size", "width": 20.0},
        {"k": 2, "objective": "flops", "width": 24.0},
        {"k": 9, "objective": "flops", "width": 41.0},
    ]))
    run.main(["render", *map(str, results), "--cotengra", str(cotengra), "--no-plots"])
    table = rows(capsys.readouterr().out)
    assert table[1][12] == "21/20"
    assert table[2][12] == "24/—"
    assert 9 not in table


def test_plots_are_written_next_to_the_output_stem(results, tmp_path, capsys):
    pytest.importorskip("matplotlib")
    stem = tmp_path / "report"
    run.main(["render", *map(str, results), "--output", str(stem)])
    assert "plots:" in capsys.readouterr().out
    assert (tmp_path / "report.qec-memory.png").stat().st_size > 0
    assert (tmp_path / "report.qec-time.png").stat().st_size > 0
