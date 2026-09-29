# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Freeze or verify the χ-sweep circuits and the 4×4 CPU references.

``generate`` needs the pinned reference environment (requirements-reference.txt,
including qdk-chemistry). ``verify`` needs the same environment but not the
chemistry builders: it re-derives each circuit's gates from its frozen QIR,
checks them against the hand-derived Suzuki schedule and recompiles them.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
from importlib import metadata
import json
from pathlib import Path
import platform
import subprocess
import time

import numpy as np
from qdk._native import QirInstructionId as Op

from build_measured_circuit import (
    ParsedCircuit,
    build_ising_2d_qir,
    build_measured_qir,
    compile_measured_qir,
    parse_gates,
    qsharp_source,
)
from ising import (
    CASE_A,
    CIRCUITS,
    DIRECTORY,
    J,
    bonds,
    circuit_name,
    correlation_terms,
    field_label,
    frozen_cases,
    magnetization_terms,
    z_expectation,
)
from reference import (
    REFERENCE_LIMIT,
    capture_reference,
    check_case_a_schedule,
    compare,
    json_bytes,
    probabilities,
    qir_instructions,
    sha256,
    verify,
    verify_conversion,
)


RECIPE = dict(total_time=1.0, order=4, num_divisions=2)
REFERENCE_SIZE = 4
# I1's published 4×4 h=0.5 values, rounded to 7 decimals.
CASE_A_ORACLE = {"m_z": 0.9508364, "c_zz": 0.9326830}


def observables(distribution: np.ndarray, size: int) -> dict:
    """m_z, C_ZZ and their per-site / per-bond parts from a little-endian distribution."""
    z = [z_expectation(distribution, [("Z", [q], 1.0)]) for q in range(size * size)]
    zz = [z_expectation(distribution, [("ZZ", [i, j], 1.0)]) for i, j in bonds(size)]
    return {
        "m_z": z_expectation(distribution, magnetization_terms(size)),
        "c_zz": z_expectation(distribution, correlation_terms(size)),
        "z": z,
        "zz": zz,
    }


def case_a_reference() -> dict:
    """The 4×4 h=0.5 reference, from I1's frozen amplitudes (hash-checked by ``verify``)."""
    state = np.load(CASE_A / "amplitudes.npy", allow_pickle=False)
    distribution, norm = probabilities(state)
    values = observables(distribution, REFERENCE_SIZE)
    for name, expected in CASE_A_ORACLE.items():
        if round(values[name], 7) != expected:
            raise ValueError(f"Case A {name} = {values[name]} does not round to {expected}")
    return {
        "size": REFERENCE_SIZE,
        "field": 0.5,
        "J": J,
        **values,
        "squared_norm": norm,
        "source": "fixtures/case_a_4x4/amplitudes.npy (I1 SparseStateSim state)",
        "amplitudes_sha256": sha256((CASE_A / "amplitudes.npy").read_bytes()),
    }


def sparse_reference(parsed: ParsedCircuit, field: float) -> dict:
    """A 4×4 reference from the pre-measurement state of QDK's sparse CPU simulator."""
    source = qsharp_source(parsed, dump_state=True)
    state = capture_reference(source, parsed.num_qubits)
    repeat = capture_reference(source, parsed.num_qubits, seed=17)
    agreement = compare(repeat, state, REFERENCE_LIMIT)
    distribution, norm = probabilities(state)
    return {
        "size": REFERENCE_SIZE,
        "field": field,
        "J": J,
        **observables(distribution, REFERENCE_SIZE),
        "squared_norm": norm,
        "source": "qdk.qsharp.run(type='sparse'), DumpMachine before MResetEachZ",
        "reference_seeds": [42, 17],
        "repeat_maximum_amplitude_error": agreement["maximum_amplitude_error"],
    }


def gates_from_measured(qir: str) -> ParsedCircuit:
    """Recover the unitary prefix of a frozen measured circuit through QDK's collector."""
    instructions, width, results = qir_instructions(qir, measured=True)
    if results != width:
        raise ValueError("Expected one result per qubit")
    gates = []
    for instruction in instructions:
        if instruction[0] == Op.RX:
            gates.append(("rx", *instruction[1:]))
        elif instruction[0] == Op.RZZ:
            gates.append(("rzz", *instruction[1:]))
        else:
            break
    return ParsedCircuit(width, gates)


