from __future__ import annotations

import argparse
import ctypes
import ctypes.util
import hashlib
import importlib.metadata
import json
import multiprocessing
import os
import platform
import subprocess
import sys
import time
import traceback
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import qdk
from qdk import TargetProfile, qsharp
from qdk.simulation import run_qir

try:
    from qdk.simulation import MpsOptions
except ImportError as error:
    MpsOptions = None
    MPS_UNAVAILABLE_REASON = f"MpsOptions is unavailable from this qdk: {error}"
else:
    MPS_UNAVAILABLE_REASON = None

if MPS_UNAVAILABLE_REASON is None and (
    sys.platform != "linux" or platform.machine() not in ("x86_64", "AMD64")
):
    MPS_UNAVAILABLE_REASON = (
        "cuTensorNet MPS execution requires a Linux x86_64 host with NVIDIA support"
    )

MPS_AVAILABLE = MPS_UNAVAILABLE_REASON is None


TASK_1_COMMIT = "fca4f780d98c276c1064e8254f31a5b172c102a2"
DEFAULT_DEPTH = 8
DEFAULT_THETA = 0.30
DEFAULT_SEED = 42
DEMO_TIME_BUDGET_SECONDS = 300.0

QSHARP_SOURCE = """\
import Std.Measurement.*;
import Std.Intrinsic.*;

operation TrotterQuench() : Result[] {
    use qs = Qubit[__WIDTH__];

    for i in __DOMAIN_WALL_START__..__DOMAIN_WALL_END__ {
        X(qs[i]);
    }

    for _layer in 0..__LAST_LAYER__ {
        for i in 0..__LAST_PAIR__ {
            CNOT(qs[i], qs[i + 1]);
            Rz(__THETA__, qs[i + 1]);
            CNOT(qs[i], qs[i + 1]);
        }
        for q in qs {
            Rx(__THETA__, q);
        }
    }

    return MResetEachZ(qs);
}
"""


