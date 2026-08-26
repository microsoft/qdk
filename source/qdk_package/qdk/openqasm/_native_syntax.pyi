# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Type declarations for OpenQASM syntax trees and source information.

Most users should import these types from :mod:`qdk.openqasm.parser` or
:mod:`qdk.openqasm.source`.
"""

from typing import Callable, Dict, Iterator, List, Optional, Tuple, Union

class Span:
    """A hashable, half-open UTF-8 byte range ``[lo, hi)``.

    Spans use global offsets across the entry source and all resolved includes.
    Use :meth:`SourceMap.range_from_span` to identify the source file and
    convert the offsets to source-local lines and columns.
    """

    def __init__(self, lo: int, hi: int) -> None: ...
    @property
    def lo(self) -> int:
        """The inclusive start offset, in bytes."""

    @property
    def hi(self) -> int:
        """The exclusive end offset, in bytes."""

    def __hash__(self) -> int: ...

class PositionEncoding:
    """How a :class:`Position` counts columns within a line.

    :attr:`UTF8` counts bytes, :attr:`CODE_POINT` counts Unicode code points,
    and :attr:`UTF16` counts UTF-16 code units. Use :attr:`UTF16` for Language
    Server Protocol positions and :attr:`CODE_POINT` for ordinary Python string
    indexing. All three encodings give the same columns for ASCII text.
    """

    UTF8: PositionEncoding
    CODE_POINT: PositionEncoding
    UTF16: PositionEncoding
    @property
    def value(self) -> str:
        """The lowercase spelling accepted by position conversion APIs."""

    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class ClassicalType(QASMNode):
    """The abstract base of every type node."""

class AccessControl:
    """Whether an array reference parameter may be written through."""

    READONLY: AccessControl
    MUTABLE: AccessControl
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class IntType(ClassicalType):
    """A signed integer type, optionally sized."""

    @property
    def size(self) -> Optional[QASMNode]:
        """The declared bit width, when written."""

    def children(self) -> List[QASMNode]: ...

class UintType(ClassicalType):
    """An unsigned integer type, optionally sized."""

    @property
    def size(self) -> Optional[QASMNode]:
        """The declared bit width, when written."""

    def children(self) -> List[QASMNode]: ...

class FloatType(ClassicalType):
    """A floating-point type, optionally sized."""

    @property
    def size(self) -> Optional[QASMNode]:
        """The declared bit width, when written."""

    def children(self) -> List[QASMNode]: ...

class AngleType(ClassicalType):
    """An angle type, optionally sized."""

    @property
    def size(self) -> Optional[QASMNode]:
        """The declared bit width, when written."""

    def children(self) -> List[QASMNode]: ...

class BitType(ClassicalType):
    """A bit or bit-register type, optionally sized."""

    @property
    def size(self) -> Optional[QASMNode]:
        """The declared register width, when written."""

    def children(self) -> List[QASMNode]: ...

class ComplexType(ClassicalType):
    """A complex type, optionally carrying its component float type."""

    @property
    def base_type(self) -> Optional[QASMNode]:
        """The component type of the real and imaginary parts, when written."""

    def children(self) -> List[QASMNode]: ...

class BoolType(ClassicalType):
    """A boolean type."""

    def children(self) -> List[QASMNode]: ...

class DurationType(ClassicalType):
    """A duration type."""

    def children(self) -> List[QASMNode]: ...

class StretchType(ClassicalType):
    """A stretch type."""

    def children(self) -> List[QASMNode]: ...

class QubitType(ClassicalType):
    """A qubit parameter type, optionally sized."""

    @property
    def size(self) -> Optional[QASMNode]:
        """The declared register width, when written."""

    def children(self) -> List[QASMNode]: ...

class ErrorType(ClassicalType):
    """A type that could not be parsed."""

    def children(self) -> List[QASMNode]: ...

class ArrayType(ClassicalType):
    """A sized array type."""

    @property
    def base_type(self) -> ClassicalType:
        """The element type."""

    @property
    def dimensions(self) -> List[QASMNode]:
        """The length of each dimension, outermost first."""

    def children(self) -> List[QASMNode]: ...

class StaticArrayReferenceType(ClassicalType):
    """An array reference with statically known dimensions."""

    @property
    def base_type(self) -> ClassicalType:
        """The element type."""

    @property
    def dimensions(self) -> List[QASMNode]:
        """The length of each dimension, outermost first."""

    @property
    def mutability(self) -> AccessControl:
        """Whether the referenced array is readonly or mutable."""

    def children(self) -> List[QASMNode]: ...

class DynArrayReferenceType(ClassicalType):
    """An array reference with a dynamic number of dimensions."""

    @property
    def base_type(self) -> ClassicalType:
        """The element type."""

    @property
    def num_dimensions(self) -> QASMNode:
        """The expression giving the number of dimensions."""

    @property
    def mutability(self) -> AccessControl:
        """Whether the referenced array is readonly or mutable."""

    def children(self) -> List[QASMNode]: ...

class ResolutionStatus:
    """How a source in a snapshot was obtained."""

    ENTRY: ResolutionStatus
    RESOLVED: ResolutionStatus
    UNRESOLVED: ResolutionStatus
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class Position:
    """A frozen, hashable zero-based line and column in a source file.

    ``line`` and ``column`` must be between ``0`` and ``2**32 - 1``;
    construction raises ``OverflowError`` otherwise.
    """

    def __init__(
        self,
        line: int,
        column: int,
        encoding: PositionEncoding = ...,
    ) -> None: ...
    @property
    def line(self) -> int:
        """The zero-based line number."""

    @property
    def column(self) -> int:
        """The zero-based column, counted according to :attr:`encoding`."""

    @property
    def encoding(self) -> PositionEncoding:
        """The encoding used for ``column``."""

    def __hash__(self) -> int: ...

class SourceRange:
    """A frozen, hashable range within one source file.

    ``source_id`` must be between ``0`` and ``2**32 - 1``; construction raises
    ``OverflowError`` otherwise. Use :meth:`SourceMap.span_from_range` to
    convert this source-local range to a global :class:`Span`.
    """

    def __init__(self, source_id: int, start: Position, end: Position) -> None: ...
    @property
    def source_id(self) -> int:
        """The identifier of the source file containing the range."""

    @property
    def start(self) -> Position:
        """The inclusive range boundary."""

    @property
    def end(self) -> Position:
        """The exclusive range boundary."""

    def __hash__(self) -> int: ...

class SourceFile:
    """One source file in a parse or analysis result."""

    @property
    def id(self) -> int:
        """The source file's stable identifier within the snapshot."""

    @property
    def path(self) -> str:
        """The logical path used to resolve this source.

        For an include, this is the normalized path passed to the include
        resolver. It is not necessarily a filesystem path.
        """

    @property
    def text(self) -> str:
        """The complete source text."""

    @property
    def span(self) -> Span:
        """The span covering the complete source text."""

    @property
    def is_entry(self) -> bool:
        """Whether this is the parse entry source."""

    @property
    def is_resolved(self) -> bool:
        """Whether the include resolver supplied this source."""

    @property
    def resolution_status(self) -> ResolutionStatus:
        """How this source entered the snapshot."""

    def __hash__(self) -> int: ...

