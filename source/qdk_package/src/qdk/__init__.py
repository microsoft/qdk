# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""qdk bundling meta-package.

Design goals:
    * Provide a single import root `qdk` that exposes bundled quantum tooling as
        submodules (`qdk.qsharp`, `qdk.widgets`, etc.).

Optional extras:
    azure   -> installs `azure-quantum`, available as `qdk.azure`.
    qiskit  -> installs `qiskit`, available as `qdk.qiskit`.
    jupyter -> installs `qsharp-widgets` + `qsharp-jupyterlab`; exposes `qdk.widgets`.

"""

from importlib import import_module

# Eagerly import qdk.qsharp to ensure qdk.qsharp is always available if qdk is.
try:
    from . import qsharp as qsharp
except Exception as _ex:
    raise

# Optional: use telemetry hook if present (skipped in stub/mock envs)
try:
    import qsharp.telemetry_events.on_qdk_import

    qsharp.telemetry_events.on_qdk_import()
except Exception:
    pass

# Some common utilities are lifted to the qdk root.
from qsharp import code
from qsharp import (
    set_quantum_seed,
    set_classical_seed,
    dump_machine,
    dump_circuit,
    Result,
    TargetProfile,
    StateDump,
    ShotResult,
    PauliNoise,
    DepolarizingNoise,
    BitFlipNoise,
    PhaseFlipNoise,
)

__all__ = [
    "qsharp",
    "estimator",
    "openqasm",
    "widgets_available",
    "azure_available",
    "qiskit_available",
    "require",
    # utilities
    "code",
    "set_quantum_seed",
    "set_classical_seed",
    "dump_machine",
    "dump_circuit",
    "Result",
    "TargetProfile",
    "StateDump",
    "ShotResult",
    "PauliNoise",
    "DepolarizingNoise",
    "BitFlipNoise",
    "PhaseFlipNoise",
]


def widgets_available() -> bool:
    """Return True if widget support (via the jupyter extra) is installed."""
    try:
        import_module("qsharp_widgets")
        return True
    except Exception:
        return False


def azure_available() -> bool:
    """Return True if the azure extra (azure-quantum) is installed."""
    try:
        import_module(
            "azure.quantum"
        )  # azure-quantum installs namespace under azure.quantum
        return True
    except Exception:
        return False


def qiskit_available() -> bool:
    """Return True if the qiskit extra is installed."""
    try:
        import_module("qiskit")
        return True
    except Exception:
        return False


def require(feature: str):
    """Return the module backing a named feature or raise ImportError.

    Recognized features:
        * 'qsharp'    -> qdk.qsharp
        * 'widgets'   -> qdk.widgets (requires jupyter extra)
        * 'jupyter'   -> alias for widgets (requires jupyter extra)
        * 'azure'     -> qdk.azure  (requires azure extra)
        * 'qiskit'    -> qdk.qiskit (requires qiskit extra)
    """
    if feature == "qsharp":
        return qsharp
    if feature in ("widgets", "jupyter"):
        if not widgets_available():
            raise ImportError(
                "Feature 'jupyter' (widgets) unavailable. Install with 'pip install qdk[jupyter]'."
            )
        return import_module("qdk.widgets")
    if feature == "azure":
        if not azure_available():
            raise ImportError(
                "Feature 'azure' unavailable. Install with 'pip install qdk[azure]'."
            )
        return import_module("qdk.azure")
    if feature == "qiskit":
        if not qiskit_available():
            raise ImportError(
                "Feature 'qiskit' unavailable. Install with 'pip install qdk[qiskit]'."
            )
        return import_module("qdk.qiskit")
    raise ImportError(
        f"Feature '{feature}' is not recognized. Available: qsharp, widgets, jupyter, azure, qiskit (if installed)."
    )
