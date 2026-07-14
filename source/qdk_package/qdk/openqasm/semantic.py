# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Semantic analysis of OpenQASM programs.

This module exposes the QDK's OpenQASM *semantic* analyzer to Python. Unlike a
purely syntactic parse, semantic analysis resolves identifiers to symbols,
infers types, and evaluates compile-time constants. The result is a tree of
richly-typed, read-only nodes rooted at :class:`Program`, together with the
resolved :class:`SymbolTable`.

Use :func:`analyze` as the entry point::

    from qdk.openqasm import semantic

    result = semantic.analyze('OPENQASM 3.0; include "stdgates.inc"; qubit q; x q;')
    if result.has_errors:
        for diagnostic in result.diagnostics:
            print(diagnostic.message)
    program = result.program

Every node derives from :class:`QASMNode`. Expressions derive from
:class:`Expression` (and :class:`SemanticExpression`, which adds ``ty``,
``const_value``, and ``symbol``); statements derive from :class:`Statement`
(and :class:`SemanticStatement`, which adds ``annotations``). There is no
``kind`` discriminant: dispatch on a node's concrete type using
:func:`isinstance` or ``type(node).__name__``, and traverse uniformly with each
node's ``children()`` method.

The node classes present clean, un-prefixed Python names (for example
``QuantumGate`` and ``BinaryExpression``) and identify this importable module
as their runtime home. They keep ``Sem``-prefixed identifiers in the native
layer, where a private namespace avoids collisions with the syntactic layer's
``openqasm3``-parity names.

Nodes are eagerly materialized and hold no reference back into the analyzer, so
they may be freely retained, inspected across threads, and traversed after the
call returns.

``AnalysisResult.document`` and ``Program.document`` are the same immutable
source snapshot. Semantic node, symbol, and diagnostic spans are global,
half-open UTF-8 byte ranges and can be mapped to their owning source through
``result.document.source_map``.

The ``includes`` argument follows the parser's logical resolver contract:
relative ``.`` and ``..`` components are normalized using ``/``-separated
logical paths, URI-like schemes remain opaque, and caller keys match exactly
and case-sensitively. ``stdgates.inc``, ``qelib1.inc``, and the QDK extension
``qdk.inc`` are built in. Without consulting the resolver, ``qdk.inc`` makes
the ``mresetz_checked(qubit) -> int`` and
``postselectz(bit, qubit) -> void`` intrinsics available.
``mresetz_checked`` returns ``0`` for Zero, ``1`` for One, or ``2`` for qubit
loss. There is no filesystem or network fallback. Missing sources and callback
failures become diagnostics and unresolved source entries instead of escaping
as callback exceptions. Each analysis call creates a fresh resolver bridge.

Semantic type and constant values are analysis data. Do not persist their
human-readable string forms as a stable interchange format.

``const_value`` returns a native Python value for every literal kind, never a
rendered string: ``bool`` for booleans and bits, ``int`` for integers and
arbitrary-precision integers, ``float`` for floats, ``complex`` for complex
values, ``str`` for a bitstring's zero-padded binary digits, :class:`Angle` for
angles, :class:`Duration` for durations, and ``None`` for arrays and for
expressions that are not constant.

Resolved types are structured nodes rather than strings. ``Type`` is the base of
a family covering every resolved kind, so an ``int[8]`` arrives as an
:class:`IntType` whose ``size`` is ``8``, and an ``array[int[8], 2, 3]`` arrives
as an :class:`ArrayType` whose ``base_type`` and ``dimensions`` are separately
addressable. Dispatch with :func:`isinstance`; ``Type.name`` remains available as
a textual rendering. Because a resolved type is analysis output rather than
syntax, it carries no ``span`` and is not a ``QASMNode``, so it does not appear
in ``children()``.

