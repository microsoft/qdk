# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Centralized mock helpers for tests.

Provides lightweight stand-ins for optional (and required during tests) dependencies.

Functions return a list of module names they created so callers can later clean them
up using cleanup_modules(). This keeps test intent explicit.
"""

import sys
import types
from typing import List


def _not_impl(*_a, **_k):
    raise NotImplementedError("qsharp stub: real 'qsharp' package not installed")


def mock_qsharp() -> List[str]:
    """Ensure a minimal 'qsharp' module exists.

    In real usage the qsharp package provides a compiled extension. Tests only
    need the attribute surface that qdk re-exports (run/estimate presently used
    for sanity checks). If the real package is installed this is a no-op.
    """
    created: List[str] = []
    if "qsharp" not in sys.modules:
        stub = types.ModuleType("qsharp")

        stub.run = _not_impl
        stub.estimate = _not_impl
        # Provide utility symbols expected to re-export at root
        stub.code = object()
        stub.set_quantum_seed = _not_impl
        stub.set_classical_seed = _not_impl
        stub.dump_machine = _not_impl
        stub.init = _not_impl

        class _T:  # placeholder types
            pass

        stub.Result = _T
        stub.TargetProfile = _T
        stub.StateDump = _T
        stub.ShotResult = _T
        stub.PauliNoise = _T
        stub.DepolarizingNoise = _T
        stub.BitFlipNoise = _T
        stub.PhaseFlipNoise = _T
        stub.__all__ = [
            "run",
            "estimate",
            "code",
            "set_quantum_seed",
            "set_classical_seed",
            "dump_machine",
            "init",
            "Result",
            "TargetProfile",
            "StateDump",
            "ShotResult",
            "PauliNoise",
            "DepolarizingNoise",
            "BitFlipNoise",
            "PhaseFlipNoise",
            "estimator",
            "openqasm",
        ]
        # Minimal submodules to back lifted shims
        est = types.ModuleType("qsharp.estimator")
        est.__doc__ = "mock estimator"
        sys.modules["qsharp.estimator"] = est
        stub.estimator = est
        oq = types.ModuleType("qsharp.openqasm")
        oq.__doc__ = "mock openqasm"
        sys.modules["qsharp.openqasm"] = oq
        stub.openqasm = oq

        sys.modules["qsharp"] = stub
        # Telemetry events package with on_qdk_import function expected by qdk import
        telemetry_pkg = types.ModuleType("qsharp.telemetry_events")

        def on_qdk_import():
            return None

        telemetry_pkg.on_qdk_import = on_qdk_import
        sys.modules["qsharp.telemetry_events"] = telemetry_pkg
        # Interop namespace for qiskit shim expectations
        interop = types.ModuleType("qsharp.interop")
        sys.modules["qsharp.interop"] = interop
        interop_qk = types.ModuleType("qsharp.interop.qiskit")
        interop_qk.__doc__ = "mock qsharp interop qiskit"
        sys.modules["qsharp.interop.qiskit"] = interop_qk

        created.extend(
            [
                "qsharp",
                "qsharp.estimator",
                "qsharp.openqasm",
                "qsharp.telemetry_events",
                "qsharp.interop",
                "qsharp.interop.qiskit",
            ]
        )
    return created


def mock_widgets() -> List[str]:
    created: List[str] = []
    if "qsharp_widgets" not in sys.modules:
        mod = types.ModuleType("qsharp_widgets")
        sys.modules["qsharp_widgets"] = mod
        created.append("qsharp_widgets")
    return created


def mock_azure() -> List[str]:
    created: List[str] = []
    if "azure" not in sys.modules:
        sys.modules["azure"] = types.ModuleType("azure")
        created.append("azure")
    if "azure.quantum" not in sys.modules:
        aq = types.ModuleType("azure.quantum")
        # Minimal submodules expected by qdk.azure shim
        tgt = types.ModuleType("azure.quantum.target")
        argt = types.ModuleType("azure.quantum.argument_types")
        job = types.ModuleType("azure.quantum.job")
        # Register in sys.modules first
        sys.modules["azure.quantum.target"] = tgt
        sys.modules["azure.quantum.argument_types"] = argt
        sys.modules["azure.quantum.job"] = job
        # Attach to parent for attribute access
        aq.target = tgt
        aq.argument_types = argt
        aq.job = job
        sys.modules["azure.quantum"] = aq
        created.extend(
            [
                "azure.quantum",
                "azure.quantum.target",
                "azure.quantum.argument_types",
                "azure.quantum.job",
            ]
        )
    return created


def mock_qiskit() -> List[str]:
    created: List[str] = []
    if "qiskit" not in sys.modules:
        qk = types.ModuleType("qiskit")
        qk.transpile = _not_impl
        sys.modules["qiskit"] = qk
        created.append("qiskit")
    return created


def cleanup_modules(created: List[str]) -> None:
    """Remove synthetic modules created during a test if still present."""
    for name in created:
        sys.modules.pop(name, None)
