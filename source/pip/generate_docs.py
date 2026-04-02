#!/usr/bin/env python3
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""Generate DocFX YAML API documentation for the qsharp Python package.

Output format matches ``python-quantum/python/docs-ref-autogen/qsharp/``
for publishing to Microsoft Learn.

Usage::

    python generate_docs.py [output_dir]

``output_dir`` defaults to ``python-api-yaml/`` next to this script.

Like the Q# standard-library doc tool (``qsc_doc_gen``/``generate_docs.js``),
this script generates documentation artefacts that can be handed to the
docs team for publishing to learn.microsoft.com.
"""

import inspect
import os
import sys
import types as _types
from enum import EnumMeta

# ---------------------------------------------------------------------------
# Add the pip package root to sys.path so we can import qsharp.
# ---------------------------------------------------------------------------
_SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, _SCRIPT_DIR)

# ---------------------------------------------------------------------------
# Pre-populate sys.modules with properly-typed stubs for compiled extensions
# and optional dependencies, exactly as in docs/conf.py.
# ---------------------------------------------------------------------------
from enum import Enum as _Enum


def _stub_module(name: str, **attrs) -> _types.ModuleType:
    mod = _types.ModuleType(name)
    for k, v in attrs.items():
        setattr(mod, k, v)
    sys.modules[name] = mod
    return mod


def _cls(name: str, module: str = "qsharp._native") -> type:
    return type(name, (), {"__doc__": f"Stub for {name}.", "__module__": module})


def _stub_fn(*_a, **_kw):
    return None


# ── qsharp._native enums ─────────────────────────────────────────────────────


class _TargetProfile(_Enum):
    """
    A Q# target profile.

    A target profile describes the capabilities of the hardware or simulator
    which will be used to run the Q# program.
    """

    Base = "base"
    Adaptive_RI = "adaptive_ri"
    Adaptive_RIF = "adaptive_rif"
    Adaptive_RIFLA = "adaptive_rifla"
    Unrestricted = "unrestricted"

    @classmethod
    def from_str(cls, value: str):
        return cls(value)


class _Result(_Enum):
    """A Q# measurement result."""

    Zero = 0
    One = 1
    Loss = 2


class _Pauli(_Enum):
    """A Q# Pauli operator."""

    I = 0
    X = 1
    Y = 2
    Z = 3


class _CircuitGenerationMethod(_Enum):
    """The method to use for circuit generation."""

    ClassicalEval = "classical_eval"
    Simulate = "simulate"
    Static = "static"


class _OutputSemantics(_Enum):
    """Represents the output semantics for OpenQASM 3 compilation."""

    Qiskit = "qiskit"
    OpenQasm = "openqasm"
    ResourceEstimation = "resource_estimation"


class _ProgramType(_Enum):
    """Represents the type of compilation output to create."""

    File = "file"
    Operation = "operation"
    Fragments = "fragments"


class _TypeKind(_Enum):
    """A Q# type kind."""

    Primitive = 0
    Tuple = 1
    Array = 2
    Udt = 3


class _PrimitiveKind(_Enum):
    """A Q# primitive kind."""

    Bool = 0
    Int = 1
    Double = 2
    Complex = 3
    String = 4
    Pauli = 5
    Result = 6


class _QirInstructionId(_Enum):
    """QIR instruction identifier."""

    I = 0
    H = 1
    X = 2
    Y = 3
    Z = 4
    S = 5
    T = 6
    CNOT = 7
    M = 8