class SourceMap:
    """The source files and coordinate conversions for one result.

    Lines and columns are zero based. Coordinate conversion is strict and
    raises ``ValueError`` rather than clamping invalid boundaries.
    """

    @property
    def entry(self) -> SourceFile:
        """The entry source file."""

    @property
    def files(self) -> Tuple[SourceFile, ...]:
        """All source files in parser pre-order."""

    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator[SourceFile]: ...
    def get(self, source_id: int) -> SourceFile:
        """Returns the source file with ``source_id``.

        Raises ``KeyError`` when the ID is not in this source map.
        """

    def find(self, path: str) -> Optional[SourceFile]:
        """Returns the first source whose logical path exactly matches ``path``.

        Matching is case-sensitive. Returns ``None`` when no source matches.
        """

    def find_all(self, path: str) -> Tuple[SourceFile, ...]:
        """Returns all sources whose logical path exactly matches ``path``.

        Matching is case-sensitive. The tuple is empty when no source matches.
        """

    def position_at(
        self,
        source_id: int,
        byte_offset: int,
        *,
        encoding: PositionEncoding = ...,
    ) -> Position:
        """Converts a source-local UTF-8 byte offset to a line and column.

        ``byte_offset`` is relative to the start of ``source_id``; it is not a
        global :class:`Span` offset. Use :meth:`range_from_span` when starting
        from a node, symbol, or diagnostic span.

        The default column encoding is :attr:`PositionEncoding.CODE_POINT`.
        Raises ``ValueError`` for an unknown source, an out-of-range offset, or
        an offset that is not a UTF-8 character boundary.
        """

    def byte_offset(self, source_id: int, position: Position) -> int:
        """Converts a source-local line and column to a UTF-8 byte offset.

        The returned offset is relative to the start of ``source_id``, not a
        global :class:`Span` offset.

        The position's own encoding controls how its column is interpreted.
        Raises ``ValueError`` for an unknown source or invalid position.
        """

    def range_from_span(
        self,
        span: Span,
        *,
        encoding: PositionEncoding = ...,
    ) -> SourceRange:
        """Converts a global byte span to a source-local line and column range.

        The default column encoding is :attr:`PositionEncoding.CODE_POINT`.
        Raises ``ValueError`` if the span is invalid or is not contained in one
        source in this map.
        """

    def span_from_range(self, source_range: SourceRange) -> Span:
        """Converts a source-local range to a global UTF-8 byte span.

        Raises ``ValueError`` if the range is invalid, refers to an unknown
        source, or belongs to a different source document.
        """

    def __eq__(self, value: object, /) -> bool: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]

