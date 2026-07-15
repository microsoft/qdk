# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Syntactic parsing of OpenQASM programs.

This module exposes the QDK's OpenQASM *parser* to Python. Unlike semantic
analysis, a syntactic parse performs lexing and parsing only: it does not
resolve identifiers, infer types, or evaluate constants. The result is a tree of
read-only nodes rooted at :class:`Program`, mirroring the raw structure of the
source text.

Use :func:`parse` as the entry point::

    from qdk.openqasm import parser

    result = parser.parse("OPENQASM 3.0; qubit q; x q;")
    if result.has_errors:
        for diagnostic in result.diagnostics:
            print(diagnostic.message)
    program = result.program

Class names follow the ``openqasm3`` reference AST wherever an equivalent class
exists (for example :class:`BinaryExpression`, :class:`QuantumGate`,
:class:`ClassicalDeclaration`, and :class:`ForInLoop`); variants with no
``openqasm3`` equivalent take a descriptive QDK name (for example
:class:`ErrorExpression` and :class:`ParenExpression`).

Every node derives from :class:`QASMNode`. Expressions derive from
:class:`Expression` and statements from :class:`Statement`. Because this is a
purely syntactic tree, expressions carry no ``ty``, ``const_value``, or
``symbol`` accessors; for that resolved information use
:func:`qdk.openqasm.semantic.analyze` instead. There is no ``kind``
discriminant: dispatch on a node's concrete type using :func:`isinstance` or
``type(node).__name__``, and traverse uniformly with each node's ``children()``
method.

Nodes are eagerly materialized and hold no reference back into the parser, so
they may be freely retained, inspected across threads, and traversed after the
call returns.
"""

from __future__ import annotations

from typing import Callable, Dict, Optional, Union

from .._native import (  # type: ignore
    AliasStatement,
    BinaryExpression,
    Box,
    BranchingStatement,
    BreakStatement,
    CalibrationDefinition,
    CalibrationGrammarDeclaration,
    CalibrationStatement,
    Cast,
    ClassicalAssignment,
    ClassicalDeclaration,
    CompoundAssignment,
    CompoundStatement,
    Concatenation,
    ConstantDeclaration,
    ContinueStatement,
    DelayInstruction,
    Diagnostic,
    DurationOf,
    EndStatement,
    ErrorExpression,
    ErrorStatement,
    Expression,
    ExpressionStatement,
    ExternDeclaration,
    ForInLoop,
    FunctionCall,
    HardwareQubit,
    IODeclaration,
    Identifier,
    Include,
    IndexExpression,
    IndexedIdentifier,
    Label,
    LiteralExpression,
    ParenExpression,
    ParseResult,
    Pragma,
    Program,
    QASMNode,
    QubitDeclaration,
    QuantumBarrier,
    QuantumGate,
    QuantumGateDefinition,
    QuantumGateModifier,
    QuantumMeasurement,
    QuantumMeasurementStatement,
    QuantumPhase,
    QuantumReset,
    ReturnStatement,
    Severity,
    Span,
    Statement,
    SubroutineDefinition,
    SwitchStatement,
    UnaryExpression,
    WhileLoop,
    parse as _parse,
)
from ._visitor import QASMVisitor

__all__ = [
    "parse",
    "QASMVisitor",
    "ParseResult",
    "Diagnostic",
    "Label",
    "Severity",
    "Span",
    "QASMNode",
    "Expression",
    "Statement",
    "Program",
    "QuantumGateModifier",
    "Identifier",
    "IndexedIdentifier",
    "HardwareQubit",
    "ErrorExpression",
    "UnaryExpression",
    "BinaryExpression",
    "LiteralExpression",
    "FunctionCall",
    "Cast",
    "IndexExpression",
    "ParenExpression",
    "DurationOf",
    "Concatenation",
    "QuantumMeasurement",
    "QubitDeclaration",
    "AliasStatement",
    "ClassicalAssignment",
    "CompoundAssignment",
    "QuantumBarrier",
    "Box",
    "BreakStatement",
    "CompoundStatement",
    "CalibrationStatement",
    "CalibrationGrammarDeclaration",
    "ClassicalDeclaration",
    "ConstantDeclaration",
    "ContinueStatement",
    "SubroutineDefinition",
    "CalibrationDefinition",
    "DelayInstruction",
    "EndStatement",
    "ExpressionStatement",
    "ExternDeclaration",
    "ForInLoop",
    "BranchingStatement",
    "QuantumGate",
    "QuantumPhase",
    "Include",
    "IODeclaration",
    "QuantumMeasurementStatement",
    "Pragma",
    "QuantumGateDefinition",
    "QuantumReset",
    "ReturnStatement",
    "SwitchStatement",
    "WhileLoop",
    "ErrorStatement",
]


def parse(
    source: str,
    *,
    path: str = "<source>",
    includes: Optional[Union[Dict[str, str], Callable[[str], Optional[str]]]] = None,
) -> ParseResult:
    """Parse OpenQASM source text into a syntax tree.

    Args:
        source: The OpenQASM 2.0 or 3.0 source text to parse.
        path: A display name for the source, used in diagnostics.
        includes: How to resolve ``include`` directives. Either a mapping from
            include name to source text, or a callable that maps an include name
            to source text (returning ``None`` when the name is unknown). When
            ``None``, includes cannot be resolved from the caller.

    Returns:
        A :class:`ParseResult` whose ``program`` is the root :class:`Program`
        and whose ``diagnostics`` list any syntax errors. Diagnostics are
        collected rather than raised.
    """
    return _parse(source, path, includes)
