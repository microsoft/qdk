# Configuration file for the Sphinx documentation builder.
# https://www.sphinx-doc.org/en/master/usage/configuration.html

import os
import sys

# Add the pip package root to sys.path so Sphinx can import qsharp.
sys.path.insert(0, os.path.abspath(".."))

# ---------------------------------------------------------------------------
# Pre-populate sys.modules with properly-typed stubs for compiled extensions
# and optional dependencies.
#
# Sphinx autodoc injecting MagicMock() instances is not enough: source files
# that use PEP-604 union syntax (e.g. `Output | str`) or access enum members
# (e.g. `TargetProfile.Unrestricted`) at module-load time need real types.
# We provide thin-but-valid class/enum stubs before Sphinx auto-mocking runs.
# ---------------------------------------------------------------------------
import types as _types
import sys as _sys
from enum import Enum as _Enum


def _stub_module(name: str, **attrs) -> _types.ModuleType:
    mod = _types.ModuleType(name)
    for k, v in attrs.items():
        setattr(mod, k, v)
    _sys.modules[name] = mod
    return mod


def _cls(name: str, module: str = "qsharp._native") -> type:
    return type(name, (), {"__doc__": f"Stub for {name}", "__module__": module})


def _stub_fn(*_a, **_kw):
    return None


# ── qsharp._native enums ─────────────────────────────────────────────────────


class _TargetProfile(_Enum):
    Base = "base"
    Adaptive_RI = "adaptive_ri"
    Adaptive_RIF = "adaptive_rif"
    Adaptive_RIFLA = "adaptive_rifla"
    Unrestricted = "unrestricted"

    @classmethod
    def from_str(cls, value: str):
        return cls(value)


class _Result(_Enum):
    Zero = 0
    One = 1
    Loss = 2


class _Pauli(_Enum):
    I = 0
    X = 1
    Y = 2
    Z = 3


class _CircuitGenerationMethod(_Enum):
    ClassicalEval = "classical_eval"
    Simulate = "simulate"
    Static = "static"


class _OutputSemantics(_Enum):
    Qiskit = "qiskit"
    OpenQasm = "openqasm"
    ResourceEstimation = "resource_estimation"


class _ProgramType(_Enum):
    File = "file"
    Operation = "operation"
    Fragments = "fragments"


class _TypeKind(_Enum):
    Primitive = 0
    Tuple = 1
    Array = 2
    Udt = 3


class _PrimitiveKind(_Enum):
    Bool = 0
    Int = 1
    Double = 2
    Complex = 3
    String = 4
    Pauli = 5
    Result = 6


class _QirInstructionId(_Enum):
    I = 0
    H = 1
    X = 2
    Y = 3
    Z = 4
    S = 5
    T = 6
    CNOT = 7
    M = 8


_stub_module(
    "qsharp._native",
    TargetProfile=_TargetProfile,
    Result=_Result,
    Pauli=_Pauli,
    CircuitGenerationMethod=_CircuitGenerationMethod,
    OutputSemantics=_OutputSemantics,
    ProgramType=_ProgramType,
    TypeKind=_TypeKind,
    PrimitiveKind=_PrimitiveKind,
    QirInstructionId=_QirInstructionId,
    **{
        n: _cls(n)
        for n in [
            "Interpreter",
            "StateDumpData",
            "QSharpError",
            "Output",
            "Circuit",
            "GlobalCallable",
            "Closure",
            "UdtValue",
            "TypeIR",
            "CircuitConfig",
            "NoiseConfig",
            "NoiseTable",
            "NoiseIntrinsicsTable",
            "QasmError",
            "QirInstruction",
            "IdleNoiseParams",
            "GpuContext",
            "GpuShotResults",
            "UdtIR",
        ]
    },
    **{
        fn: _stub_fn
        for fn in [
            "physical_estimates",
            "compile_qasm_program_to_qir",
            "compile_qasm_to_qsharp",
            "circuit_qasm_program",
            "resource_estimate_qasm_program",
            "run_qasm_program",
            "estimate_custom",
            "run_clifford",
            "run_cpu_full_state",
            "run_parallel_shots",
            "run_adaptive_parallel_shots",
            "try_create_gpu_adapter",
        ]
    },
)

