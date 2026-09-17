# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Freeze or verify the bounded 4x4 Case A input and pre-measurement CPU oracle."""

from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
from importlib import metadata
from io import BytesIO
import json
from pathlib import Path
import platform
import subprocess
import time

import numpy as np
import pyqir
from qdk import qsharp
from qdk._native import QirInstructionId as Op
from qdk.simulation._simulation import AggregateGatesPass

from build_measured_circuit import (
    ParsedCircuit,
    build_ising_2d_qir,
    compile_measured_qir,
    ising_lattice,
    parse_gates,
    qsharp_source,
)


CASE_A = dict(nx=4, ny=4, total_time=1.0, order=4, num_divisions=2)
NORM_LIMIT = 1e-8
REFERENCE_LIMIT = 1e-12
COMPARISON_LIMIT = 1e-8
DIRECTORY = Path(__file__).resolve().parent


def qir_instructions(qir: str, *, measured: bool) -> tuple[list, int, int]:
    """Use QDK's collector, admitting only this fixture's straight-line shape."""
    module = pyqir.Module.from_ir(pyqir.Context(), qir)
    error = module.verify()
    if error is not None:
        raise ValueError(f"Invalid QIR: {error}")
    functions = [function for function in module.functions if function.basic_blocks]
    if len(functions) != 1 or not pyqir.is_entry_point(functions[0]):
        raise ValueError("Expected one defined entry-point function")
    entry = functions[0]
    if len(entry.basic_blocks) != 1:
        raise ValueError("Expected a single straight-line basic block")
    if measured:
        attributes = entry.attributes.func
        if "qir_profiles" not in attributes or attributes["qir_profiles"].string_value != "base_profile":
            raise ValueError("Measured input must use Base profile")
    allowed = {
        "__quantum__rt__initialize",
        "__quantum__qis__rx__body",
        "__quantum__qis__rzz__body",
    }
    if measured:
        allowed.update({
            "__quantum__qis__m__body",
            "__quantum__qis__mz__body",
            "__quantum__qis__mresetz__body",
            "__quantum__rt__array_record_output",
            "__quantum__rt__result_record_output",
        })
    else:
        allowed.add("__quantum__rt__tuple_record_output")
    calls = []
    for instruction in entry.basic_blocks[0].instructions:
        if instruction.opcode == pyqir.Opcode.RET:
            continue
        if not isinstance(instruction, pyqir.Call) or instruction.callee.name not in allowed:
            raise ValueError(f"Unexpected instruction in fixture: {instruction}")
        calls.append(instruction.callee.name)
    if not calls or calls[0] != "__quantum__rt__initialize" or calls.count(calls[0]) != 1:
        raise ValueError("Expected exactly one initialization, before the unitary prefix")
    return AggregateGatesPass().run_and_collect(module)


def verify_conversion(unmeasured: str, measured: str, parsed: ParsedCircuit) -> dict:
    expected = [(Op.RX if gate[0] == "rx" else Op.RZZ, *gate[1:]) for gate in parsed.gates]
    for qir, terminal in [(unmeasured, False), (measured, True)]:
        instructions, width, result_count = qir_instructions(qir, measured=terminal)
        if width != parsed.num_qubits or result_count != (width if terminal else 0):
            raise ValueError("Incorrect qubit/result count")
        if instructions[:len(expected)] != expected:
            raise ValueError("QIR unitary prefix differs in gate count, order, angle or operands")
        suffix = instructions[len(expected):]
        if not terminal:
            if len(suffix) != 1 or suffix[0][:2] != (Op.TupleRecordOutput, "0"):
                raise ValueError("Unexpected unmeasured QIR suffix")
            continue
        measurements = suffix[:width]
        if len(measurements) != width or any(
            gate[0] not in (Op.M, Op.MZ, Op.MResetZ) or gate[1:] != (qubit, qubit)
            for qubit, gate in enumerate(measurements)
        ):
            raise ValueError("Expected one ordered terminal measurement per qubit")
        output = suffix[width:]
        if len(output) != width + 1 or output[0][:2] != (Op.ArrayRecordOutput, str(width)):
            raise ValueError("Expected one result array of the circuit width")
        if any(
            gate[:2] != (Op.ResultRecordOutput, str(qubit))
            for qubit, gate in enumerate(output[1:])
        ):
            raise ValueError("Incorrect terminal result-record ordering")
    return {
        "unitary_gate_count": len(expected),
        "gate_counts": dict(Counter(gate[0] for gate in parsed.gates)),
        "qubits": parsed.num_qubits,
        "results": parsed.num_qubits,
        "measurement_qubits": list(range(parsed.num_qubits)),
        "measurement_operations": [str(gate[0]) for gate in measurements],
        "output_result_ids": list(range(parsed.num_qubits)),
        "gate_sequence_preserved": True,
    }