Nodes, resolved types, and constant values all compare and hash structurally
rather than by identity. Two analyses of the same source produce equal,
equal-hashing trees, and so do two nodes describing the same construct at
different offsets, because ``span`` does not participate. A symbol's ``id`` does
not participate either, so a reference to a name compares equal regardless of
where that name landed in the analysis symbol table. One consequence is worth
knowing: structurally identical nodes taken from different documents also
compare equal, since neither position nor source document participates.

To reach a node's resolved declaration, use its ``symbol`` accessor, and use
``Symbol.id`` when you need to address the symbol table directly.

Most class names here are also class names in :mod:`qdk.openqasm.parser`, so a
value named ``Program``, ``QuantumGate``, or ``IntType`` does not say which tree
produced it. Use ``isinstance(node, SemanticNode)`` to ask.
:class:`SemanticNode` is a virtual base with no members: every class in this
tree is registered against it at import time, and
:class:`qdk.openqasm.parser.SyntaxNode` is the counterpart for the other tree.
Resolved types count, even though they are not ``QASMNode``\\ s, because
``IntType`` and its siblings are exactly the names that collide. Reach for the
predicate at an API boundary that must reject the wrong tree, or while
diagnosing where a node came from; it resolves through
``ABCMeta.__instancecheck__``, which is not what you want inside a traversal
loop.