class SourceDocument:
    """The entry source and resolved includes for one parse or analysis result."""

    @property
    def entry(self) -> SourceFile:
        """The entry source file."""

    @property
    def source_map(self) -> SourceMap:
        """The source map for this immutable snapshot."""

    def __eq__(self, value: object, /) -> bool: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]

class Severity:
    """The severity of a :class:`Diagnostic`."""

    Error: Severity
    Warning: Severity
    Advice: Severity
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class Label:
    """A frozen, hashable labeled region associated with a diagnostic."""

    @property
    def span(self) -> Span:
        """The span the label points at."""

    @property
    def message(self) -> Optional[str]:
        """An optional message describing the label."""

    def __hash__(self) -> int: ...

class Diagnostic:
    """A diagnostic reported while parsing or analyzing OpenQASM source."""

    @property
    def message(self) -> str:
        """The primary, human-readable message."""

    @property
    def severity(self) -> Severity:
        """The diagnostic's severity."""

    @property
    def code(self) -> Optional[str]:
        """An optional machine-readable code (e.g. ``Qasm.Parse.Token``)."""

    @property
    def labels(self) -> List[Label]:
        """Source labels attached to the diagnostic."""

    @property
    def related(self) -> List[Diagnostic]:
        """Related diagnostics, projected recursively."""

    def __eq__(self, value: object, /) -> bool: ...
    def __str__(self) -> str:
        """The pretty, source-annotated rendering of the diagnostic."""
        ...

    def render(
        self,
        *,
        color: Optional[bool] = None,
        unicode: Optional[bool] = None,
        width: Optional[int] = None,
    ) -> str:
        """Renders the diagnostic to its pretty, source-annotated form.

        Unlike ``str(diagnostic)``, which is a fixed no-color rendering, this
        lets the caller control the output for the current terminal:

        * ``color`` - emit ANSI color. When ``None``, color is enabled only if
            standard output is a terminal and ``NO_COLOR`` is unset.
        * ``unicode`` - use Unicode box-drawing (``True``) or ASCII (``False``).
            Defaults to ``True``.
        * ``width`` - wrap width in columns. Defaults to 80.
        """
    __hash__: ClassVar[None]  # type: ignore[assignment]

# --- classes shared by both layers (qdk.openqasm) ---

class QASMNode:
    """The abstract root of every `OpenQASM` AST node."""

    @property
    def span(self) -> Span:
        """The source span this node covers."""

    def __eq__(self, value: object, /) -> bool: ...

class Expression(QASMNode):
    """The abstract base of every expression node."""

class Statement(QASMNode):
    """The abstract base of every statement node."""

    @property
    def annotations(self) -> List["Annotation"]:
        """The annotations attached to this statement, in source order.

        ``children()`` also returns annotations before the statement's other
        children, so visitors encounter them automatically.
        """

class Annotation(QASMNode):
    """An annotation attached to an OpenQASM statement."""

    @property
    def identifier(self) -> str:
        """The annotation's dotted identifier, without the leading `@`."""

    @property
    def value(self) -> Optional[str]:
        """The annotation's remaining text, when it has any."""

    @property
    def value_span(self) -> Optional[Span]:
        """The span covering the annotation's value, when it has one."""

    def children(self) -> List[QASMNode]: ...

# --- syntactic-only nodes (qdk.openqasm.parser) ---

class Program(QASMNode):
    """The root of a parsed `OpenQASM` program."""

    @property
    def version(self) -> Optional[str]:
        """The declared `OpenQASM` version, if any (for example `\"3.0\"`)."""

    @property
    def document(self) -> SourceDocument:
        """The immutable source document for this parse snapshot."""

    @property
    def statements(self) -> List[QASMNode]:
        """The top-level statements of the program, in source order."""

    def children(self) -> List[QASMNode]: ...