def check_case_a_schedule(parsed: ParsedCircuit, nx: int, ny: int) -> dict:
    """Check layer order and coefficients against the hand-derived Suzuki schedule."""
    if parsed.num_qubits != nx * ny:
        raise ValueError("Incorrect Case A width")
    edges = {
        (y * nx + x, y * nx + x + 1)
        for y in range(ny) for x in range(nx - 1)
    } | {
        (y * nx + x, (y + 1) * nx + x)
        for y in range(ny - 1) for x in range(nx)
    }
    actual_edges = {tuple(sorted(gate[2:])) for gate in parsed.gates if gate[0] == "rzz"}
    if actual_edges != edges:
        raise ValueError("The actual lattice differs from the qualified open row-major grid")
    p = 1 / (4 - 4 ** (1 / 3))
    steps = [p / 2, p / 2, (1 - 4 * p) / 2, p / 2, p / 2]
    rx_angles = [0.5 * steps[0]]
    rx_angles += [0.5 * (left + right) for left, right in zip(steps, steps[1:])]
    rx_angles += [0.5 * steps[-1]]
    sites = {(qubit,) for qubit in range(parsed.num_qubits)}
    layers = []
    for rx_angle, step in zip(rx_angles, steps):
        layers.extend([("rx", rx_angle, sites), ("rzz", 2 * step, edges)])
    layers.append(("rx", rx_angles[-1], sites))
    maximum_error = 0.0
    offset = 0
    for kind, angle, operands in layers * 2:
        actual = parsed.gates[offset:offset + len(operands)]
        if (
            len(actual) != len(operands)
            or any(gate[0] != kind for gate in actual)
            or {tuple(sorted(gate[2:])) for gate in actual} != operands
        ):
            raise ValueError(f"Incorrect Case A Suzuki layer at gate {offset}")
        error = max(abs(gate[1] - angle) for gate in actual)
        maximum_error = max(maximum_error, error)
        if error > 1e-14:
            raise ValueError(f"Incorrect Case A Suzuki coefficients at gate {offset}")
        offset += len(operands)
    if offset != len(parsed.gates):
        raise ValueError("Unexpected gates after the Case A Suzuki schedule")
    return {
        "edges": sorted(edges),
        "edge_count": len(edges),
        "site_index": "q = y * nx + x",
        "periodic_x": False,
        "periodic_y": False,
        "dfs_ordering": False,
        "suzuki_p": p,
        "maximum_coefficient_error": maximum_error,
    }


def little_endian_state(dump: qsharp.StateDump) -> np.ndarray:
    width = dump.qubit_count
    if not 1 <= width <= 16:
        raise ValueError("This reference is bounded to 1 through 16 qubits")
    # Q# dumps put q0 at the most significant bit, unlike raw SparseStateSim.
    indices = [int(f"{index:0{width}b}"[::-1], 2) for index in range(1 << width)]
    return np.asarray(dump.as_dense_state(), dtype="<c16")[indices]


def capture_reference(source: str, width: int, *, seed: int = 42) -> np.ndarray:
    if not 1 <= width <= 16:
        raise ValueError("This reference is bounded to 1 through 16 qubits")
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    qsharp.eval(source)
    shot = qsharp.run("Ising2DTrotter()", 1, save_events=True, seed=seed, type="sparse")[0]
    if len(shot["dumps"]) != 1 or shot["dumps"][0].qubit_count != width:
        raise ValueError("Expected exactly one full-width pre-measurement dump")
    if not isinstance(shot["result"], list) or len(shot["result"]) != width:
        raise ValueError(f"Reference execution did not finish normally: {shot['result']}")
    return little_endian_state(shot["dumps"][0])


