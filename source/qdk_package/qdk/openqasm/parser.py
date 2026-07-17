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

from typing import Callable, Dict, Optional, TextIO, Union

from .._native import (  # type: ignore
    AliasStatement,
    Annotation,
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
    DiscreteSet,
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
    IndexList,
    IndexedIdentifier,
    Label,
    LiteralExpression,
    ParenExpression,
    ParseResult,
    Position,
    PositionEncoding,
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
    RangeDefinition,
    ReturnStatement,
    Severity,
    SourceDocument,
    SourceEdit,
    SourceFile,
    SourceMap,
    SourceRange,
    Span,
    Statement,
    SubroutineParameter,
    SubroutineDefinition,
    SwitchCase,
    SwitchStatement,
    UnaryExpression,
    WhileLoop,
    _QASMUnparseError as _NativeQASMUnparseError,
    parse as _parse,
    qasm_dumps as _qasm_dumps,
)
from ._visitor import QASMVisitor

CANONICAL_FORMAT_VERSION = 1

__all__ = [
    "parse",
    "dumps",
    "unparse",
    "dump",
    "CANONICAL_FORMAT_VERSION",
    "QASMUnparseError",
    "QASMVisitor",
    "Annotation",
    "ParseResult",
    "Diagnostic",
    "Label",
    "Severity",
    "Position",
    "PositionEncoding",
    "SourceDocument",
    "SourceEdit",
    "SourceFile",
    "SourceMap",
    "SourceRange",
    "Span",
    "QASMNode",
    "Expression",
    "Statement",
    "SubroutineParameter",
    "SwitchCase",
    "Program",
    "QuantumGateModifier",
    "RangeDefinition",
    "DiscreteSet",
    "IndexList",
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


class QASMUnparseError(ValueError):
    """Raised when a syntax program cannot be canonically serialized.

    Attributes:
        code: Stable machine-readable error code.
        span: Source span associated with the error, when available.
        diagnostics: Entry-source parser diagnostics that prevented output.
    """

    __slots__ = ("_code", "_span", "_diagnostics")

    def __init__(
        self,
        message: str,
        *,
        code: str,
        span: Optional[Span],
        diagnostics: tuple[Diagnostic, ...],
    ) -> None:
        super().__init__(message)
        self._code = code
        self._span = span
        self._diagnostics = diagnostics

    @property
    def code(self) -> str:
        """Stable machine-readable error code."""
        return self._code

    @property
    def span(self) -> Optional[Span]:
        """Source span associated with the error, when available."""
        return self._span

    @property
    def diagnostics(self) -> tuple[Diagnostic, ...]:
        """Entry-source diagnostics that prevented canonical output."""
        return self._diagnostics


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


def dumps(program: Program, /) -> str:
    """Serialize a syntactic program to canonical OpenQASM source.

    Canonical format version 1 uses LF line endings, two-space indentation,
    one statement per line, normalized whitespace and parentheses, and exactly
    one trailing newline. It preserves the entry source's version, include
    directives, annotations, pragmas, and calibration bodies while omitting
    comments and original formatting. During the preview period, byte-level
    stability is not promised across QDK releases.

    Args:
        program: A syntactic :class:`Program` returned by this parser.

    Returns:
        Canonical OpenQASM source.

    Raises:
        TypeError: If ``program`` is a semantic or foreign program object.
        QASMUnparseError: If the entry source contains recovered or unsupported
            syntax, an invalid string, or a non-finite floating-point value.
    """
    try:
        return _qasm_dumps(program)
    except _NativeQASMUnparseError as error:
        raise QASMUnparseError(
            str(error),
            code=error.code,
            span=error.span,
            diagnostics=error.diagnostics,
        ) from None


unparse = dumps


def dump(program: Program, stream: TextIO, /) -> None:
    """Write canonical OpenQASM source to a text stream exactly once.

    The stream is not flushed or closed. Exceptions from ``stream.write``
    propagate unchanged.

    Args:
        program: A syntactic :class:`Program` returned by this parser.
        stream: A text stream with a ``write(str)`` method.

    Raises:
        TypeError: If ``program`` is a semantic or foreign program object.
        QASMUnparseError: If canonical serialization fails.
    """
    stream.write(dumps(program))