def generate(output: Path) -> dict:
    if output.exists():
        raise FileExistsError(f"Refusing to overwrite retained artifacts: {output}")
    if metadata.version("qdk") != "1.32.3" or metadata.version("qdk-chemistry") != "2.2.1":
        raise ValueError("Generate with the pinned environment in requirements-reference.txt")
    started = time.perf_counter()
    verify(CASE_A)
    regenerated = build_measured_qir(4, 4, h=0.5, **RECIPE)
    if regenerated != (CASE_A / "measured.ll").read_text():
        raise ValueError("--field 0.5 no longer reproduces the frozen Case A circuit")
    artifacts: dict[str, bytes] = {}
    circuits = {}
    references = {"4x4_h0.5": case_a_reference()}
    for size, field in frozen_cases():
        if (size, field) == (4, 0.5):
            continue
        unmeasured = build_ising_2d_qir(size, size, h=field, **RECIPE)
        parsed = parse_gates(unmeasured)
        schedule = check_case_a_schedule(parsed, size, size, h=field)
        measured = compile_measured_qir(qsharp_source(parsed))
        conversion = verify_conversion(unmeasured, measured, parsed)
        name = circuit_name(size, field)
        artifacts[name] = measured.encode()
        circuits[name] = {
            "size": size,
            "field": field,
            "J": J,
            "args": f"--nx {size} --ny {size} --field {field_label(field)}",
            "qubits": conversion["qubits"],
            "bonds": schedule["edge_count"],
            "gate_counts": conversion["gate_counts"],
            "maximum_coefficient_error": schedule["maximum_coefficient_error"],
            "unmeasured_sha256": sha256(unmeasured.encode()),
        }
        if size == REFERENCE_SIZE:
            references[f"4x4_h{field_label(field)}"] = sparse_reference(parsed, field)
    artifacts["cpu_reference.json"] = json_bytes(references)
    provenance = {
        "scope": "Frozen χ-sweep circuits and 4×4 CPU references; no TN or GPU execution",
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "recipe": {**RECIPE, "J": J, "preparation": "identity / |0...0>", "site_index": "q = y * N + x"},
        "case_a_4x4": "fixtures/case_a_4x4/measured.ll is the size=4, h=0.5 circuit; --field 0.5 reproduces it byte for byte",
        "case_a_provenance_sha256": sha256((CASE_A / "provenance.json").read_bytes()),
        "circuits": circuits,
        "not_shipped": "unmeasured chemistry QIR; regenerate it with build_ising_2d_qir and compare unmeasured_sha256",
        "source_base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=DIRECTORY, text=True).strip(),
        "script_sha256": {name: sha256((DIRECTORY / name).read_bytes()) for name in (
            "circuits.py", "ising.py", "reference.py", "build_measured_circuit.py", "requirements-reference.txt",
        )},
        "packages": {name: metadata.version(name) for name in ("qdk", "qdk-chemistry", "pyqir", "numpy")},
        "native_binary_sha256": native_binaries(),
        "platform": {"system": platform.system(), "machine": platform.machine(), "python": platform.python_version()},
        "generation_seconds": time.perf_counter() - started,
        "sha256": {name: sha256(data) for name, data in artifacts.items()},
        "reproduce": "python circuits.py generate --output <new-directory>",
        "verify": "python circuits.py verify --input fixtures/ising_circuits",
    }
    output.mkdir(parents=True, exist_ok=False)
    for name, data in {**artifacts, "provenance.json": json_bytes(provenance)}.items():
        (output / name).write_bytes(data)
    return {"circuits": circuits, "cpu_reference": references}


def native_binaries() -> dict[str, str]:
    binaries = {}
    for package in ("qdk", "qdk-chemistry"):
        distribution = metadata.distribution(package)
        files = distribution.files
        if files is None:
            raise ValueError(f"Missing installed-file inventory: {package}")
        native = [file for file in files if str(file).endswith(".so")]
        if not native:
            raise ValueError(f"Missing native binary: {package}")
        for file in native:
            binaries[f"{package}/{file}"] = sha256(Path(distribution.locate_file(file)).read_bytes())
    return binaries


def verify_circuits(directory: Path = CIRCUITS) -> dict:
    provenance = json.loads((directory / "provenance.json").read_text())
    for name, expected in provenance["sha256"].items():
        if sha256((directory / name).read_bytes()) != expected:
            raise ValueError(f"Artifact hash mismatch: {name}")
    if sha256((CASE_A / "provenance.json").read_bytes()) != provenance["case_a_provenance_sha256"]:
        raise ValueError("Frozen Case A provenance changed")
    expected_names = {circuit_name(size, field) for size, field in frozen_cases() if (size, field) != (4, 0.5)}
    if set(provenance["circuits"]) != expected_names:
        raise ValueError("The frozen circuits differ from the catalogue in ising.py")
    report = {}
    for name, record in provenance["circuits"].items():
        measured = (directory / name).read_text()
        parsed = gates_from_measured(measured)
        schedule = check_case_a_schedule(parsed, record["size"], record["size"], h=record["field"])
        if compile_measured_qir(qsharp_source(parsed)) != measured:
            raise ValueError(f"{name} does not recompile byte for byte from its gates")
        report[name] = {"qubits": parsed.num_qubits, "gates": len(parsed.gates),
                        "maximum_coefficient_error": schedule["maximum_coefficient_error"]}
    retained = json.loads((directory / "cpu_reference.json").read_text())
    if sha256((CASE_A / "amplitudes.npy").read_bytes()) != retained["4x4_h0.5"]["amplitudes_sha256"]:
        raise ValueError("Case A amplitudes changed")
    recomputed = {
        "4x4_h0.5": case_a_reference(),
        "4x4_h3.03": sparse_reference(gates_from_measured((directory / circuit_name(4, 3.03)).read_text()), 3.03),
    }
    if set(retained) != set(recomputed):
        raise ValueError("Unexpected CPU references")
    for key, values in recomputed.items():
        for name in ("m_z", "c_zz"):
            if abs(values[name] - retained[key][name]) > REFERENCE_LIMIT:
                raise ValueError(f"{key} {name} does not replay: {values[name]} != {retained[key][name]}")
        report[key] = {name: values[name] for name in ("m_z", "c_zz")}
    return {"passed": True, **report}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("generate").add_argument("--output", type=Path, required=True)
    commands.add_parser("verify").add_argument("--input", type=Path, default=CIRCUITS)
    args = parser.parse_args()
    report = generate(args.output) if args.command == "generate" else verify_circuits(args.input)
    print(json_bytes(report).decode(), end="")


if __name__ == "__main__":
    main()