class QuantumGateModifier(QASMNode):
    """A quantum gate modifier (for example ``ctrl @`` or ``pow(2) @``)."""

    @property
    def modifier(self) -> GateModifierName:
        """The modifier keyword."""

    @property
    def argument(self) -> Optional[Expression]:
        """The modifier's argument expression, if any (the exponent for `pow` or the
        optional control count for `ctrl` / `negctrl`)."""

    def children(self) -> List[QASMNode]: ...
    @property
    def modifier_keyword_span(self) -> Span:
        """The span covering the modifier keyword."""

class RangeDefinition(QASMNode):
    """A range used in an index or a `for` loop."""

    @property
    def start(self) -> Optional[Expression]:
        """The inclusive start of the range, when written."""

    @property
    def step(self) -> Optional[Expression]:
        """The step between values, when written."""

    @property
    def end(self) -> Optional[Expression]:
        """The inclusive end of the range, when written."""

    def children(self) -> List[QASMNode]: ...

class DiscreteSet(QASMNode):
    """A brace-enclosed set of values."""

    @property
    def values(self) -> List[Expression]:
        """The set's members, in source order."""

    def children(self) -> List[QASMNode]: ...

class IndexList(QASMNode):
    """The entries of one index bracket."""

    @property
    def values(self) -> List[QASMNode]:
        """The entries of one index bracket, in source order."""

    def children(self) -> List[QASMNode]: ...

class SwitchCase(QASMNode):
    """One `case` branch of a switch statement."""

    @property
    def labels(self) -> List[Expression]:
        """The case labels this branch matches."""

    @property
    def body(self) -> List[Statement]:
        """The statements run when a label matches."""

    def children(self) -> List[QASMNode]: ...

class SubroutineParameter(QASMNode):
    """One declared parameter of a subroutine."""

    @property
    def identifier(self) -> Expression:
        """The parameter's name.

        This is an :class:`Identifier` in valid source, or an
        :class:`ErrorExpression` when parsing recovered a missing name.
        """

    @property
    def type(self) -> ClassicalType:
        """The parameter's declared type."""

    def children(self) -> List[QASMNode]: ...

class Identifier(Expression):
    """An identifier expression (a reference to a name)."""

    @property
    def name(self) -> str:
        """The identifier's source text."""

    def children(self) -> List[QASMNode]: ...

class IndexedIdentifier(Expression):
    """An indexed identifier (for example ``a[i]``) in an l-value position."""

    @property
    def name(self) -> Identifier:
        """The identifier being indexed."""

    @property
    def indices(self) -> List[Expression]:
        """The index lists applied to the identifier, outermost first."""

    def children(self) -> List[QASMNode]: ...
    @property
    def index_span(self) -> Span:
        """The span covering the bracketed indices, excluding the identifier."""

class HardwareQubit(Expression):
    """A hardware-qubit gate operand (for example ``$0``)."""

    @property
    def name(self) -> str:
        """The hardware qubit's identifier text, including the leading `$`."""

    def children(self) -> List[QASMNode]: ...

class ErrorExpression(Expression):
    """A placeholder inserted when the parser recovers from an invalid expression.

    Inspect the parse result's diagnostics for the error. The placeholder keeps
    the recovered tree traversable and identifies the affected source span.
    """

    def children(self) -> List[QASMNode]: ...

class UnaryExpression(Expression):
    """A unary operator expression."""

    @property
    def op(self) -> UnaryOperator:
        """The unary operator applied to the operand."""

    @property
    def operand(self) -> Expression:
        """The expression the operator is applied to."""

    def children(self) -> List[QASMNode]: ...

class BinaryExpression(Expression):
    """A binary operator expression."""

    @property
    def op(self) -> BinaryOperator:
        """The binary operator joining the two operands."""

    @property
    def lhs(self) -> Expression:
        """The left operand."""

    @property
    def rhs(self) -> Expression:
        """The right operand."""

    def children(self) -> List[QASMNode]: ...