def probabilities(state: np.ndarray) -> tuple[np.ndarray, float]:
    if state.ndim != 1 or not np.isfinite(state).all():
        raise ValueError("Expected a finite one-dimensional amplitude vector")
    masses = np.abs(state) ** 2
    norm = float(np.sum(masses))
    if not np.isfinite(norm) or abs(norm - 1) > NORM_LIMIT:
        raise ValueError(f"Squared norm {norm} exceeds the {NORM_LIMIT} error limit")
    return masses / norm, norm


def compare(actual: np.ndarray, expected: np.ndarray, limit: float) -> dict:
    if actual.shape != expected.shape:
        raise ValueError(f"State shape mismatch: {actual.shape} != {expected.shape}")
    actual_p, norm = probabilities(actual)
    expected_p, _ = probabilities(expected)
    amplitude_error = float(np.max(np.abs(actual - expected)))
    tv = float(np.sum(np.abs(actual_p - expected_p)) / 2)
    if amplitude_error > limit or tv > limit:
        raise ValueError(f"State comparison failed: amplitude={amplitude_error}, TV={tv}, limit={limit}")
    return {
        "squared_norm": norm,
        "squared_norm_error": abs(norm - 1),
        "maximum_amplitude_error": amplitude_error,
        "probability_total_variation": tv,
        "limit": limit,
        "global_phase_aligned": False,
    }


def json_bytes(value: dict) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n").encode()


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def npy_bytes(array: np.ndarray) -> bytes:
    buffer = BytesIO()
    np.save(buffer, array, allow_pickle=False)
    return buffer.getvalue()


def generate(output: Path) -> dict:
    if output.exists():
        raise FileExistsError(f"Refusing to overwrite retained artifacts: {output}")
    started = time.perf_counter()
    unmeasured = build_ising_2d_qir(**CASE_A)
    parsed = parse_gates(unmeasured)
    source = qsharp_source(parsed)
    reference_source = qsharp_source(parsed, dump_state=True)
    measured = compile_measured_qir(source)
    conversion = verify_conversion(unmeasured, measured, parsed)
    lattice = check_case_a_schedule(parsed, 4, 4)
    graph = ising_lattice(4, 4)
    if any(graph.weight(a, b) != 1.0 for a, b in lattice["edges"]) or graph.num_edges != 24:
        raise ValueError("Chemistry lattice weights or edge count changed")
    lattice["chemistry_graph"] = json.loads(graph.to_json())
    state = capture_reference(reference_source, 16)
    distribution, norm = probabilities(state)
    repeat = capture_reference(reference_source, 16, seed=17)
    report = {
        "scope": "I1 CPU reference only; no TN or A100 execution",
        "conversion": conversion,
        "amplitude_count": int(state.size),
        "amplitude_payload_bytes": state.nbytes,
        "probability_payload_bytes": distribution.nbytes,
        "probability_sum": float(distribution.sum()),
        "squared_norm": norm,
        "repeat_reference": compare(repeat, state, REFERENCE_LIMIT),
        "reference_seeds": [42, 17],
        "generation_and_reference_seconds": time.perf_counter() - started,
        "passed": True,
    }
    artifacts = {
        "unmeasured.ll": unmeasured.encode(),
        "measured.ll": measured.encode(),
        "measured.qs": source.encode(),
        "reference.qs": reference_source.encode(),
        "amplitudes.npy": npy_bytes(state.astype("<c16")),
        "probabilities.npy": npy_bytes(distribution.astype("<f8")),
        "validation.json": json_bytes(report),
    }
    native_hashes = {}
    for package in ("qdk", "qdk-chemistry"):
        distribution_metadata = metadata.distribution(package)
        files = distribution_metadata.files
        if files is None:
            raise ValueError(f"No installed file inventory for {package}")
        binaries = [file for file in files if str(file).endswith(".so")]
        if not binaries:
            raise ValueError(f"No native Linux binaries found for {package}")
        for file in binaries:
            path = Path(distribution_metadata.locate_file(file))
            native_hashes[f"{package}/{file}"] = sha256(path.read_bytes())
    provenance = {
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "case": {**CASE_A, "J": 1.0, "h": 0.5, "preparation": "identity / |0>^16"},
        "lattice": lattice,
        "basis": "k = sum(b[q] * 2**q), q=0..15; q0 is least significant",
        "amplitude_format": "NumPy .npy, shape=(65536,), dtype='<c16' (little-endian complex-f64)",
        "probability_format": "NumPy .npy, shape=(65536,), dtype='<f8'; abs(amplitude)^2 / squared_norm",
        "snapshot": "Std.Diagnostics.DumpMachine immediately before MResetEachZ",
        "engine": "qdk.qsharp.run(type='sparse'); SparseStateSim, no TN construction",
        "source_base_commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=DIRECTORY, text=True
        ).strip(),
        "script_sha256": {
            name: sha256((DIRECTORY / name).read_bytes())
            for name in ("build_measured_circuit.py", "reference.py", "requirements-reference.txt")
        },
        "packages": dict(sorted((dist.metadata["Name"], dist.version) for dist in metadata.distributions())),
        "native_binary_sha256": native_hashes,
        "platform": {"system": platform.system(), "machine": platform.machine(), "python": platform.python_version()},
        "numerical_contract": {
            "squared_norm_error_limit": NORM_LIMIT,
            "repeat_reference_error_limit": REFERENCE_LIMIT,
            "later_tn_amplitude_and_probability_tv_limit": COMPARISON_LIMIT,
            "global_phase_alignment": False,
            "sparse_policy": "existing engine round-off/pruning, not an exact-arithmetic oracle",
            "meaning": "finite Trotter circuit, not exact Hamiltonian evolution",
        },
        "sha256": {name: sha256(data) for name, data in artifacts.items()},
        "reproduce": "python reference.py generate --output <new-directory>",
        "verify_without_chemistry": "python reference.py verify --input fixtures/case_a_4x4",
    }
    output.mkdir(parents=True, exist_ok=False)
    for name, data in {**artifacts, "provenance.json": json_bytes(provenance)}.items():
        (output / name).write_bytes(data)
    return report


