# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Parse, inspect, and serialize OpenQASM syntax trees.

Use :func:`parse` when you want diagnostics and a recoverable syntax tree, even
for invalid source::

    from qdk.openqasm import parser

    result = parser.parse("OPENQASM 3.0; qubit q; x q;")
    if result.has_errors:
        for diagnostic in result.diagnostics:
            print(diagnostic.message)
    program = result.program

Use :func:`parse_program` instead when invalid source should raise
:class:`QASM3ParsingError`. Parsing preserves the source-level structure but
does not resolve names, infer types, or evaluate constants. For that information,
use :func:`qdk.openqasm.semantic.analyze`.

Inspect a node through its named properties, dispatch with :func:`isinstance`,
or walk its children with :class:`QASMVisitor`. All nodes derive from
:class:`QASMNode`; expression and statement nodes additionally derive from
:class:`Expression` and :class:`Statement`. Use :class:`SyntaxNode` when an API
needs to distinguish parser nodes from semantic nodes.

``result.document`` contains the entry source and every resolved include.
Node and diagnostic spans are global, half-open UTF-8 byte ranges. Convert one
to a source-local :class:`SourceRange` with
``result.document.source_map.range_from_span(span)``.

The ``includes`` argument accepts a mapping or callback from logical include
paths to source text. Paths use ``/`` separators, relative components are
normalized against the including source, and matching is exact and
case-sensitive. ``stdgates.inc``, ``qelib1.inc``, and ``qdk.inc`` are built in;
there is no implicit filesystem or network lookup. Missing includes and callback
failures are reported as diagnostics.

Use :func:`dumps` or :func:`dump` to emit canonical OpenQASM from a complete
syntax :class:`Program`. Serialization normalizes formatting and omits comments;
it does not accept semantic programs or individual nodes.

Nodes compare and hash by structure. Source positions do not participate, so
equal nodes can come from different locations or source documents.

