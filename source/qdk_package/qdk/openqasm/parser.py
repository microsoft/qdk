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

``result.document`` is the immutable source snapshot for the parse. Syntax and
diagnostic spans are global, half-open UTF-8 byte ranges; map them to a source
with ``result.document.source_map.range_from_span(span)``.

The ``includes`` argument accepts a mapping or callback over platform-neutral
logical identifiers. Use ``/`` separators. Relative ``.`` and ``..`` components
are normalized against the including source's logical parent, and URI-like
schemes are preserved without URL decoding or fetching. Caller-provided key
matching is exact and case-sensitive. ``stdgates.inc``, ``qelib1.inc``, and the
QDK extension ``qdk.inc`` are built in. Parsing recognizes ``qdk.inc`` without
consulting the resolver; semantic analysis injects the
``mresetz_checked(qubit) -> int`` and ``postselectz(bit, qubit) -> void``
intrinsic declarations. ``mresetz_checked`` returns ``0`` for Zero, ``1`` for
One, or ``2`` for qubit loss. Those names are unavailable without the include.
No other include falls back to the filesystem or network. Missing keys and
callback exceptions become result diagnostics with unresolved source entries.
A new resolver bridge is used for each call, and the result does not retain the
mapping or callback.

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
call returns. Node identity is local to one parse snapshot: repeated access to
a child within a result preserves identity, but rewriting creates a new graph
and does not preserve identities for unchanged subtrees.

Nodes compare and hash structurally, not by identity. Two trees parsed from the
same source are equal and hash equally, and so are two nodes describing the same
construct at different offsets, because ``span`` does not participate in either
operation. That makes nodes usable as ``set`` members and ``dict`` keys. One
consequence is worth knowing: structurally identical nodes taken from different
documents also compare equal, since neither position nor source document
participates.

Most class names here are also class names in :mod:`qdk.openqasm.semantic`, so a
value named ``Program``, ``QuantumGate``, or ``IntType`` does not say which tree
produced it. Use ``isinstance(node, SyntaxNode)`` to ask. :class:`SyntaxNode` is
a virtual base with no members: every class in this tree is registered against
it at import time, and :class:`qdk.openqasm.semantic.SemanticNode` is the
counterpart for the other tree. Reach for it at an API boundary that must reject
the wrong tree, or while diagnosing where a node came from; it resolves through
``ABCMeta.__instancecheck__``, which is not what you want inside a traversal
loop.

Four classes answer ``False`` to both questions: :class:`QASMNode`,
:class:`Expression`, :class:`Statement`, and :class:`Annotation`. Both trees use
them, so asking which tree one came from has no answer, and claiming either
would be false. Everything a parse actually produces is a :class:`SyntaxNode`.

This API is in preview and may change between QDK releases.

Serialization accepts only syntax ``Program`` objects; it rejects semantic
programs and does not retain comments or original spelling.
"""

from __future__ import annotations

from time import monotonic
from typing import Callable, Dict, Optional, TextIO, Union

from .._native import (  # type: ignore
    AccessControl,
    AliasStatement,
    AngleType,
    Annotation,
    ArrayType,
    BinaryExpression,
    BinaryOperator,
    BitType,
    BoolType,
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
    GateModifierName,
    IOKeyword,
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
    FloatType,
    ForInLoop,
    FunctionCall,
    HardwareQubit,
    IODeclaration,
    Identifier,
    Include,
    IndexExpression,
    IndexList,
    IndexedIdentifier,
    IntType,
    Label,
    ArrayLiteral,
    BitstringLiteral,
    BooleanLiteral,
    DurationLiteral,
    FloatLiteral,
    ImaginaryLiteral,
    IntegerLiteral,
    StringLiteral,
    TimeUnit,
    ParenExpression,
    ParseResult,
    Position,
    PositionEncoding,
    Pragma,
    Program,
    QASMNode,
    QubitDeclaration,
    QubitType,
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
    ResolutionStatus,
    Severity,
    SourceDocument,
    SourceFile,
    SourceMap,
    SourceRange,
    Span,
    StaticArrayReferenceType,
    Statement,
    StretchType,
    SubroutineParameter,
    SubroutineDefinition,
    SwitchCase,
    SwitchStatement,
    UintType,
    UnaryExpression,
    UnaryOperator,
    WhileLoop,
    _QASMUnparseError as _NativeQASMUnparseError,
    parse as _parse,
    qasm_dumps as _qasm_dumps,
)
from .._native import _semantic  # type: ignore
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
    includes: Optional[Union[Dict[str, str], Callable[[str], Optional[str]]]] = None,
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

    This wrapper offers control-flow compatibility with the reference
    ``openqasm3`` parser: errors raise unless ``permissive`` is true. It returns
    QDK syntax objects and does not provide reference AST compatibility,
    ANTLR diagnostic text, or identical recovery behavior. The underlying
    recovery-oriented :func:`parse` API remains unchanged.

    Args:
        source: The OpenQASM 2.0 or 3.0 source text to parse.
        permissive: Return the recovered program even when diagnostics exist.
        path: A display name for the source, used in diagnostics.
        includes: How to resolve ``include`` directives.

    Returns:
        The program from the single underlying parse result.

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
    if isinstance(program, _semantic.Program):
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

    Output is derived from the entry source of the program's source document
    rather than by walking the node tree, so this costs one reparse per call.
    Trees are immutable, so the result always matches the program passed in.

    Only a whole syntactic program is accepted. ``openqasm3.dumps`` serializes
    any node, and a statement, an expression, or a semantic tree has no entry
    source to reparse, so serializing one needs an emitter that walks the node
    tree. The parameter may widen once such an emitter exists.

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
