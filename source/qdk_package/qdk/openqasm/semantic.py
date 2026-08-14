# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Resolve OpenQASM symbols, types, and compile-time values.

Use :func:`analyze` to obtain a read-only semantic tree and symbol table::

    from qdk.openqasm import semantic

    result = semantic.analyze('OPENQASM 3.0; include "stdgates.inc"; qubit q; x q;')
    if result.has_errors:
        for diagnostic in result.diagnostics:
            print(diagnostic.message)
    program = result.program

Use semantic analysis when you need to follow an identifier to its declaration,
inspect an expression's resolved type, or read a value computed at compile time.
Use :func:`qdk.openqasm.parser.parse` instead when you need the source-level
syntax, including unresolved identifiers and type expressions as written.

Every semantic expression exposes ``ty``, ``const_value``, and ``symbol``.
Dispatch on concrete classes such as :class:`BinaryExpression` with
:func:`isinstance`, traverse nodes with :class:`QASMVisitor` or ``children()``,
and use :class:`SemanticNode` when an API must distinguish semantic values from
parser values.

``AnalysisResult.document`` and ``Program.document`` are the same immutable
source snapshot. Semantic node, symbol, and diagnostic spans are global,
half-open UTF-8 byte ranges and can be mapped to their owning source through
``result.document.source_map``.

The ``includes`` argument accepts the same logical-path mapping or callback as
:func:`qdk.openqasm.parser.parse`. ``stdgates.inc``, ``qelib1.inc``, and
``qdk.inc`` are built in; there is no implicit filesystem or network lookup.
Missing includes and callback failures are reported as diagnostics.

Semantic type and constant values are analysis data. Do not persist their
human-readable string forms as a stable interchange format.

``const_value`` returns a native Python value for every literal kind, never a
rendered string: ``bool`` for booleans and bits, ``int`` for integers and
arbitrary-precision integers, ``float`` for floats, ``complex`` for complex
values, ``str`` for a bitstring's zero-padded binary digits, :class:`Angle` for
angles, :class:`Duration` for durations, and ``None`` for arrays and for
expressions that are not constant.

Resolved types are structured values rather than strings. :class:`Type` is the
base of the resolved type family, so an ``int[8]`` arrives as an
:class:`IntType` whose ``size`` is ``8``, and an ``array[int[8], 2, 3]`` arrives
as an :class:`ArrayType` whose ``base_type`` and ``dimensions`` are separately
addressable. Dispatch with :func:`isinstance`; ``Type.name`` remains available as
a display string. Resolved types have no source span and do not appear in an
AST node's ``children()``.

Nodes, resolved types, and constant values all compare and hash structurally
rather than by identity. Two analyses of the same source produce equal,
equal-hashing trees, and so do two nodes describing the same construct at
different offsets, because ``span`` does not participate. A symbol's ``id`` does
not participate either, so a reference to a name compares equal regardless of
where that name landed in the analysis symbol table. One consequence is worth
knowing: structurally identical nodes taken from different documents also
compare equal, since neither position nor source document participates.

To reach a referenced declaration, use an expression's ``symbol`` property.
Use :meth:`SymbolTable.lookup` to find a declaration by name or
:meth:`SymbolTable.get` to find one by ``Symbol.id``.
"""

from __future__ import annotations

from time import monotonic

from ._native_syntax import (
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
)
from ._native_semantic import (
    AnalysisResult,
    analyze as _analyze,
    # Category bases and projections.
    SemanticExpression,
    SemanticStatement,
    Program,
    Type,
    Symbol,
    SymbolTable,
    CastKind,
    IOKind,
    HardwareQubit,
    QuantumGateModifier,
    RangeDefinition,
    DiscreteSet,
    SwitchCase,
    SubroutineParameter,
    GateParameter,
    # Constant values carried by `const_value`.
    Angle,
    Duration,
    # are not `QASMNode` instances.
    IntType,
    UintType,
    FloatType,
    AngleType,
    ComplexType,
    BitType,
    BoolType,
    DurationType,
    StretchType,
    QubitType,
    HardwareQubitType,
    BitArrayType,
    QubitArrayType,
    ArrayType,
    StaticArrayReferenceType,
    DynArrayReferenceType,
    GateType,
    FunctionType,
    RangeType,
    SetType,
    VoidType,
    ErrorType,
    # Expression leaf nodes.
    ErrorExpression,
    Identifier,
    CapturedIdentifier,
    UnaryExpression,
    BinaryExpression,
    LiteralExpression,
    FunctionCall,
    BuiltinFunctionCall,
    Cast,
    IndexExpression,
    ParenExpression,
    QuantumMeasurement,
    RuntimeSizeof,
    DurationOf,
    Concatenation,
    # Statement leaf nodes.
    AliasStatement,
    ClassicalAssignment,
    QuantumBarrier,
    Box,
    CompoundStatement,
    BreakStatement,
    CalibrationStatement,
    CalibrationGrammarDeclaration,
    ClassicalDeclaration,
    ContinueStatement,
    SubroutineDefinition,
    CalibrationDefinition,
    DelayInstruction,
    EndStatement,
    ExpressionStatement,
    ExternDeclaration,
    ForInLoop,
    QuantumGate,
    BranchingStatement,
    IndexedClassicalAssignment,
    InputDeclaration,
    OutputDeclaration,
    Pragma,
    QuantumGateDefinition,
    QubitDeclaration,
    QubitArrayDeclaration,
    QuantumReset,
    ReturnStatement,
    SwitchStatement,
    WhileLoop,
    ErrorStatement,
)
from .. import telemetry_events
from ._layers import SemanticNode, register_layer as _register_layer
from ._visitor import QASMVisitor
from .parser import IncludeResolver

__all__ = [
    "analyze",
    "IncludeResolver",
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
    includes: IncludeResolver = None,
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