# Give stub enums proper public names and canonical module so signatures
# render cleanly and _is_own_type() can match them to the right module.
_TargetProfile.__name__ = _TargetProfile.__qualname__ = "TargetProfile"
_TargetProfile.__module__ = "qsharp._native"
_Result.__name__ = _Result.__qualname__ = "Result"
_Result.__module__ = "qsharp._native"
_Pauli.__name__ = _Pauli.__qualname__ = "Pauli"
_Pauli.__module__ = "qsharp._native"
_CircuitGenerationMethod.__name__ = _CircuitGenerationMethod.__qualname__ = (
    "CircuitGenerationMethod"
)
_CircuitGenerationMethod.__module__ = "qsharp._native"
_OutputSemantics.__name__ = _OutputSemantics.__qualname__ = "OutputSemantics"
_OutputSemantics.__module__ = "qsharp._native"
_ProgramType.__name__ = _ProgramType.__qualname__ = "ProgramType"
_ProgramType.__module__ = "qsharp._native"
_TypeKind.__name__ = _TypeKind.__qualname__ = "TypeKind"
_TypeKind.__module__ = "qsharp._native"
_PrimitiveKind.__name__ = _PrimitiveKind.__qualname__ = "PrimitiveKind"
_PrimitiveKind.__module__ = "qsharp._native"
_QirInstructionId.__name__ = _QirInstructionId.__qualname__ = "QirInstructionId"
_QirInstructionId.__module__ = "qsharp._native"

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
_stub_module(
    "qiskit",
    QuantumCircuit=_cls("QuantumCircuit", module="qiskit"),
    QiskitError=_cls("QiskitError", module="qiskit"),
    transpile=_stub_fn,
)
_stub_module(
    "qiskit.circuit",
    **{
        n: _cls(n, module="qiskit.circuit")
        for n in [
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
            "Store",
        ]
    },
)
_stub_module(
    "qiskit.circuit.controlflow",
    **{
        n: _cls(n, module="qiskit.circuit.controlflow")
        for n in [
            "ControlFlowOp",
            "ForLoopOp",
            "IfElseOp",
            "SwitchCaseOp",
            "WhileLoopOp",
        ]
    },
)
_stub_module("qiskit.circuit.library")
_stub_module(
    "qiskit.circuit.library.standard_gates",
    **{
        n: _cls(n, module="qiskit.circuit.library.standard_gates")
        for n in [
            "CHGate",
            "CCXGate",
            "CXGate",
            "CYGate",
            "CZGate",
            "CRXGate",
            "CRYGate",
            "CRZGate",
            "RXGate",
            "RXXGate",
            "RYGate",
            "RYYGate",
            "RZGate",
            "RZZGate",
            "HGate",
            "SGate",
            "SdgGate",
            "SXGate",
            "SwapGate",
            "TGate",
            "TdgGate",
            "XGate",
            "YGate",
            "ZGate",
            "IGate",
        ]
    },
)
_stub_module(
    "qiskit.providers",
    BackendV2=_cls("BackendV2", module="qiskit.providers"),
    Options=_cls("Options", module="qiskit.providers"),
    JobV1=_cls("JobV1", module="qiskit.providers"),
    JobStatus=_cls("JobStatus", module="qiskit.providers"),
    JobError=_cls("JobError", module="qiskit.providers"),
)
_stub_module(
    "qiskit.transpiler",
    PassManager=_cls("PassManager", module="qiskit.transpiler"),
)
_stub_module(
    "qiskit.transpiler.target",
    Target=_cls("Target", module="qiskit.transpiler.target"),
)
_stub_module(
    "qiskit.transpiler.passes",
    RemoveBarriers=_cls("RemoveBarriers", module="qiskit.transpiler.passes"),
    RemoveResetInZeroState=_cls(
        "RemoveResetInZeroState", module="qiskit.transpiler.passes"
    ),
)


def _identity_decorator(fn):
    """A no-op decorator used to stand in for qiskit decorator stubs."""
    return fn


_control_flow_stub = _types.ModuleType("qiskit.transpiler.passes.utils.control_flow")
_control_flow_stub.trivial_recurse = _identity_decorator
sys.modules["qiskit.transpiler.passes.utils.control_flow"] = _control_flow_stub
_stub_module("qiskit.transpiler.passes.utils", control_flow=_control_flow_stub)