def qsharp_source(width: int, depth: int, theta: float) -> str:
    replacements = {
        "__WIDTH__": str(width),
        "__DOMAIN_WALL_START__": str(width // 2),
        "__DOMAIN_WALL_END__": str(width - 1),
        "__LAST_LAYER__": str(depth - 1),
        "__LAST_PAIR__": str(width - 2),
        "__THETA__": format(theta, ".17g"),
    }
    source = QSHARP_SOURCE
    for placeholder, value in replacements.items():
        source = source.replace(placeholder, value)
    return source


def qubit(index: int) -> str:
    return f"%Qubit* inttoptr (i64 {index} to %Qubit*)"


def result(index: int) -> str:
    return f"%Result* inttoptr (i64 {index} to %Result*)"


def generate_qir(width: int, depth: int, theta: float) -> str:
    instructions = []
    for index in range(width // 2, width):
        instructions.append(
            f"    call void @__quantum__qis__x__body({qubit(index)})"
        )

    angle = format(theta, ".17g")
    for _layer in range(depth):
        for index in range(width - 1):
            instructions.extend(
                (
                    "    call void @__quantum__qis__cx__body("
                    f"{qubit(index)}, {qubit(index + 1)})",
                    "    call void @__quantum__qis__rz__body("
                    f"double {angle}, {qubit(index + 1)})",
                    "    call void @__quantum__qis__cx__body("
                    f"{qubit(index)}, {qubit(index + 1)})",
                )
            )
        for index in range(width):
            instructions.append(
                "    call void @__quantum__qis__rx__body("
                f"double {angle}, {qubit(index)})"
            )

    for index in range(width):
        instructions.append(
            "    call void @__quantum__qis__mz__body("
            f"{qubit(index)}, {result(index)})"
        )
    instructions.append(
        f"    call void @__quantum__rt__tuple_record_output(i64 {width}, i8* null)"
    )
    for index in range(width):
        instructions.append(
            "    call void @__quantum__rt__result_record_output("
            f"{result(index)}, i8* null)"
        )

    body = "\n".join(instructions)
    return f"""\
%Result = type opaque
%Qubit = type opaque

define void @ENTRYPOINT__main() #0 {{
entry:
{body}
    ret void
}}

declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__rx__body(double, %Qubit*)
declare void @__quantum__qis__rz__body(double, %Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = {{ "entry_point" "qir_profiles"="base_profile" "required_num_qubits"="{width}" "required_num_results"="{width}" }}
"""


def evolution_gate_count(width: int, depth: int) -> int:
    return depth * (4 * width - 3)


def _bit(value: Any) -> str:
    if isinstance(value, bool):
        return "1" if value else "0"
    if isinstance(value, int) and value in (0, 1):
        return str(value)

    name = getattr(value, "name", None)
    if name in ("Zero", "One"):
        return "0" if name == "Zero" else "1"

    text = str(value)
    if text in ("Zero", "Result.Zero"):
        return "0"
    if text in ("One", "Result.One"):
        return "1"
    raise ValueError(f"unsupported result value: {value!r}")


def _shot_to_bitstring(shot: Any) -> str:
    if isinstance(shot, str) and shot and set(shot) <= {"0", "1"}:
        return shot
    if isinstance(shot, (list, tuple)):
        return "".join(_shot_to_bitstring(value) for value in shot)
    return _bit(shot)


def _summarize_shots(
    shots: list[Any], width: int, retain_shots: bool
) -> dict[str, Any]:
    bitstrings = [_shot_to_bitstring(shot) for shot in shots]
    invalid_lengths = sorted({len(bits) for bits in bitstrings if len(bits) != width})
    if invalid_lengths:
        raise ValueError(
            f"expected {width} result bits per shot, observed lengths {invalid_lengths}"
        )

    shot_count = len(bitstrings)
    ones = [0] * width
    for bits in bitstrings:
        for index, bit in enumerate(bits):
            ones[index] += bit == "1"

    summary: dict[str, Any] = {
        "returned_shots": shot_count,
        "distinct_outcomes": len(set(bitstrings)),
        "histogram": dict(sorted(Counter(bitstrings).items())),
        "one_frequencies": [count / shot_count for count in ones]
        if shot_count
        else [],
        "first_outcomes": bitstrings[:5],
    }
    if retain_shots:
        summary["shot_bitstrings"] = bitstrings
    return summary


def _peak_rss_bytes() -> int | None:
    try:
        import resource

        peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        return int(peak if sys.platform == "darwin" else peak * 1024)
    except (ImportError, OSError):
        return None


def _worker(connection: Any, request: dict[str, Any]) -> None:
    started = time.perf_counter()
    try:
        action = request["action"]
        width = request["width"]
        depth = request["depth"]
        theta = request["theta"]

        if action == "compile_qsharp":
            qsharp.init(target_profile=TargetProfile.Base)
            qsharp.eval(qsharp_source(width, depth, theta))
            call_started = time.perf_counter()
            compiled_qir = str(qsharp.compile("TrotterQuench()"))
            call_wall = time.perf_counter() - call_started
            response = {
                "outcome": "success",
                "simulator_wall_seconds": call_wall,
                "compiled_qir": compiled_qir if request["return_qir"] else None,
                "qir_sha256": hashlib.sha256(compiled_qir.encode()).hexdigest(),
                "qir_characters": len(compiled_qir),
            }
        elif action == "run_qsharp":
            qsharp.init()
            qsharp.eval(qsharp_source(width, depth, theta))
            call_started = time.perf_counter()
            shots = qsharp.run(
                "TrotterQuench()",
                shots=request["shots"],
                seed=request["seed"],
                type=request["backend"],
            )
            call_wall = time.perf_counter() - call_started
            response = {
                "outcome": "success",
                "simulator_wall_seconds": call_wall,
                **_summarize_shots(shots, width, request["retain_shots"]),
            }
        elif action == "run_qir":
            qir = request.get("qir") or generate_qir(width, depth, theta)
            if request["backend"] == "mps" and not MPS_AVAILABLE:
                raise RuntimeError(MPS_UNAVAILABLE_REASON)
            options = MpsOptions(device="nvidia") if request["backend"] == "mps" else None
            call_started = time.perf_counter()
            shots = run_qir(
                qir,
                shots=request["shots"],
                seed=request["seed"],
                type=request["backend"],
                mps_options=options,
            )
            call_wall = time.perf_counter() - call_started
            response = {
                "outcome": "success",
                "simulator_wall_seconds": call_wall,
                **_summarize_shots(shots, width, request["retain_shots"]),
            }
        else:
            raise ValueError(f"unknown worker action: {action}")
    except BaseException as error:
        response = {
            "outcome": "error",
            "error_type": type(error).__name__,
            "error": str(error),
            "traceback": traceback.format_exc(),
        }

    response["worker_wall_seconds"] = time.perf_counter() - started
    response["peak_host_memory_bytes"] = _peak_rss_bytes()
    try:
        connection.send(response)
    finally:
        connection.close()


def _proc_peak_rss_bytes(process_id: int) -> int | None:
    try:
        status = Path(f"/proc/{process_id}/status").read_text(encoding="utf-8")
    except OSError:
        return None
    for line in status.splitlines():
        if line.startswith("VmHWM:"):
            return int(line.split()[1]) * 1024
    return None


def run_isolated(request: dict[str, Any], timeout_seconds: float) -> dict[str, Any]:
    context = multiprocessing.get_context("spawn")
    parent, child = context.Pipe(duplex=False)
    process = context.Process(target=_worker, args=(child, request), daemon=False)
    started = time.perf_counter()
    process.start()
    child.close()

    response = None
    sampled_peak = 0
    deadline = started + timeout_seconds
    while time.perf_counter() < deadline:
        sampled_peak = max(sampled_peak, _proc_peak_rss_bytes(process.pid) or 0)
        if parent.poll(0.05):
            try:
                response = parent.recv()
            except EOFError:
                response = None
            break
        if not process.is_alive():
            break

    if response is None and process.is_alive():
        process.terminate()
        process.join(2.0)
        if process.is_alive():
            process.kill()
            process.join()
        response = {
            "outcome": "timeout",
            "error_type": "TimeoutError",
            "error": f"simulator worker exceeded {timeout_seconds:.3f} seconds",
        }
    else:
        process.join(2.0)
        if process.is_alive():
            process.terminate()
            process.join()

    if response is None:
        response = {
            "outcome": "process_exit",
            "error_type": "WorkerProcessExit",
            "error": f"simulator worker exited with code {process.exitcode}",
        }

    parent.close()
    response["process_exit_code"] = process.exitcode
    response["total_wall_seconds"] = time.perf_counter() - started
    response["peak_host_memory_bytes"] = max(
        sampled_peak,
        response.get("peak_host_memory_bytes") or 0,
    ) or None
    return response


class Evidence:
    def __init__(self, destination: Path, arguments: dict[str, Any]) -> None:
        self.destination = destination
        self.data: dict[str, Any] = {
            "schema_version": 1,
            "created_at": datetime.now(timezone.utc).isoformat(),
            "task_1_commit": TASK_1_COMMIT,
            "arguments": arguments,
            "environment": environment_metadata(),
            "runs": [],
            "analysis": {},
            "status": "running",
        }
        self.write()

    def add_run(self, record: dict[str, Any]) -> None:
        self.data["runs"].append(record)
        self.write()

    def analyze(self, key: str, value: Any) -> None:
        self.data["analysis"][key] = value
        self.write()

    def finish(self, status: str) -> None:
        self.data["status"] = status
        self.data["finished_at"] = datetime.now(timezone.utc).isoformat()
        self.write()

    def write(self) -> None:
        temporary = self.destination.with_suffix(
            f"{self.destination.suffix}.{os.getpid()}.tmp"
        )
        temporary.write_text(
            json.dumps(self.data, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        temporary.replace(self.destination)


def _command_output(command: list[str]) -> dict[str, Any]:
    try:
        completed = subprocess.run(
            command,
            capture_output=True,
            check=False,
            text=True,
            timeout=5.0,
        )
        return {
            "command": command,
            "return_code": completed.returncode,
            "stdout": completed.stdout.strip(),
            "stderr": completed.stderr.strip(),
        }
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"command": command, "error": str(error)}


def _cutensornet_metadata() -> dict[str, Any]:
    candidates = [
        os.environ.get("QDK_CUTENSORNET_LIBRARY"),
        "/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2",
        ctypes.util.find_library("cutensornet"),
        "libcutensornet.so.2",
    ]
    errors = []
    for candidate in dict.fromkeys(value for value in candidates if value):
        try:
            library = ctypes.CDLL(candidate)
            get_version = library.cutensornetGetVersion
            get_version.restype = ctypes.c_size_t
            return {"library": candidate, "version": get_version()}
        except (AttributeError, OSError) as error:
            errors.append(f"{candidate}: {error}")
    return {"error": "; ".join(errors)}


def environment_metadata() -> dict[str, Any]:
    try:
        total_memory = os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES")
    except (AttributeError, OSError, ValueError):
        total_memory = None
    return {
        "qdk_file": qdk.__file__,
        "qdk_package_version": importlib.metadata.version("qdk"),
        "mps_available": MPS_AVAILABLE,
        "mps_unavailable_reason": MPS_UNAVAILABLE_REASON,
        "hostname": platform.node(),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "python": sys.version,
        "logical_cpus": os.cpu_count(),
        "total_host_memory_bytes": total_memory,
        "nvidia_smi": _command_output(
            [
                "nvidia-smi",
                "--query-gpu=name,driver_version,memory.total",
                "--format=csv,noheader,nounits",
            ]
        ),
        "cutensornet": _cutensornet_metadata(),
    }


def resolve_output_path(value: Path) -> Path:
    resolved = value.expanduser().resolve()
    if resolved.exists() and resolved.is_dir():
        timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
        resolved = resolved / f"mps-trotter-quench-{timestamp}.json"
    elif not resolved.suffix:
        resolved.mkdir(parents=True, exist_ok=True)
        timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
        resolved = resolved / f"mps-trotter-quench-{timestamp}.json"
    else:
        resolved.parent.mkdir(parents=True, exist_ok=True)

    repository_root = Path(__file__).resolve().parents[3]
    if resolved.is_relative_to(repository_root):
        raise ValueError(
            f"refusing to write evidence inside repository tree: {resolved}"
        )
    return resolved


def run_record(
    evidence: Evidence,
    *,
    phase: str,
    label: str,
    action: str,
    backend: str,
    entry_point: str,
    width: int,
    depth: int,
    theta: float,
    shots: int | None,
    seed: int,
    timeout_seconds: float,
    qir: str | None = None,
    retain_shots: bool = False,
    return_qir: bool = False,
) -> tuple[dict[str, Any], dict[str, Any]]:
    if backend == "mps" and not MPS_AVAILABLE:
        response = {
            "outcome": "unavailable",
            "error_type": "MpsUnavailable",
            "error": MPS_UNAVAILABLE_REASON,
            "total_wall_seconds": 0.0,
            "peak_host_memory_bytes": _peak_rss_bytes(),
            "process_exit_code": None,
        }
    else:
        request = {
            "action": action,
            "backend": backend,
            "width": width,
            "depth": depth,
            "theta": theta,
            "shots": shots,
            "seed": seed,
            "qir": qir,
            "retain_shots": retain_shots,
            "return_qir": return_qir,
        }
        response = run_isolated(request, timeout_seconds)

    total = response["total_wall_seconds"]
    record = {
        "phase": phase,
        "label": label,
        "width": width,
        "depth": depth,
        "theta": theta,
        "evolution_gate_count": evolution_gate_count(width, depth),
        "shots": shots,
        "seed": seed,
        "backend": backend,
        "entry_point": entry_point,
        "timeout_seconds": timeout_seconds,
        "total_wall_seconds": total,
        "per_shot_wall_seconds": total / shots if shots else None,
        "simulator_wall_seconds": response.get("simulator_wall_seconds"),
        "peak_host_memory_bytes": response.get("peak_host_memory_bytes"),
        "outcome": response["outcome"],
        "error_type": response.get("error_type"),
        "error": response.get("error"),
        "process_exit_code": response.get("process_exit_code"),
    }
    for key in (
        "returned_shots",
        "distinct_outcomes",
        "histogram",
        "one_frequencies",
        "first_outcomes",
        "qir_sha256",
        "qir_characters",
        "traceback",
    ):
        if key in response:
            record[key] = response[key]
    evidence.add_run(record)

    detail = (
        f"total={total:.3f}s per_shot={record['per_shot_wall_seconds']:.3f}s"
        if shots
        else f"total={total:.3f}s"
    )
    print(
        f"{phase} | {label} | width={width} | {backend} | "
        f"{response['outcome']} | {detail}",
        flush=True,
    )
    if response.get("error"):
        print(
            f"{phase} | {label} | error={response['error'].replace(chr(10), ' | ')}",
            flush=True,
        )
    return record, response


def succeeded(record: dict[str, Any]) -> bool:
    return record["outcome"] == "success"


def max_frequency_deviation(*frequency_sets: list[float]) -> float:
    if not frequency_sets:
        return 0.0
    width = len(frequency_sets[0])
    if any(len(frequencies) != width for frequencies in frequency_sets):
        raise ValueError("cannot compare frequency vectors with different widths")
    return max(
        max(values) - min(values)
        for values in zip(*frequency_sets, strict=True)
    )


def remaining_timeout(
    configured_timeout: float,
    demo_deadline: float | None,
) -> float:
    if demo_deadline is None:
        return configured_timeout
    remaining = demo_deadline - time.perf_counter()
    if remaining <= 0:
        raise TimeoutError("demo exceeded its end-to-end time budget")
    return min(configured_timeout, remaining)


def run_equivalence(
    evidence: Evidence,
    args: argparse.Namespace,
    demo_deadline: float | None,
) -> bool:
    phase = "preflight-qir-equivalence"
    compile_record, compile_response = run_record(
        evidence,
        phase=phase,
        label="compile-inline-qsharp",
        action="compile_qsharp",
        backend="qsharp-compiler",
        entry_point="qsharp.compile",
        width=args.equivalence_width,
        depth=args.depth,
        theta=args.theta,
        shots=None,
        seed=args.seed,
        timeout_seconds=remaining_timeout(args.timeout_seconds, demo_deadline),
        return_qir=True,
    )
    if not succeeded(compile_record):
        return False

    direct_record, direct_response = run_record(
        evidence,
        phase=phase,
        label="direct-qir-cpu",
        action="run_qir",
        backend="cpu",
        entry_point="run_qir",
        width=args.equivalence_width,
        depth=args.depth,
        theta=args.theta,
        shots=args.equivalence_shots,
        seed=args.seed,
        timeout_seconds=remaining_timeout(args.timeout_seconds, demo_deadline),
        retain_shots=True,
    )
    if not succeeded(direct_record):
        return False

    compiled_record, compiled_response = run_record(
        evidence,
        phase=phase,
        label="compiled-qsharp-qir-cpu",
        action="run_qir",
        backend="cpu",
        entry_point="run_qir",
        width=args.equivalence_width,
        depth=args.depth,
        theta=args.theta,
        shots=args.equivalence_shots,
        seed=args.seed,
        timeout_seconds=remaining_timeout(args.timeout_seconds, demo_deadline),
        qir=compile_response["compiled_qir"],
        retain_shots=True,
    )
    if not succeeded(compiled_record):
        return False

    equivalent = (
        direct_response["shot_bitstrings"]
        == compiled_response["shot_bitstrings"]
    )
    comparison: dict[str, Any] = {
        "status": "executed",
        "width": args.equivalence_width,
        "shots": args.equivalence_shots,
        "exact_seeded_shot_match": equivalent,
        "direct_qir_sha256": hashlib.sha256(
            generate_qir(
                args.equivalence_width,
                args.depth,
                args.theta,
            ).encode()
        ).hexdigest(),
        "compiled_qir_sha256": compile_response["qir_sha256"],
    }
    if not equivalent:
        comparison["direct_one_frequencies"] = direct_record["one_frequencies"]
        comparison["compiled_one_frequencies"] = compiled_record[
            "one_frequencies"
        ]
    evidence.analyze(
        "qir_equivalence",
        comparison,
    )
    print(
        f"{phase} | exact_seeded_shot_match={str(equivalent).lower()}",
        flush=True,
    )
    if not equivalent:
        print(
            f"{phase} | direct One frequencies="
            f"{comparison['direct_one_frequencies']}",
            flush=True,
        )
        print(
            f"{phase} | compiled One frequencies="
            f"{comparison['compiled_one_frequencies']}",
            flush=True,
        )
    return True


def run_phase_1(
    evidence: Evidence,
    args: argparse.Namespace,
    demo_deadline: float | None,
) -> tuple[str, dict[str, Any]]:
    phase = "phase-1-correctness"
    probe_record, _ = run_record(
        evidence,
        phase=phase,
        label="cpu-one-shot-probe",
        action="run_qir",
        backend="cpu",
        entry_point="run_qir",
        width=args.correctness_width,
        depth=args.depth,
        theta=args.theta,
        shots=1,
        seed=args.seed,
        timeout_seconds=remaining_timeout(args.timeout_seconds, demo_deadline),
    )
    simulator_wall = probe_record["simulator_wall_seconds"]
    shots = (
        args.correctness_fallback_shots
        if succeeded(probe_record)
        and simulator_wall is not None
        and simulator_wall > args.cpu_shot_threshold_seconds
        else args.correctness_shots
    )
    if shots != args.correctness_shots:
        print(
            f"{phase} | CPU one-shot time {simulator_wall:.3f}s exceeds "
            f"{args.cpu_shot_threshold_seconds:.3f}s; using {shots} shots",
            flush=True,
        )

    cpu_record, _ = run_record(
        evidence,
        phase=phase,
        label="cpu",
        action="run_qir",
        backend="cpu",
        entry_point="run_qir",
        width=args.correctness_width,
        depth=args.depth,
        theta=args.theta,
        shots=shots,
        seed=args.seed,
        timeout_seconds=remaining_timeout(args.timeout_seconds, demo_deadline),
    )
    mps_record, _ = run_record(
        evidence,
        phase=phase,
        label="mps",
        action="run_qir",
        backend="mps",
        entry_point="run_qir",
        width=args.correctness_width,
        depth=args.depth,
        theta=args.theta,
        shots=shots,
        seed=args.seed,
        timeout_seconds=remaining_timeout(args.timeout_seconds, demo_deadline),
    )
    context = {"shots": shots, "cpu": cpu_record, "mps": mps_record}
    cpu_frequencies = cpu_record.get("one_frequencies")
    mps_frequencies = mps_record.get("one_frequencies")
    if mps_record["outcome"] == "unavailable":
        evidence.analyze(
            "phase_1_correctness",
            {
                "status": "cpu-half-complete-mps-and-deviation-pending-vm",
                "reason": mps_record["error"],
                "width": args.correctness_width,
                "shots": shots,
                "cpu_one_frequencies": cpu_frequencies,
                "mps_one_frequencies": None,
                "maximum_per_qubit_one_frequency_deviation": None,
                "run_outcomes": {
                    "cpu": cpu_record["outcome"],
                    "mps": mps_record["outcome"],
                },
            },
        )
        print(f"{phase} | cpu One frequencies={cpu_frequencies}", flush=True)
        print(f"{phase} | mps One frequencies=pending VM", flush=True)
        print(
            f"{phase} | maximum per-qubit One-frequency deviation=pending VM",
            flush=True,
        )
        print(
            f"{phase} | CPU half complete; MPS half and maximum per-qubit "
            "deviation pending VM",
            flush=True,
        )
        return "pending-vm", context

    both_produced_results = succeeded(cpu_record) and succeeded(mps_record)
    deviation = (
        max_frequency_deviation(cpu_frequencies, mps_frequencies)
        if both_produced_results
        else None
    )
    status = (
        "results-recorded"
        if both_produced_results
        else "completed-with-execution-findings"
    )
    evidence.analyze(
        "phase_1_correctness",
        {
            "status": status,
            "width": args.correctness_width,
            "shots": shots,
            "cpu_one_frequencies": cpu_frequencies,
            "mps_one_frequencies": mps_frequencies,
            "maximum_per_qubit_one_frequency_deviation": deviation,
            "run_outcomes": {
                "cpu": cpu_record["outcome"],
                "mps": mps_record["outcome"],
            },
        },
    )
    print(f"{phase} | cpu One frequencies={cpu_frequencies}", flush=True)
    print(f"{phase} | mps One frequencies={mps_frequencies}", flush=True)
    print(
        f"{phase} | maximum per-qubit One-frequency deviation={deviation}",
        flush=True,
    )
    return status, context


def run_phase_2(
    evidence: Evidence,
    args: argparse.Namespace,
    demo_deadline: float | None,
) -> tuple[str, list[dict[str, Any]]]:
    phase = "phase-2-scale"
    planned_runs = [
        ("mandatory-width-guard", args.guard_width, args.guard_shots),
        *(
            (f"headline-{shots}-shot", args.headline_width, shots)
            for shots in args.headline_shots
        ),
    ]
    records = []
    for label, width, shots in planned_runs:
        try:
            timeout_seconds = remaining_timeout(
                args.timeout_seconds,
                demo_deadline,
            )
        except TimeoutError as error:
            record = {
                "phase": phase,
                "label": label,
                "width": width,
                "depth": args.depth,
                "theta": args.theta,
                "evolution_gate_count": evolution_gate_count(width, args.depth),
                "shots": shots,
                "seed": args.seed,
                "backend": "mps",
                "entry_point": "run_qir",
                "timeout_seconds": 0.0,
                "total_wall_seconds": 0.0,
                "per_shot_wall_seconds": 0.0,
                "simulator_wall_seconds": None,
                "peak_host_memory_bytes": _peak_rss_bytes(),
                "outcome": "not-run-demo-budget-exhausted",
                "error_type": type(error).__name__,
                "error": str(error),
                "process_exit_code": None,
            }
            evidence.add_run(record)
            records.append(record)
            continue

        record, _ = run_record(
            evidence,
            phase=phase,
            label=label,
            action="run_qir",
            backend="mps",
            entry_point="run_qir",
            width=width,
            depth=args.depth,
            theta=args.theta,
            shots=shots,
            seed=args.seed,
            timeout_seconds=timeout_seconds,
        )
        records.append(record)

    headline_records = [
        record for record in records if record["width"] == args.headline_width
    ]
    successful_headline = [
        record for record in headline_records if succeeded(record)
    ]
    scaling: dict[str, Any] = {
        "status": "insufficient-successful-runs",
        "smallest_measured_shot_count_showing_distribution": None,
    }
    if len(successful_headline) >= 2:
        first, second = successful_headline[:2]
        ratio = second["total_wall_seconds"] / first["total_wall_seconds"]
        if ratio >= 1.6:
            observation = "consistent-with-per-shot-re-evolution"
        elif ratio <= 1.4:
            observation = "consistent-with-shared-evolution"
        else:
            observation = "inconclusive"
        scaling = {
            "status": "measured",
            "first_shot_count": first["shots"],
            "first_total_seconds": first["total_wall_seconds"],
            "second_shot_count": second["shots"],
            "second_total_seconds": second["total_wall_seconds"],
            "second_to_first_total_time_ratio": ratio,
            "observation": observation,
            "smallest_measured_shot_count_showing_distribution": next(
                (
                    record["shots"]
                    for record in successful_headline
                    if record.get("distinct_outcomes", 0) > 1
                ),
                None,
            ),
        }
        print(
            f"{phase} | {second['shots']}-shot/{first['shots']}-shot total "
            f"ratio={ratio:.3f} | {observation}",
            flush=True,
        )

    outcomes = Counter(record["outcome"] for record in records)
    status = "completed" if all(succeeded(record) for record in records) else "completed-with-findings"
    evidence.analyze(
        "phase_2_scale",
        {
            "status": status,
            "run_outcomes": dict(sorted(outcomes.items())),
            "scaling": scaling,
        },
    )
    return status, records


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("demo", "campaign"), default="demo")
    parser.add_argument("--output", type=Path, default=Path.cwd())
    parser.add_argument("--depth", type=int, default=DEFAULT_DEPTH)
    parser.add_argument("--theta", type=float, default=DEFAULT_THETA)
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED)
    parser.add_argument("--timeout-seconds", type=float, default=240.0)
    parser.add_argument(
        "--demo-time-budget-seconds",
        type=float,
        default=DEMO_TIME_BUDGET_SECONDS,
    )
    parser.add_argument("--equivalence-width", type=int, default=8)
    parser.add_argument("--equivalence-shots", type=int, default=20)
    parser.add_argument("--correctness-width", type=int, default=16)
    parser.add_argument("--correctness-shots", type=int, default=1000)
    parser.add_argument("--correctness-fallback-shots", type=int, default=400)
    parser.add_argument("--cpu-shot-threshold-seconds", type=float, default=0.5)
    parser.add_argument("--guard-width", type=int, default=128)
    parser.add_argument("--guard-shots", type=int, default=20)
    parser.add_argument("--headline-width", type=int, default=1024)
    parser.add_argument("--headline-shots", type=int, nargs="+", default=[1, 2])
    parser.add_argument(
        "--campaign-widths", type=int, nargs="+", default=[8, 12, 16, 20, 24]
    )
    parser.add_argument("--campaign-shots", type=int, default=5)
    parser.add_argument(
        "--compile-widths", type=int, nargs="+", default=[8, 16, 32, 64]
    )
    parser.add_argument("--include-wall-probes", action="store_true")
    parser.add_argument(
        "--wall-probe-widths", type=int, nargs="+", default=[26, 28]
    )
    parser.add_argument("--wall-probe-shots", type=int, default=1)
    parser.add_argument("--oversized-width", type=int, default=40)
    parser.add_argument("--oversized-shots", type=int, default=1)
    parser.add_argument("--clifford-width", type=int, default=8)
    parser.add_argument("--clifford-shots", type=int, default=1)
    parser.add_argument(
        "--include-verified-oversized-probe",
        action="store_true",
        help="Run the oversized dense probe in demo mode only after campaign mode proved it raises cleanly.",
    )
    args = parser.parse_args()

    widths = [
        args.equivalence_width,
        args.correctness_width,
        args.guard_width,
        args.headline_width,
        args.oversized_width,
        args.clifford_width,
        *args.campaign_widths,
        *args.compile_widths,
        *args.wall_probe_widths,
    ]
    shot_counts = [
        args.equivalence_shots,
        args.correctness_shots,
        args.correctness_fallback_shots,
        args.guard_shots,
        args.oversized_shots,
        args.clifford_shots,
        args.campaign_shots,
        args.wall_probe_shots,
        *args.headline_shots,
    ]
    if any(width < 2 for width in widths):
        parser.error("all widths must be at least two")
    if any(shots < 1 for shots in shot_counts):
        parser.error("all shot counts must be positive")
    if len(args.headline_shots) < 2:
        parser.error("--headline-shots requires at least two measurements")
    if args.depth < 1:
        parser.error("--depth must be positive")
    if args.timeout_seconds <= 0 or args.demo_time_budget_seconds <= 0:
        parser.error("timeouts and budgets must be positive")
    return args


def main() -> int:
    args = parse_args()
    destination = resolve_output_path(args.output)
    arguments = {
        key: str(value) if isinstance(value, Path) else value
        for key, value in vars(args).items()
    }
    evidence = Evidence(destination, arguments)
    started = time.perf_counter()
    demo_deadline = (
        started + args.demo_time_budget_seconds if args.mode == "demo" else None
    )
    print(f"evidence | {destination}", flush=True)
    print(
        f"circuit | theta={args.theta} depth={args.depth} | "
        f"evolution gate count={args.depth}*(4n-3)",
        flush=True,
    )

    try:
        if not run_equivalence(evidence, args, demo_deadline):
            evidence.finish("failed-qir-equivalence")
            return 1

        phase_1_status, _ = run_phase_1(evidence, args, demo_deadline)
        if phase_1_status == "pending-vm":
            evidence.analyze(
                "phase_2_scale",
                {
                    "status": "not-run-phase-1-gate-pending-vm",
                    "planned_runs": [
                        {
                            "width": args.guard_width,
                            "shots": args.guard_shots,
                        },
                        *(
                            {"width": args.headline_width, "shots": shots}
                            for shots in args.headline_shots
                        ),
                    ],
                },
            )
            evidence.analyze(
                "phase_3_campaign",
                {"status": "deferred-next-iteration"},
            )
            evidence.finish("phase-1-cpu-complete-mps-pending-vm")
            elapsed = time.perf_counter() - started
            print(
                "phase-1-correctness | CPU half complete; MPS half and "
                "maximum per-qubit deviation pending VM",
                flush=True,
            )
            print(
                f"hold | Phase 2 gated | total={elapsed:.3f}s | "
                f"task_1={TASK_1_COMMIT}",
                flush=True,
            )
            return 2
        phase_2_status, phase_2_records = run_phase_2(
            evidence,
            args,
            demo_deadline,
        )
        evidence.analyze(
            "phase_3_campaign",
            {"status": "deferred-next-iteration"},
        )

        elapsed = time.perf_counter() - started
        successful_headline = [
            record
            for record in phase_2_records
            if record["width"] == args.headline_width and succeeded(record)
        ]
        evidence.analyze(
            "demo_candidate",
            {
                "status": "measured-not-pinned",
                "headline_width": args.headline_width,
                "smallest_measured_shot_count_showing_distribution": next(
                    (
                        record["shots"]
                        for record in successful_headline
                        if record.get("distinct_outcomes", 0) > 1
                    ),
                    None,
                ),
                "phase_2_status": phase_2_status,
                "measured_end_to_end_seconds": elapsed,
                "within_five_minutes": elapsed < 300.0,
            },
        )
        evidence.finish(f"phase-1-{phase_1_status}-phase-2-{phase_2_status}")
        print(
            f"complete | mode={args.mode} | Phase 1 {phase_1_status} | "
            f"Phase 2 {phase_2_status} | total={elapsed:.3f}s | "
            f"task_1={TASK_1_COMMIT}",
            flush=True,
        )
        return 0
    except TimeoutError as error:
        evidence.analyze("harness_error", str(error))
        evidence.finish("timed-out-before-phase-2-completed")
        print(f"harness | timeout | {error}", flush=True)
        return 1
    except BaseException as error:
        evidence.analyze(
            "harness_error",
            {
                "type": type(error).__name__,
                "error": str(error),
                "traceback": traceback.format_exc(),
            },
        )
        evidence.finish("failed")
        print(
            f"harness | error | {type(error).__name__}: "
            f"{str(error).replace(chr(10), ' | ')}",
            flush=True,
        )
        return 1


if __name__ == "__main__":
    multiprocessing.freeze_support()
    raise SystemExit(main())