class BinaryOperator:
    """A binary operator.

    Member names are descriptive because the ``openqasm3`` reference enum names
    its members by symbol, which are not Python identifiers. The OpenQASM
    spelling is available as ``value``.
    """

    ADD: BinaryOperator
    SUB: BinaryOperator
    MUL: BinaryOperator
    DIV: BinaryOperator
    MOD: BinaryOperator
    POW: BinaryOperator
    EQ: BinaryOperator
    NEQ: BinaryOperator
    GT: BinaryOperator
    GTE: BinaryOperator
    LT: BinaryOperator
    LTE: BinaryOperator
    LOGIC_AND: BinaryOperator
    LOGIC_OR: BinaryOperator
    BIT_AND: BinaryOperator
    BIT_OR: BinaryOperator
    BIT_XOR: BinaryOperator
    SHL: BinaryOperator
    SHR: BinaryOperator
    @property
    def value(self) -> str:
        """The OpenQASM spelling of the operator, for example ``\">=\"``."""

    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class UnaryOperator:
    """A unary operator. The OpenQASM spelling is available as ``value``."""

    NEG: UnaryOperator
    BIT_NOT: UnaryOperator
    LOGIC_NOT: UnaryOperator
    @property
    def value(self) -> str:
        """The OpenQASM spelling of the operator, for example ``\"~\"``."""

    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class TimeUnit:
    """The time unit of a duration literal."""

    DT: TimeUnit
    NS: TimeUnit
    US: TimeUnit
    MS: TimeUnit
    S: TimeUnit
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class IOKeyword:
    """The direction of an ``input`` or ``output`` declaration.

    The OpenQASM keyword is available as ``value``.
    """

    INPUT: IOKeyword
    OUTPUT: IOKeyword
    @property
    def value(self) -> str:
        """The ``OpenQASM`` keyword, either ``\"input\"`` or ``\"output\"``."""

    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class GateModifierName:
    """The keyword naming a quantum gate modifier.

    The OpenQASM keyword is available as ``value``.
    """

    INV: GateModifierName
    POW: GateModifierName
    CTRL: GateModifierName
    NEGCTRL: GateModifierName
    @property
    def value(self) -> str:
        """The ``OpenQASM`` keyword, for example ``\"negctrl\"``."""

    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class IntegerLiteral(Expression):
    """An integer literal of arbitrary precision."""

    @property
    def value(self) -> int:
        """The literal's value as a Python `int`, of unbounded width."""

    def children(self) -> List[QASMNode]: ...

class FloatLiteral(Expression):
    """A floating-point literal."""

    @property
    def value(self) -> float:
        """The literal's value."""

    def children(self) -> List[QASMNode]: ...

class ImaginaryLiteral(Expression):
    """An imaginary literal, carrying its magnitude."""

    @property
    def value(self) -> float:
        """The imaginary coefficient, so `2.0im` carries `2.0`."""

    def children(self) -> List[QASMNode]: ...

class BooleanLiteral(Expression):
    """A boolean literal."""

    @property
    def value(self) -> bool:
        """The literal's value."""

    def children(self) -> List[QASMNode]: ...

class BitstringLiteral(Expression):
    """A bitstring literal, carrying its value and declared width."""

    @property
    def value(self) -> int:
        """The bit pattern as a Python `int`, with the leftmost bit most significant."""

    @property
    def width(self) -> int:
        """The number of bits written in the source, including leading zeros."""

    def children(self) -> List[QASMNode]: ...

class DurationLiteral(Expression):
    """A duration literal, carrying its magnitude and unit."""

    @property
    def value(self) -> float:
        """The numeric part of the duration."""

    @property
    def unit(self) -> TimeUnit:
        """The time unit the value is expressed in."""

    def children(self) -> List[QASMNode]: ...

class ArrayLiteral(Expression):
    """An array literal, exposing its elements as children."""

    @property
    def values(self) -> List[Expression]:
        """The literal's elements, in source order."""

    def children(self) -> List[QASMNode]: ...

class StringLiteral(Expression):
    """A string literal."""

    @property
    def value(self) -> str:
        """The string's contents, with the surrounding quotes removed."""

    def children(self) -> List[QASMNode]: ...

class FunctionCall(Expression):
    """A function-call expression."""

    @property
    def name(self) -> Identifier:
        """The identifier naming the callee."""

    @property
    def args(self) -> List[Expression]:
        """The call arguments, in source order."""

    def children(self) -> List[QASMNode]: ...

class Cast(Expression):
    """A type-cast expression."""

    @property
    def type(self) -> ClassicalType:
        """The type the operand is cast to."""

    @property
    def operand(self) -> Expression:
        """The expression being cast."""

    def children(self) -> List[QASMNode]: ...

class IndexExpression(Expression):
    """An index expression (for example ``a[i]``)."""

    @property
    def collection(self) -> Expression:
        """The expression being indexed."""

    @property
    def indices(self) -> List[QASMNode]:
        """The index lists applied to the collection, outermost first."""

    def children(self) -> List[QASMNode]: ...

class ParenExpression(Expression):
    """A parenthesized expression."""

    @property
    def operand(self) -> Expression:
        """The expression inside the parentheses."""

    def children(self) -> List[QASMNode]: ...