def verify(directory: Path) -> dict:
    provenance = json.loads((directory / "provenance.json").read_text())
    for name in (
        "unmeasured.ll", "measured.ll", "measured.qs", "reference.qs",
        "amplitudes.npy", "probabilities.npy", "validation.json",
    ):
        if sha256((directory / name).read_bytes()) != provenance["sha256"][name]:
            raise ValueError(f"Artifact hash mismatch: {name}")
    unmeasured = (directory / "unmeasured.ll").read_text()
    measured = (directory / "measured.ll").read_text()
    parsed = parse_gates(unmeasured)
    check_case_a_schedule(parsed, 4, 4)
    for name, diagnostic in [("measured.qs", False), ("reference.qs", True)]:
        if (directory / name).read_text() != qsharp_source(parsed, dump_state=diagnostic):
            raise ValueError(f"{name} does not reproduce the retained circuit")
    conversion = verify_conversion(unmeasured, measured, parsed)
    recompiled = compile_measured_qir((directory / "measured.qs").read_text())
    verify_conversion(unmeasured, recompiled, parsed)
    retained = np.load(directory / "amplitudes.npy", allow_pickle=False)
    retained_p = np.load(directory / "probabilities.npy", allow_pickle=False)
    if retained.dtype.str != "<c16" or retained_p.dtype.str != "<f8":
        raise ValueError("Expected little-endian complex-f64 amplitudes and f64 probabilities")
    if retained.shape != (65536,) or retained_p.shape != (65536,):
        raise ValueError("The Case A reference must contain 65536 amplitudes/probabilities")
    expected_p, _ = probabilities(retained)
    if (
        not np.isfinite(retained_p).all()
        or (retained_p < 0).any()
        or abs(float(retained_p.sum()) - 1) > REFERENCE_LIMIT
        or float(np.sum(np.abs(retained_p - expected_p)) / 2) > REFERENCE_LIMIT
    ):
        raise ValueError("Retained probabilities do not match the retained amplitudes")
    state = capture_reference((directory / "reference.qs").read_text(), 16)
    return {
        "scope": "I1 CPU reference only; no TN or A100 execution",
        "conversion": conversion,
        "reference_comparison": compare(state, retained, REFERENCE_LIMIT),
        "recompiled_qir_byte_identical": recompiled == measured,
        "passed": True,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("generate").add_argument("--output", type=Path, required=True)
    commands.add_parser("verify").add_argument("--input", type=Path, required=True)
    args = parser.parse_args()
    report = generate(args.output) if args.command == "generate" else verify(args.input)
    print(json_bytes(report).decode(), end="")


if __name__ == "__main__":
    main()