_stub_module(
    "qiskit.transpiler.basepasses",
    TransformationPass=_cls(
        "TransformationPass", module="qiskit.transpiler.basepasses"
    ),
    AnalysisPass=_cls("AnalysisPass", module="qiskit.transpiler.basepasses"),
)
_stub_module(
    "qiskit.dagcircuit",
    DAGCircuit=_cls("DAGCircuit", module="qiskit.dagcircuit"),
)
_qiskit_result_cls = _cls("Result", module="qiskit.result")
_qiskit_experiment_result_cls = _cls("ExperimentResult", module="qiskit.result.result")
_stub_module("qiskit.result", Result=_qiskit_result_cls)
_stub_module(
    "qiskit.result.result",
    Result=_qiskit_result_cls,
    ExperimentResult=_qiskit_experiment_result_cls,
)
_stub_module("qiskit.version", get_version_info=_stub_fn)
_stub_module("qiskit.qasm3")
_stub_module(
    "qiskit.qasm3.exporter", Exporter=_cls("Exporter", module="qiskit.qasm3.exporter")
)
for _sub in ["qiskit.primitives", "qiskit.qasm2", "qiskit_aer"]:
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
# Import qsharp and all sub-packages
# ---------------------------------------------------------------------------
import qsharp  # noqa: E402
import qsharp.estimator  # noqa: E402
import qsharp.noisy_simulator  # noqa: E402
import qsharp.openqasm  # noqa: E402
import qsharp.interop  # noqa: E402
import qsharp.interop.qiskit  # noqa: E402
import qsharp.interop.cirq  # noqa: E402
import qsharp.utils  # noqa: E402

# ---------------------------------------------------------------------------
# Introspection helpers
# ---------------------------------------------------------------------------

# Internal module paths that should be simplified in documented signatures.
_MODULE_REWRITES = [
    ("qsharp._qsharp.", "qsharp."),
    ("qsharp._native.", "qsharp."),
    ("qsharp.estimator._estimator.", "qsharp.estimator."),
    ("qsharp.noisy_simulator._noisy_simulator.", "qsharp.noisy_simulator."),
    ("__main__.", ""),
]


import re as _re

_ENUM_REPR_RE = _re.compile(r"<(\w+)\.(\w+):\s*['\"][^'\"]*['\"]>")


def _clean_sig(sig_str: str) -> str:
    """Remove internal module prefixes and tidy enum reprs from a signature string."""
    for old, new in _MODULE_REWRITES:
        sig_str = sig_str.replace(old, new)
    # Replace <EnumClass.Member: 'value'> → EnumClass.Member
    sig_str = _ENUM_REPR_RE.sub(r"\1.\2", sig_str)
    return sig_str


def _get_summary(obj) -> str:
    """Return the first paragraph of an object's docstring as summary text."""
    doc = inspect.getdoc(obj)
    if not doc:
        return ""
    paragraphs = doc.split("\n\n")
    return paragraphs[0].strip().replace("\n", " ")


def _get_signature_str(fn, name=None) -> str:
    """Return a clean signature string, without ``self``/``cls``."""
    display_name = name or getattr(fn, "__name__", str(fn))
    try:
        sig = inspect.signature(fn)
        params = [v for k, v in sig.parameters.items() if k not in ("self", "cls")]
        new_sig = sig.replace(parameters=params)
        return _clean_sig(f"{display_name}{new_sig}")
    except (ValueError, TypeError):
        return f"{display_name}(...)"


def _default_repr(value) -> str:
    """Represent a parameter default value as a readable string."""
    if value is None:
        return "None"
    if isinstance(value, bool):
        return str(value)
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, str):
        return f"'{value}'"
    if isinstance(value, list) and value == []:
        return "[]"
    # Render enum members as EnumClass.MemberName (e.g. TargetProfile.Unrestricted)
    if isinstance(value, _Enum):
        return f"{type(value).__name__}.{value.name}"
    return repr(value)


def _get_parameters(fn):
    """Return (regular_params, keyword_only_params) as lists of dicts."""
    regular = []
    keyword_only = []
    try:
        sig = inspect.signature(fn)
    except (ValueError, TypeError):
        return [], []

    for param_name, param in sig.parameters.items():
        if param_name in ("self", "cls"):
            continue
        if param.kind in (
            inspect.Parameter.VAR_POSITIONAL,
            inspect.Parameter.VAR_KEYWORD,
        ):
            continue
        entry = {"name": param_name}
        if param.default is inspect.Parameter.empty:
            entry["isRequired"] = True
        else:
            entry["defaultValue"] = _default_repr(param.default)
        if param.kind == inspect.Parameter.KEYWORD_ONLY:
            keyword_only.append(entry)
        else:
            regular.append(entry)
    return regular, keyword_only