class DurationOf(Expression):
    """A ``durationof`` expression over a block of statements."""

    @property
    def body(self) -> List[Statement]:
        """The statements whose duration is being measured, in source order."""

    def children(self) -> List[QASMNode]: ...
    @property
    def name_span(self) -> Span:
        """The span covering the `durationof` keyword."""

class Concatenation(Expression):
    """A concatenation r-value (for example ``a ++ b``)."""

    @property
    def operands(self) -> List[Expression]:
        """The operands joined by `++`, in source order."""

    def children(self) -> List[QASMNode]: ...

class QuantumMeasurement(Expression):
    """A measurement r-value (for example ``measure q``)."""

    @property
    def qubits(self) -> List[Expression]:
        """The qubits being measured."""

    def children(self) -> List[QASMNode]: ...
    @property
    def measure_token_span(self) -> Span:
        """The span covering the `measure` keyword."""

class QubitDeclaration(Statement):
    """A qubit declaration statement (for example ``qubit q;``)."""

    @property
    def qubit(self) -> Identifier:
        """The identifier naming the declared qubit or register."""

    @property
    def size(self) -> Optional[Expression]:
        """The register width, when the declaration is an array."""

    def children(self) -> List[QASMNode]: ...

class AliasStatement(Statement):
    """An alias declaration statement (``let``)."""

    @property
    def target(self) -> Expression:
        """The identifier the alias binds."""

    @property
    def exprs(self) -> List[Expression]:
        """The expressions the alias refers to, joined by `++` when several."""

    def children(self) -> List[QASMNode]: ...

class ClassicalAssignment(Statement):
    """A classical assignment statement (``a = b;``)."""

    @property
    def lhs(self) -> Expression:
        """The assignment target."""

    @property
    def rhs(self) -> Expression:
        """The assigned value."""

    def children(self) -> List[QASMNode]: ...

class CompoundAssignment(Statement):
    """A compound assignment statement (for example ``a += b;``)."""

    @property
    def op(self) -> BinaryOperator:
        """The underlying operator, so `+=` reports addition."""

    @property
    def lhs(self) -> Expression:
        """The assignment target."""

    @property
    def rhs(self) -> Expression:
        """The right operand of the compound operation."""

    def children(self) -> List[QASMNode]: ...

class QuantumBarrier(Statement):
    """A ``barrier`` statement."""

    @property
    def qubits(self) -> List[Expression]:
        """The qubits the barrier applies to."""

    def children(self) -> List[QASMNode]: ...

class Box(Statement):
    """A ``box`` statement."""

    @property
    def duration(self) -> Optional[Expression]:
        """The box's declared duration, when written."""

    @property
    def body(self) -> List[QASMNode]:
        """The statements inside the box."""

    def children(self) -> List[QASMNode]: ...

class BreakStatement(Statement):
    """A ``break`` statement."""

    def children(self) -> List[QASMNode]: ...

class CompoundStatement(Statement):
    """A block of statements (``{ ... }``)."""

    @property
    def statements(self) -> List[QASMNode]:
        """The statements inside the block, in source order."""

    def children(self) -> List[QASMNode]: ...

class CalibrationStatement(Statement):
    """A ``cal`` calibration block."""

    @property
    def body(self) -> str:
        """The calibration block's raw text, which this parser does not interpret."""

    def children(self) -> List[QASMNode]: ...

class CalibrationGrammarDeclaration(Statement):
    """A ``defcalgrammar`` declaration."""

    @property
    def name(self) -> str:
        """The named calibration grammar, for example `openpulse`."""

    def children(self) -> List[QASMNode]: ...

class ClassicalDeclaration(Statement):
    """A classical variable declaration."""

    @property
    def type(self) -> ClassicalType:
        """The declared type."""

    @property
    def identifier(self) -> Identifier:
        """The identifier being declared."""

    @property
    def init_expr(self) -> Optional[Expression]:
        """The initializer, when the declaration has one."""

    def children(self) -> List[QASMNode]: ...

class ConstantDeclaration(Statement):
    """A ``const`` declaration."""

    @property
    def type(self) -> ClassicalType:
        """The declared type."""

    @property
    def identifier(self) -> Identifier:
        """The identifier being declared."""

    @property
    def init_expr(self) -> Expression:
        """The initializer, which a constant always has."""

    def children(self) -> List[QASMNode]: ...

class ContinueStatement(Statement):
    """A ``continue`` statement."""

    def children(self) -> List[QASMNode]: ...

