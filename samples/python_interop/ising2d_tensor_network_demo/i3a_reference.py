# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Retain bounded native-test inputs using the existing recipe and CPU oracle.

Generation refuses to overwrite its directory. The frozen 4x4 arrays are
referenced in place, not regenerated or replaced.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
from importlib import metadata
import json
from pathlib import Path
import platform
import struct
import subprocess

import numpy as np

from build_measured_circuit import (
    build_ising_2d_qir, compile_measured_qir, parse_gates, qsharp_source,
    shared_buffer_diagnostic,
)
from reference import (
    DIRECTORY, capture_reference, check_case_a_schedule, compare, json_bytes,
    npy_bytes, sha256, verify, verify_conversion,
)


def gate_records(parsed):
    return [
        [gate[0], struct.unpack("<Q", struct.pack("<d", gate[1]))[0], *gate[2:]]
        for gate in parsed.gates
    ]


def generate(output: Path) -> dict:
    if output.exists():
        raise FileExistsError(f"Refusing to overwrite retained artifacts: {output}")
    if metadata.version("qdk") != "1.32.3":
        raise ValueError("Generate with the separate released qdk==1.32.3 oracle")
    frozen = DIRECTORY / "fixtures/case_a_4x4"
    frozen_report = verify(frozen)
    raw_2x2 = build_ising_2d_qir(2, 2, 1.0, 4, 2)
    cases = [
        ("diagnostic", shared_buffer_diagnostic(), None, 1e-12),
        ("case_a_2x2", parse_gates(raw_2x2), raw_2x2, 1e-12),
        ("case_a_4x4", parse_gates((frozen / "unmeasured.ll").read_text()), None, 1e-8),
    ]
    artifacts = {}
    reports = {}
    for name, parsed, raw, limit in cases:
        source = qsharp_source(parsed)
        diagnostic = qsharp_source(parsed, dump_state=True)
        if name == "case_a_4x4":
            amplitudes = "../case_a_4x4/amplitudes.npy"
            reports[name] = frozen_report
        else:
            state = capture_reference(diagnostic, parsed.num_qubits)
            repeat = capture_reference(diagnostic, parsed.num_qubits, seed=17)
            reports[name] = compare(repeat, state, 1e-12)
            if abs(float(np.vdot(state, state).real) - 1) > 1e-12:
                raise ValueError("Tiny oracle exceeds the approved squared-norm limit")
            amplitudes = f"{name}.npy"
            artifacts[amplitudes] = npy_bytes(state)
            artifacts[f"{name}.qs"] = source.encode()
            artifacts[f"{name}.reference.qs"] = diagnostic.encode()
        if raw is not None:
            measured = compile_measured_qir(source)
            reports[name]["conversion"] = verify_conversion(raw, measured, parsed)
            reports[name]["schedule"] = check_case_a_schedule(parsed, 2, 2)
            artifacts[f"{name}.unmeasured.ll"] = raw.encode()
            artifacts[f"{name}.measured.ll"] = measured.encode()
        artifacts[f"{name}.json"] = json_bytes({
            "name": name,
            "qubits": parsed.num_qubits,
            "gates": gate_records(parsed),
            "angle_encoding": "IEEE754 binary64 bits as unsigned integer",
            "amplitudes": amplitudes,
            "amplitudes_sha256": sha256(
                (frozen / "amplitudes.npy").read_bytes()
                if name == "case_a_4x4" else artifacts[amplitudes]
            ),
            "limit": limit,
            "basis": "q0 least significant; first axis fastest",
        })
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
    provenance = {
        "scope": "Independent CPU inputs/oracles; no native TN or GPU execution",
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "source_base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=DIRECTORY, text=True).strip(),
        "script_sha256": {name: sha256((DIRECTORY / name).read_bytes()) for name in (
            "i3a_reference.py", "reference.py", "build_measured_circuit.py",
        )},
        "packages": {name: metadata.version(name) for name in ("qdk", "qdk-chemistry", "pyqir", "numpy")},
        "native_binary_sha256": binaries,
        "platform": {"machine": platform.machine(), "python": platform.python_version()},
        "reports": reports,
        "frozen_4x4_provenance_sha256": sha256((frozen / "provenance.json").read_bytes()),
        "sha256": {name: sha256(data) for name, data in artifacts.items()},
    }
    output.mkdir(parents=True, exist_ok=False)
    for name, data in {**artifacts, "provenance.json": json_bytes(provenance)}.items():
        (output / name).write_bytes(data)
    return provenance


def verify_inputs(directory: Path) -> None:
    provenance = json.loads((directory / "provenance.json").read_text())
    for name, expected in provenance["sha256"].items():
        if sha256((directory / name).read_bytes()) != expected:
            raise ValueError(f"Artifact hash mismatch: {name}")
    frozen = DIRECTORY / "fixtures/case_a_4x4"
    if sha256((frozen / "provenance.json").read_bytes()) != provenance["frozen_4x4_provenance_sha256"]:
        raise ValueError("Frozen I1 provenance changed")
    for name in ("diagnostic", "case_a_2x2", "case_a_4x4"):
        record = json.loads((directory / f"{name}.json").read_text())
        array = directory / record["amplitudes"]
        if sha256(array.read_bytes()) != record["amplitudes_sha256"]:
            raise ValueError(f"Amplitude hash mismatch: {name}")
        if name == "diagnostic":
            parsed = shared_buffer_diagnostic()
        else:
            raw = (directory / f"{name}.unmeasured.ll") if name == "case_a_2x2" else frozen / "unmeasured.ll"
            parsed = parse_gates(raw.read_text())
            size = 2 if name == "case_a_2x2" else 4
            check_case_a_schedule(parsed, size, size)
            measured = directory / f"{name}.measured.ll" if name == "case_a_2x2" else frozen / "measured.ll"
            verify_conversion(raw.read_text(), measured.read_text(), parsed)
        if record["qubits"] != parsed.num_qubits or record["gates"] != gate_records(parsed):
            raise ValueError(f"Native gate input differs from the verified circuit: {name}")
        state = capture_reference(qsharp_source(parsed, dump_state=True), parsed.num_qubits)
        compare(state, np.load(array, allow_pickle=False), 1e-12)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["generate", "verify"])
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    if args.command == "generate":
        generate(args.directory)
    else:
        verify_inputs(args.directory)
    print(f"{args.command}: independent inputs/oracles passed")


if __name__ == "__main__":
    main()