def _base_class_names(cls) -> list:
    """Return the list of immediate base-class fully-qualified names."""
    result = []
    for base in cls.__bases__:
        if base is object:
            result.append("builtins.object")
        else:
            mod = getattr(base, "__module__", "") or ""
            qname = getattr(base, "__qualname__", base.__name__)
            if mod and mod not in ("builtins", "__builtin__"):
                result.append(_clean_sig(f"{mod}.{qname}"))
            else:
                result.append(f"builtins.{base.__name__}")
    return result or ["builtins.object"]


# Generic Python Enum docstrings are not useful in API docs.
_ENUM_BOILERPLATE = {
    "Create a collection of name/value pairs.",
    "An enumeration.",
}

# ---------------------------------------------------------------------------
# DocFX data builders
# ---------------------------------------------------------------------------


def _build_function_entry(fn, module_uid: str) -> dict:
    """Build the dict for a single function within a PythonPackage."""
    fn_name = fn.__name__
    uid = f"{module_uid}.{fn_name}"
    summary = _get_summary(fn)
    sig_str = _get_signature_str(fn)
    regular_params, kw_params = _get_parameters(fn)
    entry = {"uid": uid, "name": fn_name}
    if summary:
        entry["summary"] = summary
    entry["signature"] = sig_str
    if regular_params:
        entry["parameters"] = regular_params
    if kw_params:
        entry["keywordOnlyParameters"] = kw_params
    return entry


def _build_class_data(cls, module_uid: str) -> dict:
    """Build the dict for a YamlMime:PythonClass file."""
    cls_name = cls.__name__
    uid = f"{module_uid}.{cls_name}"
    is_enum = isinstance(cls, EnumMeta)

    data = {
        "uid": uid,
        "name": cls_name,
        "fullName": uid,
        "module": module_uid,
        "inheritances": _base_class_names(cls),
    }

    summary = _get_summary(cls)
    if summary and summary not in _ENUM_BOILERPLATE:
        data["summary"] = summary

    if is_enum:
        data["constructor"] = {"syntax": f"{cls_name}()"}
    else:
        ctor_sig = _get_signature_str(cls, name=cls_name)
        ctor = {"syntax": ctor_sig}
        regular_params, kw_params = _get_parameters(cls)
        if regular_params:
            ctor["parameters"] = regular_params
        if kw_params:
            ctor["keywordOnlyParameters"] = kw_params
        data["constructor"] = ctor

    methods = []
    attributes = []

    if is_enum:
        for member_name in cls.__members__:
            attributes.append(
                {
                    "uid": f"{uid}.{member_name}",
                    "name": member_name,
                    "signature": f"{member_name} = {cls_name}.{member_name}",
                }
            )
    else:
        for member_name, member_val in sorted(cls.__dict__.items()):
            if member_name.startswith("_"):
                continue
            member_uid = f"{uid}.{member_name}"

            if isinstance(member_val, property):
                attr = {"uid": member_uid, "name": member_name}
                doc = _get_summary(member_val.fget) if member_val.fget else ""
                if doc:
                    attr["summary"] = doc
                if member_val.fget:
                    ret_ann = getattr(member_val.fget, "__annotations__", {}).get(
                        "return"
                    )
                    if ret_ann is not None:
                        type_str = _clean_sig(
                            getattr(ret_ann, "__name__", None) or str(ret_ann)
                        )
                        attr["signature"] = f"{member_name}: {type_str}"
                attributes.append(attr)

            elif callable(member_val) and not isinstance(member_val, (type, EnumMeta)):
                summary_m = _get_summary(member_val)
                sig_str = _get_signature_str(member_val, name=member_name)
                regular_params, kw_params = _get_parameters(member_val)
                meth = {"uid": member_uid, "name": member_name}
                if summary_m:
                    meth["summary"] = summary_m
                meth["signature"] = sig_str
                if regular_params:
                    meth["parameters"] = regular_params
                if kw_params:
                    meth["keywordOnlyParameters"] = kw_params
                methods.append(meth)

            elif not callable(member_val) and not isinstance(
                member_val, (type, EnumMeta)
            ):
                attr = {"uid": member_uid, "name": member_name}
                ann = cls.__dict__.get("__annotations__", {}).get(member_name)
                if ann is not None:
                    type_str = _clean_sig(getattr(ann, "__name__", None) or str(ann))
                    attr["signature"] = (
                        f"{member_name}: {type_str} = {_default_repr(member_val)}"
                    )
                else:
                    attr["signature"] = f"{member_name} = {_default_repr(member_val)}"
                attributes.append(attr)

        # Also collect annotated class attributes not already captured
        for attr_name, ann in cls.__dict__.get("__annotations__", {}).items():
            if attr_name.startswith("_"):
                continue
            if any(a["name"] == attr_name for a in attributes):
                continue
            type_str = _clean_sig(getattr(ann, "__name__", None) or str(ann))
            attributes.append(
                {
                    "uid": f"{uid}.{attr_name}",
                    "name": attr_name,
                    "signature": f"{attr_name}: {type_str}",
                }
            )

    if methods:
        data["methods"] = methods
    if attributes:
        data["attributes"] = attributes

    return data