class SubroutineDefinition(Statement):
    """A ``def`` subroutine definition."""

    @property
    def name(self) -> Identifier:
        """The identifier naming the subroutine."""

    @property
    def params(self) -> List[SubroutineParameter]:
        """The declared parameters, in source order."""

    @property
    def return_type(self) -> Optional[ClassicalType]:
        """The declared return type, when the subroutine returns a value."""

    @property
    def body(self) -> List[QASMNode]:
        """The statements making up the subroutine body."""

    def children(self) -> List[QASMNode]: ...

class CalibrationDefinition(Statement):
    """A ``defcal`` calibration definition."""

    @property
    def body(self) -> str:
        """The `defcal` block's raw text, which this parser does not interpret."""

    def children(self) -> List[QASMNode]: ...

class DelayInstruction(Statement):
    """A ``delay`` instruction."""

    @property
    def duration(self) -> Expression:
        """The delay's duration."""

    @property
    def qubits(self) -> List[Expression]:
        """The qubits the delay applies to."""

    def children(self) -> List[QASMNode]: ...

class EndStatement(Statement):
    """An ``end`` statement."""

    def children(self) -> List[QASMNode]: ...

class ExpressionStatement(Statement):
    """A statement that evaluates an expression."""

    @property
    def expr(self) -> Expression:
        """The evaluated expression."""

    def children(self) -> List[QASMNode]: ...

class ExternDeclaration(Statement):
    """An ``extern`` declaration."""

    @property
    def name(self) -> Identifier:
        """The identifier naming the external subroutine."""

    @property
    def param_types(self) -> List[ClassicalType]:
        """The declared parameter types, in source order."""

    @property
    def return_type(self) -> Optional[ClassicalType]:
        """The declared return type, when the subroutine returns a value."""

    def children(self) -> List[QASMNode]: ...

class ForInLoop(Statement):
    """A ``for`` loop over an iterable set."""

    @property
    def type(self) -> ClassicalType:
        """The loop variable's declared type."""

    @property
    def identifier(self) -> Identifier:
        """The loop variable."""

    @property
    def iterable(self) -> QASMNode:
        """The range, set, or expression being iterated."""

    @property
    def body(self) -> QASMNode:
        """The loop body."""

    def children(self) -> List[QASMNode]: ...

class BranchingStatement(Statement):
    """An ``if`` / ``else`` branching statement."""

    @property
    def condition(self) -> Expression:
        """The branch condition."""

    @property
    def if_body(self) -> QASMNode:
        """The statement run when the condition holds."""

    @property
    def else_body(self) -> Optional[QASMNode]:
        """The `else` branch, when written."""

    def children(self) -> List[QASMNode]: ...

class QuantumGate(Statement):
    """A quantum gate call."""

    @property
    def name(self) -> Identifier:
        """The identifier naming the gate."""

    @property
    def modifiers(self) -> List[QuantumGateModifier]:
        """The modifiers applied to the gate, such as `ctrl` or `inv`."""

    @property
    def args(self) -> List[Expression]:
        """The classical arguments, such as a rotation angle."""

    @property
    def qubits(self) -> List[Expression]:
        """The qubit operands the gate acts on."""

    @property
    def duration(self) -> Optional[Expression]:
        """The gate's declared duration, when written."""

    def children(self) -> List[QASMNode]: ...

class QuantumPhase(Statement):
    """A ``gphase`` statement."""

    @property
    def modifiers(self) -> List[QuantumGateModifier]:
        """The modifiers applied to the phase, such as `ctrl`."""

    @property
    def args(self) -> List[Expression]:
        """The phase arguments."""

    @property
    def qubits(self) -> List[Expression]:
        """The qubit operands, present only when the phase is controlled."""

    @property
    def duration(self) -> Optional[Expression]:
        """The declared duration, when written."""

    def children(self) -> List[QASMNode]: ...
    @property
    def gphase_token_span(self) -> Span:
        """The span covering the `gphase` keyword."""

class Include(Statement):
    """An ``include`` directive."""

    @property
    def filename(self) -> str:
        """The included file's path as written in the source."""

    def children(self) -> List[QASMNode]: ...

class IODeclaration(Statement):
    """An ``input`` / ``output`` declaration."""

    @property
    def io_keyword(self) -> IOKeyword:
        """Whether the declaration is an `input` or an `output`."""

    @property
    def type(self) -> ClassicalType:
        """The declared type."""

    @property
    def identifier(self) -> Identifier:
        """The identifier being declared."""

    def children(self) -> List[QASMNode]: ...

