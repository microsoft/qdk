# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Experimental circuit construction API.

Importing this module emits :class:`ExperimentalWarning`. The API may change
in any release while it carries that status, and normal versioning guarantees
begin only once the marker is removed.

The warning fires during import, so silencing it has to happen before that::

    import warnings

    warnings.filterwarnings("ignore", message="qdk.circuit is experimental")
    from qdk.circuit import QuantumCircuit

Filtering on :class:`ExperimentalWarning` by category works for warnings
raised after import, but not for the import warning itself, because the class
only becomes importable once the module has already run.
"""

import warnings as _warnings


class ExperimentalWarning(UserWarning):
    """Marks an API whose contract may change before it stabilizes.

    A dedicated category rather than :class:`UserWarning` so that callers can
    silence this API's instability without silencing unrelated warnings.
    """


_warnings.warn(
    "qdk.circuit is experimental. Its API may change in any release until the "
    "experimental marker is removed.",
    ExperimentalWarning,
    stacklevel=2,
)

from .._native import (  # noqa: E402
    AngleExpression,
    AngleOp,
    BlockBuilder,
    CircuitOrigin,
    ClassicalRegister,
    ConditionView,
    EdgeKind,
    EdgeView,
    EffectDomain,
    EffectEvent,
    EffectMode,
    ForLoopBuilder,
    GraphBuilder,
    InstructionView,
    InterfaceBinding,
    InterfaceRole,
    LoopVariable,
    NodeAngle,
    NodeView,
    OriginKind,
    Parameter,
    PassAnchor,
    PassListing,
    PassMatch,
    PassPatch,
    PassPipeline,
    PassResult,
    PassStage,
    PipelineResult,
    PortDirection,
    PortKind,
    PortStage,
    PortUsage,
    PortView,
    QuantumCircuit,
    QuantumGraph,
    QuantumRegister,
    RaisedCircuit,
    RegionKind,
    RegionView,
    SwitchBuilder,
    SwitchCaseBuilder,
    SymbolId,
    ValueType,
)

__all__ = [
    "AngleExpression",
    "AngleOp",
    "BlockBuilder",
    "CircuitOrigin",
    "ClassicalRegister",
    "ConditionView",
    "EdgeKind",
    "EdgeView",
    "EffectDomain",
    "EffectEvent",
    "EffectMode",
    "ExperimentalWarning",
    "ForLoopBuilder",
    "GraphBuilder",
    "InstructionView",
    "InterfaceBinding",
    "InterfaceRole",
    "LoopVariable",
    "NodeAngle",
    "NodeView",
    "OriginKind",
    "Parameter",
    "PassAnchor",
    "PassListing",
    "PassMatch",
    "PassPatch",
    "PassPipeline",
    "PassResult",
    "PassStage",
    "PipelineResult",
    "PortDirection",
    "PortKind",
    "PortStage",
    "PortUsage",
    "PortView",
    "QuantumCircuit",
    "QuantumGraph",
    "QuantumRegister",
    "RaisedCircuit",
    "RegionKind",
    "RegionView",
    "SwitchBuilder",
    "SwitchCaseBuilder",
    "SymbolId",
    "ValueType",
]