This API is in preview and may change between QDK releases.
"""

from __future__ import annotations

from time import monotonic
from typing import Callable, Optional, TextIO

from ._native_syntax import (
    AccessControl,
    AliasStatement,
    AngleType,
    Annotation,
    ArrayLiteral,
    ArrayType,
    BinaryExpression,
    BinaryOperator,
    BitType,
    BitstringLiteral,
    BoolType,
    BooleanLiteral,
    Box,
    BranchingStatement,
    BreakStatement,
    CalibrationDefinition,
    CalibrationGrammarDeclaration,
    CalibrationStatement,
    Cast,
    ClassicalAssignment,
    ClassicalDeclaration,
    ClassicalType,
    ComplexType,
    CompoundAssignment,
    CompoundStatement,
    Concatenation,
    ConstantDeclaration,
    ContinueStatement,
    DelayInstruction,
    Diagnostic,
    DiscreteSet,
    DurationLiteral,
    DurationOf,
    DurationType,
    DynArrayReferenceType,
    EndStatement,
    ErrorExpression,
    ErrorStatement,
    ErrorType,
    Expression,
    ExpressionStatement,
    ExternDeclaration,
    FloatLiteral,
    FloatType,
    ForInLoop,
    FunctionCall,
    GateModifierName,
    HardwareQubit,
    IODeclaration,
    IOKeyword,
    Identifier,
    ImaginaryLiteral,
    Include,
    IndexExpression,
    IndexList,
    IndexedIdentifier,
    IntType,
    IntegerLiteral,
    Label,
    ParenExpression,
    ParseResult,
    Position,
    PositionEncoding,
    Pragma,
    Program,
    QASMNode,
    QuantumBarrier,
    QuantumGate,
    QuantumGateDefinition,
    QuantumGateModifier,
    QuantumMeasurement,
    QuantumMeasurementStatement,
    QuantumPhase,
    QuantumReset,
    QubitDeclaration,
    QubitType,
    RangeDefinition,
    ResolutionStatus,
    ReturnStatement,
    Severity,
    SourceDocument,
    SourceFile,
    SourceMap,
    SourceRange,
    Span,
    Statement,
    StaticArrayReferenceType,
    StretchType,
    StringLiteral,
    SubroutineDefinition,
    SubroutineParameter,
    SwitchCase,
    SwitchStatement,
    TimeUnit,
    UintType,
    UnaryExpression,
    UnaryOperator,
    WhileLoop,
    _QASMUnparseError as _NativeQASMUnparseError,
    parse as _parse,
    qasm_dumps as _qasm_dumps,
)
from ._native_semantic import Program as _SemanticProgram
from .. import telemetry_events
from ._layers import SyntaxNode, register_layer as _register_layer
from ._visitor import QASMVisitor

IncludeResolver = dict[str, str] | Callable[[str], str | None] | None

__all__ = [
    "parse",
    "parse_program",
    "QASM3ParsingError",
    "IncludeResolver",
    "dumps",
    "dump",
    "QASMUnparseError",
    "QASMVisitor",
    "SyntaxNode",
    "Annotation",
    "ParseResult",
    "Diagnostic",
    "Label",
    "Severity",
    "Position",
    "PositionEncoding",
    "ResolutionStatus",
    "SourceDocument",
    "SourceFile",
    "SourceMap",
    "SourceRange",
    "Span",
    "QASMNode",
    "Expression",
    "Statement",
    "ClassicalType",
    "AccessControl",
    "IntType",
    "UintType",
    "FloatType",
    "AngleType",
    "BitType",
    "ComplexType",
    "BoolType",
    "DurationType",
    "StretchType",
    "QubitType",
    "ErrorType",
    "ArrayType",
    "StaticArrayReferenceType",
    "DynArrayReferenceType",
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
    "IntegerLiteral",
    "FloatLiteral",
    "ImaginaryLiteral",
    "BooleanLiteral",
    "BitstringLiteral",
    "DurationLiteral",
    "ArrayLiteral",
    "StringLiteral",
    "TimeUnit",
    "BinaryOperator",
    "UnaryOperator",
    "IOKeyword",
    "GateModifierName",
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

# Runs once at import. Every exported class rooted at `QASMNode`, apart from the
# four this layer shares with the semantic layer, becomes a `SyntaxNode`.
_register_layer(SyntaxNode, globals(), __all__, (QASMNode,))


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


class QASM3ParsingError(ValueError):
    """Raised when :func:`parse_program` encounters parser diagnostics.

    Attributes:
        result: The complete recovery-oriented parse result.
        diagnostics: An immutable snapshot of every parser diagnostic.
    """

    __slots__ = ("_result", "_diagnostics")

    def __init__(self, result: ParseResult) -> None:
        diagnostics = tuple(result.diagnostics)
        message = "\n\n".join(
            diagnostic.render(color=False).rstrip("\n") for diagnostic in diagnostics
        )
        super().__init__(message or "OpenQASM parsing failed")
        self._result = result
        self._diagnostics = diagnostics

    @property
    def result(self) -> ParseResult:
        """The identical parse result that caused this exception."""
        return self._result

    @property
    def diagnostics(self) -> tuple[Diagnostic, ...]:
        """An immutable snapshot of all parser diagnostics."""
        return self._diagnostics


def parse(
    source: str,
    *,
    path: str = "<source>",
    includes: IncludeResolver = None,
) -> ParseResult:
    """Parse OpenQASM source text into a syntax tree.

    Args:
        source: The OpenQASM 2.0 or 3.0 source text to parse.
        path: A display name for the source, used in diagnostics.
        includes: How to resolve ``include`` directives. Either a mapping from
            normalized logical identifier to source text, or a callable that
            maps that identifier to source text (returning ``None`` when the
            identifier is unknown). Matching is exact and case-sensitive.
            Built-in standard includes remain available when this is ``None``;
            other includes produce diagnostics because there is no filesystem
            fallback. Callback exceptions are converted to diagnostics.

    Returns:
        A :class:`ParseResult` whose ``program`` is the root :class:`Program`
        and whose ``diagnostics`` list any parse errors. Diagnostics are
        collected rather than raised.
    """
    telemetry_events.on_parse_qasm(
        len(source), resolver=includes is not None, compat=False, permissive=False
    )
    start = monotonic()
    result = _parse(source, path, includes)
    durationMs = (monotonic() - start) * 1000
    # Spans are global and half-open: per-file widths sum to the exact total, the last span's end does not.
    total_source_bytes = sum(
        f.span.hi - f.span.lo for f in result.document.source_map.files
    )
    telemetry_events.on_parse_qasm_end(
        durationMs,
        result.has_errors,
        compat=False,
        resolver=includes is not None,
        total_source_bytes=total_source_bytes,
    )
    return result


def parse_program(
    source: str,
    *,
    permissive: bool = False,
    path: str = "<source>",
    includes: IncludeResolver = None,
) -> Program:
    """Parse source with strict-by-default compatibility control flow.

    This wrapper follows the reference ``openqasm3`` parser's error-handling
    convention: errors raise unless ``permissive`` is true. It returns QDK
    syntax objects; AST details, diagnostic text, and recovery behavior may
    differ from the reference parser.

    Args:
        source: The OpenQASM 2.0 or 3.0 source text to parse.
        permissive: Return the recovered program even when diagnostics exist.
        path: A display name for the source, used in diagnostics.
        includes: How to resolve ``include`` directives.

    Returns:
        The parsed syntax program.

    Raises:
        QASM3ParsingError: If parsing reports errors and ``permissive`` is
            false.
    """
    telemetry_events.on_parse_qasm(
        len(source), resolver=includes is not None, compat=True, permissive=permissive
    )
    start = monotonic()
    # Calls the native parser rather than parse() so one call reports one parse event.
    result = _parse(source, path, includes)
    durationMs = (monotonic() - start) * 1000
    total_source_bytes = sum(
        f.span.hi - f.span.lo for f in result.document.source_map.files
    )
    telemetry_events.on_parse_qasm_end(
        durationMs,
        result.has_errors,
        compat=True,
        resolver=includes is not None,
        total_source_bytes=total_source_bytes,
    )
    if not permissive and result.has_errors:
        raise QASM3ParsingError(result)
    return result.program


def _require_syntax_program(program: object) -> None:
    """Rejects a non-syntactic program with a message that names both types.

    PyO3's own rejection reads ``'Program' object is not an instance of
    'Program'`` when a semantic program is passed, because both roots are named
    ``Program``.
    """
    if isinstance(program, Program):
        return
    if program is None:
        actual = "None"
    else:
        kind = type(program)
        actual = f"{kind.__module__}.{kind.__qualname__}"
    expected = f"{Program.__module__}.{Program.__qualname__}"
    message = f"expected a {expected}, got {actual}"
    if isinstance(program, _SemanticProgram):
        message += (
            "; a semantic program carries analysis results rather than syntax, "
            "so parse the source with parse() and serialize that program instead"
        )
    raise TypeError(message)


def dumps(program: Program, /) -> str:
    """Serialize a syntactic program to canonical OpenQASM source.

    This returns OpenQASM source text, matching ``openqasm3.dumps``. It is not
    the debug tree representation that :func:`ast.dump` produces.

    Canonical format version 1 uses LF line endings, two-space indentation,
    one statement per line, normalized whitespace and parentheses, and exactly
    one trailing newline. It preserves the entry source's version, include
    directives, annotations, pragmas, and calibration bodies while omitting
    comments and original formatting. During the preview period, byte-level
    stability is not promised across QDK releases.

    Only a whole syntactic program is accepted. Statements, expressions, and
    semantic programs cannot be serialized with this function.

    Args:
        program: A syntactic :class:`Program` returned by this parser.

    Returns:
        Canonical OpenQASM source.

    Raises:
        TypeError: If ``program`` is a semantic or foreign program object.
        QASMUnparseError: If the entry source contains recovered or unsupported
            syntax, an invalid string, or a non-finite floating-point value.
    """
    telemetry_events.on_dumps_qasm()
    start = monotonic()
    try:
        _require_syntax_program(program)
        result = _qasm_dumps(program)
    except _NativeQASMUnparseError as error:
        raise QASMUnparseError(
            str(error),
            code=error.code,
            span=error.span,
            diagnostics=error.diagnostics,
        ) from None
    durationMs = (monotonic() - start) * 1000
    telemetry_events.on_dumps_qasm_end(durationMs)
    return result


def dump(program: Program, stream: TextIO, /) -> None:
    """Write canonical OpenQASM source to a text stream exactly once.

    The stream is not flushed or closed. Exceptions from ``stream.write``
    propagate unchanged.

    As with :func:`dumps`, only a whole syntactic program is accepted.

    Args:
        program: A syntactic :class:`Program` returned by this parser.
        stream: A text stream with a ``write(str)`` method.

    Raises:
        TypeError: If ``program`` is a semantic or foreign program object.
        QASMUnparseError: If canonical serialization fails.
    """
    stream.write(dumps(program))