Four classes answer ``False`` to both questions: :class:`QASMNode`,
:class:`Expression`, :class:`Statement`, and :class:`Annotation`. Both trees use
them, so asking which tree one came from has no answer, and claiming either
would be false. Everything an analysis actually produces is a
:class:`SemanticNode`.
"""

from __future__ import annotations

from time import monotonic
from typing import Callable, Dict, Optional, Union

from .._native import (  # type: ignore
    AnalysisResult,
    Annotation,
    BinaryOperator,
    Diagnostic,
    Expression,
    GateModifierName,
    Label,
    QASMNode,
    Severity,
    Span,
    Statement,
    TimeUnit,
    UnaryOperator,
    analyze as _analyze,
)
from .._native import _semantic  # type: ignore
from .. import telemetry_events
from ._layers import SemanticNode, register_layer as _register_layer
from ._visitor import QASMVisitor

# The semantic node classes present clean, un-prefixed Python names from the
# `qdk._native._semantic` native submodule. Each class keeps its `Sem`-prefixed
# Rust identifier (for example `SemGateCall`) but is exposed here without the
# prefix (`QuantumGate`). Isolating the family in a submodule avoids colliding
# with the syntactic layer's `openqasm3`-parity names in the flat `qdk._native`.

# Category bases and projections.
SemanticExpression = _semantic.SemanticExpression
SemanticStatement = _semantic.SemanticStatement
Program = _semantic.Program
Type = _semantic.Type
Symbol = _semantic.Symbol
SymbolTable = _semantic.SymbolTable
CastKind = _semantic.CastKind
IOKind = _semantic.IOKind
HardwareQubit = _semantic.HardwareQubit
QuantumGateModifier = _semantic.QuantumGateModifier
RangeDefinition = _semantic.RangeDefinition
DiscreteSet = _semantic.DiscreteSet
SwitchCase = _semantic.SwitchCase
SubroutineParameter = _semantic.SubroutineParameter
GateParameter = _semantic.GateParameter

# Constant values carried by `const_value`.
Angle = _semantic.Angle
Duration = _semantic.Duration

# Resolved type nodes. `Type` is the base; dispatch over the concrete kinds with
# `isinstance`. These are analysis results, not syntax, so they carry no span and
# are not `QASMNode` instances.
IntType = _semantic.IntType
UintType = _semantic.UintType
FloatType = _semantic.FloatType
AngleType = _semantic.AngleType
ComplexType = _semantic.ComplexType
BitType = _semantic.BitType
BoolType = _semantic.BoolType
DurationType = _semantic.DurationType
StretchType = _semantic.StretchType
QubitType = _semantic.QubitType
HardwareQubitType = _semantic.HardwareQubitType
BitArrayType = _semantic.BitArrayType
QubitArrayType = _semantic.QubitArrayType
ArrayType = _semantic.ArrayType
StaticArrayReferenceType = _semantic.StaticArrayReferenceType
DynArrayReferenceType = _semantic.DynArrayReferenceType
GateType = _semantic.GateType
FunctionType = _semantic.FunctionType
RangeType = _semantic.RangeType
SetType = _semantic.SetType
VoidType = _semantic.VoidType
ErrorType = _semantic.ErrorType

# Expression leaf nodes.
ErrorExpression = _semantic.ErrorExpression
Identifier = _semantic.Identifier
CapturedIdentifier = _semantic.CapturedIdentifier
UnaryExpression = _semantic.UnaryExpression
BinaryExpression = _semantic.BinaryExpression
LiteralExpression = _semantic.LiteralExpression
FunctionCall = _semantic.FunctionCall
BuiltinFunctionCall = _semantic.BuiltinFunctionCall
Cast = _semantic.Cast
IndexExpression = _semantic.IndexExpression
ParenExpression = _semantic.ParenExpression
QuantumMeasurement = _semantic.QuantumMeasurement
RuntimeSizeof = _semantic.RuntimeSizeof
DurationOf = _semantic.DurationOf
Concatenation = _semantic.Concatenation

# Statement leaf nodes.
AliasStatement = _semantic.AliasStatement
ClassicalAssignment = _semantic.ClassicalAssignment
QuantumBarrier = _semantic.QuantumBarrier
Box = _semantic.Box
CompoundStatement = _semantic.CompoundStatement
BreakStatement = _semantic.BreakStatement
CalibrationStatement = _semantic.CalibrationStatement
CalibrationGrammarDeclaration = _semantic.CalibrationGrammarDeclaration
ClassicalDeclaration = _semantic.ClassicalDeclaration
ContinueStatement = _semantic.ContinueStatement
SubroutineDefinition = _semantic.SubroutineDefinition
CalibrationDefinition = _semantic.CalibrationDefinition
DelayInstruction = _semantic.DelayInstruction
EndStatement = _semantic.EndStatement
ExpressionStatement = _semantic.ExpressionStatement
ExternDeclaration = _semantic.ExternDeclaration
ForInLoop = _semantic.ForInLoop
QuantumGate = _semantic.QuantumGate
BranchingStatement = _semantic.BranchingStatement
IndexedClassicalAssignment = _semantic.IndexedClassicalAssignment
InputDeclaration = _semantic.InputDeclaration
OutputDeclaration = _semantic.OutputDeclaration
Pragma = _semantic.Pragma
QuantumGateDefinition = _semantic.QuantumGateDefinition
QubitDeclaration = _semantic.QubitDeclaration
QubitArrayDeclaration = _semantic.QubitArrayDeclaration
QuantumReset = _semantic.QuantumReset
ReturnStatement = _semantic.ReturnStatement
SwitchStatement = _semantic.SwitchStatement
WhileLoop = _semantic.WhileLoop
ErrorStatement = _semantic.ErrorStatement

__all__ = [
    "analyze",
    "QASMVisitor",
    "SemanticNode",
    "AnalysisResult",
    "Annotation",
    "Diagnostic",
    "Label",
    "Severity",
    "Span",
    "QASMNode",
    "Expression",
    "Statement",
    "SemanticExpression",
    "SemanticStatement",
    "Program",
    "Type",
    "Symbol",
    "SymbolTable",
    "CastKind",
    "IOKind",
    "BinaryOperator",
    "UnaryOperator",
    "GateModifierName",
    "TimeUnit",
    "Angle",
    "Duration",
    "IntType",
    "UintType",
    "FloatType",
    "AngleType",
    "ComplexType",
    "BitType",
    "BoolType",
    "DurationType",
    "StretchType",
    "QubitType",
    "HardwareQubitType",
    "BitArrayType",
    "QubitArrayType",
    "ArrayType",
    "StaticArrayReferenceType",
    "DynArrayReferenceType",
    "GateType",
    "FunctionType",
    "RangeType",
    "SetType",
    "VoidType",
    "ErrorType",
    "HardwareQubit",
    "QuantumGateModifier",
    "RangeDefinition",
    "DiscreteSet",
    "SwitchCase",
    "SubroutineParameter",
    "GateParameter",
    "ErrorExpression",
    "Identifier",
    "CapturedIdentifier",
    "UnaryExpression",
    "BinaryExpression",
    "LiteralExpression",
    "FunctionCall",
    "BuiltinFunctionCall",
    "Cast",
    "IndexExpression",
    "ParenExpression",
    "QuantumMeasurement",
    "RuntimeSizeof",
    "DurationOf",
    "Concatenation",
    "AliasStatement",
    "ClassicalAssignment",
    "QuantumBarrier",
    "Box",
    "CompoundStatement",
    "BreakStatement",
    "CalibrationStatement",
    "CalibrationGrammarDeclaration",
    "ClassicalDeclaration",
    "ContinueStatement",
    "SubroutineDefinition",
    "CalibrationDefinition",
    "DelayInstruction",
    "EndStatement",
    "ExpressionStatement",
    "ExternDeclaration",
    "ForInLoop",
    "QuantumGate",
    "BranchingStatement",
    "IndexedClassicalAssignment",
    "InputDeclaration",
    "OutputDeclaration",
    "Pragma",
    "QuantumGateDefinition",
    "QubitDeclaration",
    "QubitArrayDeclaration",
    "QuantumReset",
    "ReturnStatement",
    "SwitchStatement",
    "WhileLoop",
    "ErrorStatement",
]

# Runs once at import. Every exported class rooted at `QASMNode` or at `Type`,
# apart from the four this layer shares with the syntactic layer, becomes a
# `SemanticNode`. `Type` is a second root because resolved types are not
# `QASMNode`s yet are unmistakably part of this layer.
_register_layer(SemanticNode, globals(), __all__, (QASMNode, Type))


def analyze(
    source: str,
    *,
    path: str = "<source>",
    includes: Optional[Union[Dict[str, str], Callable[[str], Optional[str]]]] = None,
) -> AnalysisResult:
    """Parse and semantically analyze OpenQASM source text.

    Args:
        source: The OpenQASM 2.0 or 3.0 source text to analyze.
        path: A display name for the source, used in diagnostics.
        includes: How to resolve ``include`` directives. Either a mapping from
            normalized logical identifier to source text, or a callable that
            maps that identifier to source text (returning ``None`` when the
            identifier is unknown). Matching is exact and case-sensitive.
            Built-in standard includes remain available when this is ``None``;
            other includes produce diagnostics because there is no filesystem
            fallback. Callback exceptions are converted to diagnostics.

    Returns:
        An :class:`AnalysisResult` whose ``program`` is the root
        :class:`Program`, whose ``symbols`` is the resolved
        :class:`SymbolTable`, whose ``document`` owns every source in the
        analysis snapshot, and whose ``diagnostics`` list any errors.
        Diagnostics are collected rather than raised. All spans are global,
        half-open UTF-8 byte ranges resolved through ``document.source_map``.
    """
    telemetry_events.on_analyze_qasm(len(source), resolver=includes is not None)
    start = monotonic()
    result = _analyze(source, path, includes)
    durationMs = (monotonic() - start) * 1000
    # Spans are global and half-open: per-file widths sum to the exact total, the last span's end does not.
    total_source_bytes = sum(
        f.span.hi - f.span.lo for f in result.document.source_map.files
    )
    telemetry_events.on_analyze_qasm_end(
        durationMs,
        result.has_errors,
        resolver=includes is not None,
        total_source_bytes=total_source_bytes,
    )
    return result