# ── qsharp.noisy_simulator._noisy_simulator ─────────────────────────────────
_ns = "qsharp.noisy_simulator._noisy_simulator"
_stub_module(
    _ns,
    **{
        n: _cls(n, module=_ns)
        for n in [
            "NoisySimulatorError",
            "Operation",
            "Instrument",
            "DensityMatrixSimulator",
            "StateVectorSimulator",
        ]
    },
)

# ── Optional interop: qiskit ─────────────────────────────────────────────────
_qiskit_circuit_classes = [
    "QuantumCircuit",
    "QuantumRegister",
    "ClassicalRegister",
    "Barrier",
    "Gate",
    "Instruction",
    "Delay",
    "Measure",
    "Reset",
    "Parameter",
    "ParameterVector",
]
_stub_module(
    "qiskit",
    QuantumCircuit=_cls("QuantumCircuit", module="qiskit"),
    QiskitError=_cls("QiskitError", module="qiskit"),
)
_stub_module(
    "qiskit.circuit",
    **{n: _cls(n, module="qiskit.circuit") for n in _qiskit_circuit_classes},
)
for _sub in [
    "qiskit.providers",
    "qiskit.transpiler",
    "qiskit.primitives",
    "qiskit.result",
    "qiskit.qasm2",
    "qiskit.qasm3",
    "qiskit_aer",
]:
    _stub_module(_sub)

# ── Optional interop: cirq ───────────────────────────────────────────────────
_stub_module(
    "cirq",
    **{
        n: _cls(n, module="cirq")
        for n in [
            "ResultDict",
            "Study",
            "Sampler",
            "Circuit",
            "Moment",
            "Gate",
            "Operation",
            "AbstractCircuit",
            "LineQubit",
        ]
    },
)
for _sub in ["cirq_core", "cirq.ops", "cirq.protocols", "cirq.result"]:
    _stub_module(_sub)

# ---------------------------------------------------------------------------
# Project information
# ---------------------------------------------------------------------------
project = "qsharp"
copyright = "Microsoft Corporation"
author = "Microsoft Quantum"

# ---------------------------------------------------------------------------
# General configuration
# ---------------------------------------------------------------------------
extensions = [
    "sphinx.ext.autodoc",
    "sphinx.ext.napoleon",
    "sphinx.ext.viewcode",
    "sphinx.ext.intersphinx",
    "sphinx.ext.autosummary",
]

autodoc_mock_imports: list[str] = []

# ---------------------------------------------------------------------------
# Autodoc settings
# ---------------------------------------------------------------------------
autodoc_default_options = {
    "members": True,
    "member-order": "bysource",
    "undoc-members": False,
    "show-inheritance": True,
    "special-members": "__init__",
    "exclude-members": "__weakref__, __dict__, __module__, __abstractmethods__",
}
autodoc_typehints = "description"
autodoc_typehints_description_target = "documented"
autoclass_content = "both"

# ---------------------------------------------------------------------------
# Napoleon settings
# ---------------------------------------------------------------------------
napoleon_google_docstring = True
napoleon_numpy_docstring = True
napoleon_include_init_with_doc = True
napoleon_include_private_with_doc = False
napoleon_include_special_with_doc = True
napoleon_use_param = True
napoleon_use_rtype = True
napoleon_preprocess_types = True

# ---------------------------------------------------------------------------
# Intersphinx mapping
# ---------------------------------------------------------------------------
intersphinx_mapping = {
    "python": ("https://docs.python.org/3", None),
}

# ---------------------------------------------------------------------------
# HTML output settings
# ---------------------------------------------------------------------------
html_theme = "furo"
html_title = "Q# Python API Reference"
templates_path = ["_templates"]
exclude_patterns = ["_build", "Thumbs.db", ".DS_Store"]