def _is_own_type(obj, module_uid: str) -> bool:
    """Return True if *obj* is a class/enum whose canonical module is in *module_uid*.

    A type is considered "own" if:
    - Its ``__module__`` equals *module_uid* exactly, or
    - Its ``__module__`` is a public sub-package of *module_uid* (e.g.
      ``qsharp.estimator.params`` for ``module_uid="qsharp.estimator"``), or
    - Its ``__module__`` is a private internal submodule of the root package
      (e.g. ``qsharp._native``, ``qsharp._qsharp``) — these native extension
      modules host the canonical definitions that are re-exported into public
      modules via ``__all__``.
    """
    if not isinstance(obj, (type, EnumMeta)):
        return False
    obj_module = getattr(obj, "__module__", "") or ""
    if obj_module == module_uid or obj_module.startswith(module_uid + "."):
        return True
    # Accept types from private native submodules of the root package.
    # e.g. qsharp._native types are owned by any qsharp.* module that
    # explicitly re-exports them.
    root = module_uid.split(".")[0]  # e.g. "qsharp"
    if obj_module.startswith(root + "._"):
        return True
    return False


def _public_names(module, module_uid: str) -> list:
    """Return the public names to document for a module."""
    candidate_names = getattr(module, "__all__", None)
    use_all = candidate_names is not None
    if candidate_names is None:
        candidate_names = [n for n in dir(module) if not n.startswith("_")]

    result = []
    for name in candidate_names:
        obj = getattr(module, name, None)
        if obj is None:
            continue
        if isinstance(obj, (type, EnumMeta)):
            if _is_own_type(obj, module_uid):
                result.append(name)
        elif callable(obj):
            obj_module = getattr(obj, "__module__", "") or ""
            if obj_module.startswith("qsharp") or obj_module == module_uid:
                result.append(name)
    return result


def _build_package_data(module, module_uid: str, subpackage_uids: list) -> dict:
    """Build the dict for a YamlMime:PythonPackage file."""
    all_names = _public_names(module, module_uid)

    functions = []
    class_uids = []

    for name in all_names:
        obj = getattr(module, name, None)
        if obj is None:
            continue
        if isinstance(obj, (type, EnumMeta)):
            class_uids.append(f"{module_uid}.{name}")
        elif callable(obj):
            functions.append(_build_function_entry(obj, module_uid))

    data = {
        "uid": module_uid,
        "name": module_uid.split(".")[-1],
        "fullName": module_uid,
        "type": "import",
    }

    summary = _get_summary(module)
    if summary:
        data["summary"] = summary

    if functions:
        data["functions"] = functions
    if class_uids:
        data["classes"] = class_uids
    if subpackage_uids:
        data["packages"] = subpackage_uids

    return data