class QuantumMeasurementStatement(Statement):
    """A measurement statement (for example ``c = measure q;``)."""

    @property
    def qubits(self) -> List[Expression]:
        """The qubits being measured."""

    @property
    def target(self) -> Optional[Expression]:
        """The classical target the result is written to, when written."""

    def children(self) -> List[QASMNode]: ...

class Pragma(Statement):
    """A ``pragma`` directive.

    :attr:`command` contains all text after the keyword. :attr:`name` and
    :attr:`value` split that text into its leading dotted identifier and the
    remaining content for convenient inspection.
    """

    @property
    def command(self) -> str:
        """The pragma's full text after the keyword."""

    @property
    def name(self) -> Optional[str]:
        """The leading dotted identifier, when the pragma has one."""

    @property
    def value(self) -> Optional[str]:
        """The remaining text after the identifier, when present."""

    def children(self) -> List[QASMNode]: ...
    @property
    def command_span(self) -> Span:
        """The span covering the full command text after the keyword."""

    @property
    def value_span(self) -> Optional[Span]:
        """The span covering the text after the identifier, when there is any."""

class QuantumGateDefinition(Statement):
    """A ``gate`` definition.

    ``params`` and ``qubits`` hold :class:`Identifier` nodes. A parameter the
    parser recovered from is an :class:`ErrorExpression` at the span it would
    have occupied.
    """

    @property
    def name(self) -> Identifier:
        """The identifier naming the gate."""

    @property
    def params(self) -> List[Expression]:
        """The classical parameters, in source order.

        Each item is an :class:`Identifier` in valid source, or an
        :class:`ErrorExpression` where parsing recovered a missing parameter.
        """

    @property
    def qubits(self) -> List[Expression]:
        """The qubit parameters, in source order.

        Each item is an :class:`Identifier` in valid source, or an
        :class:`ErrorExpression` where parsing recovered a missing parameter.
        """

    @property
    def body(self) -> List[QASMNode]:
        """The statements making up the gate body."""

    def children(self) -> List[QASMNode]: ...

class QuantumReset(Statement):
    """A ``reset`` statement."""

    @property
    def qubits(self) -> List[Expression]:
        """The qubits being reset."""

    def children(self) -> List[QASMNode]: ...
    @property
    def reset_token_span(self) -> Span:
        """The span covering the `reset` keyword."""

class ReturnStatement(Statement):
    """A ``return`` statement."""

    @property
    def value(self) -> Optional[Expression]:
        """The returned expression, when the subroutine returns a value."""

    def children(self) -> List[QASMNode]: ...

class SwitchStatement(Statement):
    """A ``switch`` statement."""

    @property
    def target(self) -> Expression:
        """The expression being switched on."""

    @property
    def cases(self) -> List[SwitchCase]:
        """The `case` branches, in source order."""

    @property
    def default(self) -> Optional[List[Statement]]:
        """The `default` branch's statements, or `None` when there is no `default`."""

    def children(self) -> List[QASMNode]: ...

class WhileLoop(Statement):
    """A ``while`` loop."""

    @property
    def condition(self) -> Expression:
        """The loop condition, tested before each iteration."""

    @property
    def body(self) -> QASMNode:
        """The loop body."""

    def children(self) -> List[QASMNode]: ...

class ErrorStatement(Statement):
    """A placeholder inserted when the parser recovers from an invalid statement.

    Inspect the parse result's diagnostics for the error. The placeholder keeps
    the recovered tree traversable and identifies the affected source span.
    """

    def children(self) -> List[QASMNode]: ...

class ParseResult:
    """The result of a syntactic :func:`parse`."""

    @property
    def program(self) -> Program:
        """The root of the parsed syntactic program."""

    @property
    def document(self) -> SourceDocument:
        """The immutable source document for this parse snapshot."""

    @property
    def diagnostics(self) -> List[Diagnostic]:
        """All diagnostics (parse errors) produced while parsing."""

    @property
    def has_errors(self) -> bool:
        """Whether any errors were produced."""

class _QASMUnparseError(ValueError):
    """Internal checked serialization error carrier."""

    code: str
    span: Optional[Span]
    diagnostics: Tuple[Diagnostic, ...]

def parse(
    source: str,
    path: str = ...,
    includes: Optional[Union[Dict[str, str], Callable[[str], Optional[str]]]] = ...,
) -> ParseResult:
    """Parses `OpenQASM` source text into a syntax tree."""
    ...

def qasm_dumps(program: Program) -> str:
    """Canonically serializes a syntactic program from its entry source.

    Only a syntactic `Program` is accepted. The parameter may widen to other
    node kinds once an emitter that walks the Python nodes exists.
    """
    ...
