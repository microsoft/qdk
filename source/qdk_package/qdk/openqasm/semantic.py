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
and case-sensitively. ``stdgates.inc`` and ``qelib1.inc`` are built in. There is
no filesystem or network fallback. Missing sources and callback failures become
diagnostics and unresolved source entries instead of escaping as callback
exceptions. Each analysis call creates a fresh resolver bridge.

Semantic type and constant values are analysis data. Do not persist their
human-readable string forms as a stable interchange format.
"""

from __future__ import annotations

from typing import Callable, Dict, Optional, Union

from .._native import (  # type: ignore
    AnalysisResult,
    Annotation,
    Diagnostic,
    Expression,
    Label,
    QASMNode,
    Severity,
    Span,
    Statement,
    analyze as _analyze,
)
from .._native import _semantic  # type: ignore
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
HardwareQubit = _semantic.HardwareQubit
QuantumGateModifier = _semantic.QuantumGateModifier
RangeDefinition = _semantic.RangeDefinition
DiscreteSet = _semantic.DiscreteSet
SwitchCase = _semantic.SwitchCase
SubroutineParameter = _semantic.SubroutineParameter

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
Include = _semantic.Include
IndexedClassicalAssignment = _semantic.IndexedClassicalAssignment
InputDeclaration = _semantic.InputDeclaration
OutputDeclaration = _semantic.OutputDeclaration
QuantumMeasurementStatement = _semantic.QuantumMeasurementStatement
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
    "HardwareQubit",
    "QuantumGateModifier",
    "RangeDefinition",
    "DiscreteSet",
    "SwitchCase",
    "SubroutineParameter",
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
    "Include",
    "IndexedClassicalAssignment",
    "InputDeclaration",
    "OutputDeclaration",
    "QuantumMeasurementStatement",
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
    return _analyze(source, path, includes)