# ---------------------------------------------------------------------------
# YAML writer
# ---------------------------------------------------------------------------


def _write_docfx_yaml(mime_type: str, data: dict, output_path: str) -> None:
    """Write a DocFX YAML file with the ``### YamlMime:`` directive header."""
    try:
        import yaml
    except ImportError:
        raise ImportError(
            "PyYAML is required. Install it with: pip install pyyaml"
        ) from None

    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    content = yaml.dump(
        data,
        allow_unicode=True,
        default_flow_style=False,
        sort_keys=False,
        width=120,
    )
    with open(output_path, "w", encoding="utf-8") as f:
        f.write(f"### YamlMime:{mime_type}\n")
        f.write(content)


# ---------------------------------------------------------------------------
# Main entry-point
# ---------------------------------------------------------------------------


def generate_all(output_dir: str) -> None:
    """Generate all DocFX YAML files into *output_dir*."""
    module_map = {
        "qsharp": qsharp,
        "qsharp.estimator": qsharp.estimator,
        "qsharp.openqasm": qsharp.openqasm,
        "qsharp.noisy_simulator": qsharp.noisy_simulator,
        "qsharp.interop.qiskit": qsharp.interop.qiskit,
        "qsharp.interop.cirq": qsharp.interop.cirq,
        "qsharp.utils": qsharp.utils,
    }
    modules = [
        (
            "qsharp",
            [
                "qsharp.estimator",
                "qsharp.openqasm",
                "qsharp.noisy_simulator",
                "qsharp.interop.qiskit",
                "qsharp.interop.cirq",
                "qsharp.utils",
            ],
        ),
        ("qsharp.estimator", []),
        ("qsharp.openqasm", []),
        ("qsharp.noisy_simulator", []),
        ("qsharp.interop.qiskit", []),
        ("qsharp.interop.cirq", []),
        ("qsharp.utils", []),
    ]

    os.makedirs(output_dir, exist_ok=True)
    toc_items = []

    for module_uid, subpackage_uids in modules:
        module = module_map[module_uid]
        print(f"  {module_uid} ...", flush=True)

        pkg_data = _build_package_data(module, module_uid, subpackage_uids)
        pkg_path = os.path.join(output_dir, f"{module_uid}.yml")
        _write_docfx_yaml("PythonPackage", pkg_data, pkg_path)

        all_names = _public_names(module, module_uid)
        toc_children = []

        for name in all_names:
            obj = getattr(module, name, None)
            if obj is None or not isinstance(obj, (type, EnumMeta)):
                continue
            cls_uid = f"{module_uid}.{name}"
            cls_data = _build_class_data(obj, module_uid)
            cls_path = os.path.join(output_dir, f"{cls_uid}.yml")
            _write_docfx_yaml("PythonClass", cls_data, cls_path)
            toc_children.append({"name": name, "uid": cls_uid})

        toc_entry = {"name": module_uid, "uid": module_uid}
        if toc_children:
            toc_entry["items"] = toc_children
        toc_items.append(toc_entry)

    try:
        import yaml
    except ImportError:
        raise ImportError("PyYAML is required. Install it with: pip install pyyaml")

    toc_path = os.path.join(output_dir, "toc.yml")
    with open(toc_path, "w", encoding="utf-8") as f:
        f.write(
            yaml.dump(
                toc_items, allow_unicode=True, default_flow_style=False, sort_keys=False
            )
        )

    total_files = sum(1 for e in os.scandir(output_dir) if e.name.endswith(".yml"))
    print(f"\nWrote {total_files} YAML files to: {output_dir}")


if __name__ == "__main__":
    output_dir = (
        sys.argv[1]
        if len(sys.argv) > 1
        else os.path.join(_SCRIPT_DIR, "python-api-yaml")
    )
    print(f"Generating DocFX YAML docs -> {output_dir}")
    generate_all(output_dir)
