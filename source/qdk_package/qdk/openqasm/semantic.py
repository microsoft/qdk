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
:class:`Expression` (and :class:`Expr`, which adds ``ty``, ``const_value``,
and ``symbol``); statements derive from :class:`Statement` (and
:class:`Stmt`, which adds ``annotations``). There is no ``kind``
discriminant: dispatch on a node's concrete type using :func:`isinstance` or
``type(node).__name__``, and traverse uniformly with each node's ``children()``
method.

The node classes present clean, un-prefixed Python names (for example
``GateCall`` and ``BinaryOpExpr``). They keep ``Sem``-prefixed identifiers in
the native layer and live in the ``qdk._native._semantic`` submodule to avoid
colliding with the syntactic layer's ``openqasm3``-parity names.

Nodes are eagerly materialized and hold no reference back into the analyzer, so
they may be freely retained, inspected across threads, and traversed after the
call returns.
"""

from __future__ import annotations

from typing import Callable, Dict, Optional, Union

from .._native import (  # type: ignore
    AnalysisResult,
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
# prefix (`GateCall`). Isolating the family in a submodule avoids colliding with
# the syntactic layer's `openqasm3`-parity names in the flat `qdk._native`.

# Category bases and projections.
Expr = _semantic.Expr
Stmt = _semantic.Stmt
Program = _semantic.Program
Type = _semantic.Type
Symbol = _semantic.Symbol
SymbolTable = _semantic.SymbolTable
HardwareQubit = _semantic.HardwareQubit

# Expression leaf nodes.
ErrExpr = _semantic.ErrExpr
ResolvedIdent = _semantic.ResolvedIdent
CapturedResolvedIdent = _semantic.CapturedResolvedIdent
UnaryOpExpr = _semantic.UnaryOpExpr
BinaryOpExpr = _semantic.BinaryOpExpr
Literal = _semantic.Literal
FunctionCall = _semantic.FunctionCall
BuiltinFunctionCall = _semantic.BuiltinFunctionCall
Cast = _semantic.Cast
IndexedExpr = _semantic.IndexedExpr
Paren = _semantic.Paren
Measure = _semantic.Measure
RuntimeSizeof = _semantic.RuntimeSizeof
EvaluatedDurationof = _semantic.EvaluatedDurationof
Concat = _semantic.Concat

# Statement leaf nodes.
AliasDecl = _semantic.AliasDecl
Assign = _semantic.Assign
Barrier = _semantic.Barrier
Box = _semantic.Box
Block = _semantic.Block
Break = _semantic.Break
Calibration = _semantic.Calibration
CalibrationGrammar = _semantic.CalibrationGrammar
ClassicalDecl = _semantic.ClassicalDecl
Continue = _semantic.Continue
Def = _semantic.Def
DefCal = _semantic.DefCal
Delay = _semantic.Delay
End = _semantic.End
ExprStmt = _semantic.ExprStmt
ExternDecl = _semantic.ExternDecl
ForLoop = _semantic.ForLoop
GateCall = _semantic.GateCall
IfStmt = _semantic.IfStmt
Include = _semantic.Include
IndexedAssign = _semantic.IndexedAssign
InputDeclaration = _semantic.InputDeclaration
OutputDeclaration = _semantic.OutputDeclaration
MeasureArrow = _semantic.MeasureArrow
Pragma = _semantic.Pragma
GateDefinition = _semantic.GateDefinition
QubitDecl = _semantic.QubitDecl
QubitArrayDecl = _semantic.QubitArrayDecl
Reset = _semantic.Reset
Return = _semantic.Return
Switch = _semantic.Switch
WhileLoop = _semantic.WhileLoop
ErrStmt = _semantic.ErrStmt

__all__ = [
    "analyze",
    "QASMVisitor",
    "AnalysisResult",
    "Diagnostic",
    "Label",
    "Severity",
    "Span",
    "QASMNode",
    "Expression",
    "Statement",
    "Expr",
    "Stmt",
    "Program",
    "Type",
    "Symbol",
    "SymbolTable",
    "HardwareQubit",
    "ErrExpr",
    "ResolvedIdent",
    "CapturedResolvedIdent",
    "UnaryOpExpr",
    "BinaryOpExpr",
    "Literal",
    "FunctionCall",
    "BuiltinFunctionCall",
    "Cast",
    "IndexedExpr",
    "Paren",
    "Measure",
    "RuntimeSizeof",
    "EvaluatedDurationof",
    "Concat",
    "AliasDecl",
    "Assign",
    "Barrier",
    "Box",
    "Block",
    "Break",
    "Calibration",
    "CalibrationGrammar",
    "ClassicalDecl",
    "Continue",
    "Def",
    "DefCal",
    "Delay",
    "End",
    "ExprStmt",
    "ExternDecl",
    "ForLoop",
    "GateCall",
    "IfStmt",
    "Include",
    "IndexedAssign",
    "InputDeclaration",
    "OutputDeclaration",
    "MeasureArrow",
    "Pragma",
    "GateDefinition",
    "QubitDecl",
    "QubitArrayDecl",
    "Reset",
    "Return",
    "Switch",
    "WhileLoop",
    "ErrStmt",
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
            include name to source text, or a callable that maps an include name
            to source text (returning ``None`` when the name is unknown). When
            ``None``, includes cannot be resolved from the caller.

    Returns:
        An :class:`AnalysisResult` whose ``program`` is the root
        :class:`Program`, whose ``symbols`` is the resolved
        :class:`SymbolTable`, and whose ``diagnostics`` list any errors.
        Diagnostics are collected rather than raised.
    """
    return _analyze(source, path, includes)
