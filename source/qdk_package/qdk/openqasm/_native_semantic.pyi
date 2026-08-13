# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Type declarations for the semantic OpenQASM native surface.

The semantic node classes are registered into ``qdk._native._semantic``, an
attribute-only namespace with no ``sys.modules`` entry, so they are rebound by
the sibling shim rather than imported. This stub describes what that shim
exports, which lets sibling references be written as plain names.
"""

from typing import Any, Callable, Dict, List, Optional, Union

from ._native_syntax import (
    AccessControl,
    BinaryOperator,
    Diagnostic,
    Expression,
    GateModifierName,
    QASMNode,
    SourceDocument,
    Span,
    Statement,
    TimeUnit,
    UnaryOperator,
)

class CastKind:
    """Whether a cast was written in the source or inserted by the analyzer."""

    EXPLICIT: CastKind
    IMPLICIT: CastKind
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class IOKind:
    """Whether a symbol is a program input, a program output, or neither."""

    DEFAULT: IOKind
    INPUT: IOKind
    OUTPUT: IOKind
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class Angle:
    """A constant angle value, stored as OpenQASM stores it: a fixed-point integer."""

    def __init__(self, value: int, size: int) -> None: ...
    @property
    def value(self) -> int:
        """The fixed-point numerator, in units of a full turn divided by ``2 ** size``."""
    @property
    def size(self) -> int:
        """The number of bits ``value`` is expressed in.

        This is the precision the analyzer folded the constant at, not the
        width written in the source. A ``const angle[4]`` is folded at the
        full 53 bits a float carries; the declared width is on the
        expression's ``ty``.
        """
    @property
    def radians(self) -> float:
        """The angle in radians, in the range ``[0, 2 * pi)``.

        Derived from ``value`` and ``size`` rather than stored by the analyzer, so it
        is lossy whenever ``size`` exceeds the 53 bits a Python float can hold. It
        is ``nan`` when ``size`` exceeds 64, which no representable angle reaches.
        """

class Duration:
    """A constant duration value: a magnitude paired with the unit it was written in."""

    def __init__(self, value: float, unit: TimeUnit) -> None: ...
    @property
    def value(self) -> float:
        """The magnitude, expressed in ``unit``."""
    @property
    def unit(self) -> TimeUnit:
        """The unit the duration was written in."""

class Type:
    """The abstract base of every resolved semantic type.

    A resolved type is analysis output, not syntax, so it carries no source
    position and is not a ``QASMNode``. Dispatch over the concrete
    subclasses with ``isinstance``.
    """

    @property
    def name(self) -> str:
        """The type's textual rendering, for example ``"int[32]"`` or ``"qubit"``."""
    @property
    def is_const(self) -> bool:
        """Whether the type is compile-time constant."""
    def children(self) -> List[Type]: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __str__(self) -> str: ...

class IntType(Type):
    """A resolved signed integer type."""

    @property
    def size(self) -> Optional[int]:
        """The resolved bit width, when the type has one."""

class UintType(Type):
    """A resolved unsigned integer type."""

    @property
    def size(self) -> Optional[int]:
        """The resolved bit width, when the type has one."""

class FloatType(Type):
    """A resolved floating-point type."""

    @property
    def size(self) -> Optional[int]:
        """The resolved bit width, when the type has one."""

class AngleType(Type):
    """A resolved angle type."""

    @property
    def size(self) -> Optional[int]:
        """The resolved bit width, when the type has one."""

class ComplexType(Type):
    """A resolved complex type."""

    @property
    def size(self) -> Optional[int]:
        """The resolved bit width of each component, when the type has one.

        A `complex[float[64]]` resolves to a size of 64, describing one
        component; the syntax layer instead carries the component type node.
        """

class BitType(Type):
    """A resolved single-bit type."""

class BoolType(Type):
    """A resolved boolean type."""

class DurationType(Type):
    """A resolved duration type."""

class StretchType(Type):
    """A resolved stretch type."""

class QubitType(Type):
    """A resolved single-qubit type."""

class HardwareQubitType(Type):
    """A resolved hardware qubit type, as written ``$0``."""

class BitArrayType(Type):
    """A resolved bit register type."""

    @property
    def size(self) -> int:
        """The register width."""

class QubitArrayType(Type):
    """A resolved qubit register type."""

    @property
    def size(self) -> int:
        """The register width."""

class ArrayType(Type):
    """A resolved array type."""

    @property
    def base_type(self) -> Type:
        """The element type."""
    @property
    def dimensions(self) -> List[int]:
        """The length of each dimension, outermost first."""

class StaticArrayReferenceType(Type):
    """A resolved array reference whose dimension lengths are all known."""

    @property
    def base_type(self) -> Type:
        """The element type."""
    @property
    def dimensions(self) -> List[int]:
        """The length of each dimension, outermost first."""
    @property
    def mutability(self) -> AccessControl:
        """Whether the referenced array is readonly or mutable."""

class DynArrayReferenceType(Type):
    """A resolved array reference declared with ``#dim``, so only its rank is known."""

    @property
    def base_type(self) -> Type:
        """The element type."""
    @property
    def num_dimensions(self) -> int:
        """The number of dimensions, which is known even though their lengths are not."""
    @property
    def mutability(self) -> AccessControl:
        """Whether the referenced array is readonly or mutable."""

class GateType(Type):
    """A resolved gate type."""

    @property
    def num_classical_args(self) -> int:
        """The number of classical parameters the gate declares."""
    @property
    def num_qubit_args(self) -> int:
        """The number of qubit parameters the gate declares."""

class FunctionType(Type):
    """A resolved subroutine signature."""

    @property
    def parameter_types(self) -> List[Type]:
        """The parameter types, in declaration order."""
    @property
    def return_type(self) -> Type:
        """The return type, which is `VoidType` for a subroutine that returns nothing."""

class RangeType(Type):
    """The resolved type of a range expression."""

class SetType(Type):
    """The resolved type of a discrete set expression."""

class VoidType(Type):
    """The resolved type of an expression that yields no value."""

class ErrorType(Type):
    """The resolved type analysis assigns when it could not determine one."""

class Symbol:
    """A read-only view of a resolved symbol."""

    @property
    def id(self) -> int:
        """The symbol's unique id within the containing :class:`SymbolTable`."""
    @property
    def name(self) -> str:
        """The symbol's name."""
    @property
    def span(self) -> Span:
        """The span where the symbol is declared."""
    @property
    def ty(self) -> Type:
        """The symbol's resolved type."""
    @property
    def io_kind(self) -> IOKind:
        """Whether the symbol is a program input, a program output, or neither."""
    @property
    def const_value(self) -> Optional[Any]:
        """The symbol's const-evaluated value, if it is a constant."""
    @property
    def ty_span(self) -> Span:
        """The span covering the type as written in the source."""

class SymbolTable:
    """An iterable, read-only projection of the resolved symbol table."""

    def __len__(self) -> int: ...
    def __iter__(self) -> Any: ...
    def get(self, id: int) -> Optional[Symbol]: ...
    def lookup(self, name: str) -> Optional[Symbol]: ...
    def symbols(self) -> List[Symbol]: ...

class SemanticExpression(Expression):
    """The base of every semantic expression node."""

    @property
    def ty(self) -> Type: ...
    @property
    def const_value(self) -> Optional[Any]: ...
    @property
    def symbol(self) -> Optional[Symbol]: ...

class SemanticStatement(Statement):
    """The base of every semantic statement node."""

class Program(QASMNode):
    """The root of a semantic `OpenQASM` program."""

    @property
    def version(self) -> Optional[str]:
        """The declared `OpenQASM` version, if any (for example `"3.0"`)."""
    @property
    def pragmas(self) -> List[QASMNode]:
        """The program's top-level pragmas, in source order."""
    @property
    def statements(self) -> List[QASMNode]:
        """The program's top-level statements, in source order."""
    @property
    def document(self) -> SourceDocument:
        """The immutable source document for this analysis snapshot."""
    def children(self) -> List[QASMNode]: ...

class HardwareQubit(SemanticExpression):
    """A hardware-qubit gate operand (for example ``$0``)."""

    @property
    def name(self) -> str:
        """The hardware qubit's identifier text (for example `"$0"`)."""
    def children(self) -> List[QASMNode]: ...

class QuantumGateModifier(QASMNode):
    """A semantic quantum gate modifier."""

    @property
    def modifier(self) -> GateModifierName:
        """The modifier keyword."""
    @property
    def argument(self) -> Optional[Expression]:
        """The modifier's argument, such as a `pow` exponent or a control count."""
    def children(self) -> List[QASMNode]: ...
    @property
    def modifier_keyword_span(self) -> Span:
        """The span covering the modifier keyword."""

class RangeDefinition(QASMNode):
    """A semantic range, as written in a slice or ``for`` loop."""

    @property
    def ty(self) -> Type:
        """The range's resolved type, which is always `RangeType`."""
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
    """A semantic brace-delimited set of values."""

    @property
    def ty(self) -> Type:
        """The set's resolved type, which is always `SetType`."""
    @property
    def values(self) -> List[Expression]:
        """The set's members, in source order."""
    def children(self) -> List[QASMNode]: ...

class SwitchCase(QASMNode):
    """A semantic ``case`` branch of a ``switch`` statement."""

    @property
    def labels(self) -> List[Expression]:
        """The case labels this branch matches."""
    @property
    def body(self) -> List[Statement]:
        """The statements run when a label matches."""
    def children(self) -> List[QASMNode]: ...

class SubroutineParameter(QASMNode):
    """A semantic subroutine parameter declaration."""

    @property
    def name(self) -> Optional[str]:
        """The parameter's name, when analysis resolved one."""
    @property
    def symbol(self) -> Symbol: ...
    @property
    def type(self) -> Type:
        """The parameter's resolved type."""
    def children(self) -> List[QASMNode]: ...

class GateParameter(QASMNode):
    """A semantic gate parameter declaration."""

    @property
    def name(self) -> Optional[str]:
        """The parameter's name, when analysis resolved one."""
    @property
    def symbol(self) -> Symbol: ...
    @property
    def type(self) -> Type:
        """The parameter's resolved type."""
    def children(self) -> List[QASMNode]: ...

# --- semantic expression nodes ---

class ErrorExpression(SemanticExpression):
    """An expression that could not be resolved."""

    def children(self) -> List[QASMNode]: ...

class Identifier(SemanticExpression):
    """A reference to a resolved symbol."""

    @property
    def name(self) -> Optional[str]:
        """The identifier's name, when analysis resolved one."""
    def children(self) -> List[QASMNode]: ...

class CapturedIdentifier(SemanticExpression):
    """A reference to a symbol captured from an enclosing scope."""

    @property
    def name(self) -> Optional[str]:
        """The identifier's name, when analysis resolved one."""
    def children(self) -> List[QASMNode]: ...

class UnaryExpression(SemanticExpression):
    """A unary operator expression."""

    @property
    def op(self) -> UnaryOperator:
        """The unary operator applied to the operand."""
    @property
    def operand(self) -> Expression:
        """The expression the operator is applied to."""
    def children(self) -> List[QASMNode]: ...

class BinaryExpression(SemanticExpression):
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

class LiteralExpression(SemanticExpression):
    """A literal expression."""

    @property
    def value(self) -> Optional[Any]:
        """The literal's value, or `None` for an array literal."""
    @property
    def elements(self) -> List[Expression]:
        """The element expressions, for an array literal."""
    def children(self) -> List[QASMNode]: ...

class FunctionCall(SemanticExpression):
    """A call to a resolved function."""

    @property
    def name(self) -> Optional[str]:
        """The callee's name, when analysis resolved one."""
    @property
    def args(self) -> List[Expression]:
        """The call arguments, in source order."""
    def children(self) -> List[QASMNode]: ...
    @property
    def fn_name_span(self) -> Span:
        """The span covering the callee's name."""

class BuiltinFunctionCall(SemanticExpression):
    """A call to a built-in function."""

    @property
    def name(self) -> str:
        """The built-in function's name."""
    @property
    def args(self) -> List[Expression]:
        """The call arguments, in source order."""
    def children(self) -> List[QASMNode]: ...
    @property
    def fn_name_span(self) -> Span:
        """The span covering the function's name."""

class Cast(SemanticExpression):
    """A type cast expression."""

    @property
    def operand(self) -> Expression:
        """The expression being cast."""
    @property
    def kind(self) -> CastKind:
        """Whether the cast was written in the source or inserted by analysis."""
    def children(self) -> List[QASMNode]: ...

class IndexExpression(SemanticExpression):
    """An indexing expression."""

    @property
    def collection(self) -> Expression:
        """The expression being indexed."""
    @property
    def indices(self) -> List[Expression]:
        """The indices applied to the collection, outermost first."""
    def children(self) -> List[QASMNode]: ...

class ParenExpression(SemanticExpression):
    """A parenthesized expression."""

    @property
    def operand(self) -> Expression:
        """The expression inside the parentheses."""
    def children(self) -> List[QASMNode]: ...

class QuantumMeasurement(SemanticExpression):
    """A measurement expression."""

    @property
    def qubits(self) -> List[Expression]:
        """The qubits being measured."""
    def children(self) -> List[QASMNode]: ...
    @property
    def measure_token_span(self) -> Span:
        """The span covering the `measure` keyword."""

class RuntimeSizeof(SemanticExpression):
    """A runtime ``sizeof`` expression."""

    @property
    def array(self) -> Expression:
        """The array whose size is being taken."""
    @property
    def dimension(self) -> Expression:
        """The dimension being measured."""
    @property
    def array_rank(self) -> int:
        """The array's number of dimensions."""
    def children(self) -> List[QASMNode]: ...
    @property
    def fn_name_span(self) -> Span:
        """The span covering the `sizeof` keyword."""

class DurationOf(SemanticExpression):
    """An evaluated ``durationof`` expression."""

    @property
    def body(self) -> List[Statement]:
        """The statements whose duration was measured."""
    def children(self) -> List[QASMNode]: ...
    @property
    def fn_name_span(self) -> Span:
        """The span covering the `durationof` keyword."""

class Concatenation(SemanticExpression):
    """A concatenation expression."""

    @property
    def operands(self) -> List[Expression]:
        """The operands joined by `++`, in source order."""
    def children(self) -> List[QASMNode]: ...

# --- semantic statement nodes ---

class AliasStatement(SemanticStatement):
    """An alias declaration statement."""

    @property
    def name(self) -> Optional[str]:
        """The alias's name, when analysis resolved one."""
    @property
    def exprs(self) -> List[Expression]:
        """The expressions the alias refers to, joined by `++` when several."""
    def children(self) -> List[QASMNode]: ...

class ClassicalAssignment(SemanticStatement):
    """An assignment statement."""

    @property
    def lhs(self) -> Expression:
        """The assignment target."""
    @property
    def rhs(self) -> Expression:
        """The assigned value, cast to the target's type when needed."""
    def children(self) -> List[QASMNode]: ...

class QuantumBarrier(SemanticStatement):
    """A barrier statement."""

    @property
    def qubits(self) -> List[Expression]:
        """The qubits the barrier applies to."""
    def children(self) -> List[QASMNode]: ...

class Box(SemanticStatement):
    """A box statement."""

    @property
    def duration(self) -> Optional[Expression]:
        """The box's declared duration, when written."""
    @property
    def body(self) -> List[Statement]:
        """The statements inside the box."""
    def children(self) -> List[QASMNode]: ...

class CompoundStatement(SemanticStatement):
    """A block of statements."""

    @property
    def statements(self) -> List[Statement]:
        """The statements inside the block, in source order."""
    def children(self) -> List[QASMNode]: ...

class BreakStatement(SemanticStatement):
    """A break statement."""

    def children(self) -> List[QASMNode]: ...

class CalibrationStatement(SemanticStatement):
    """A calibration statement."""

    @property
    def content(self) -> str:
        """The calibration block's raw text, which analysis does not interpret."""
    def children(self) -> List[QASMNode]: ...

class CalibrationGrammarDeclaration(SemanticStatement):
    """A calibration grammar statement."""

    @property
    def name(self) -> str:
        """The named calibration grammar, for example `openpulse`."""
    def children(self) -> List[QASMNode]: ...

class ClassicalDeclaration(SemanticStatement):
    """A classical variable declaration statement."""

    @property
    def name(self) -> Optional[str]:
        """The declared name, when analysis resolved one."""
    @property
    def type(self) -> Type:
        """The declared, resolved type."""
    @property
    def init_expr(self) -> Expression:
        """The initializer, defaulted by analysis when the source omitted one."""
    def children(self) -> List[QASMNode]: ...
    @property
    def ty_span(self) -> Span:
        """The span covering the type as written in the source."""

class ContinueStatement(SemanticStatement):
    """A continue statement."""

    def children(self) -> List[QASMNode]: ...

class SubroutineDefinition(SemanticStatement):
    """A subroutine definition statement."""

    @property
    def name(self) -> Optional[str]:
        """The subroutine's name, when analysis resolved one."""
    @property
    def params(self) -> List[SubroutineParameter]:
        """The declared parameters, in source order."""
    @property
    def return_type(self) -> Type:
        """The resolved return type, which is `VoidType` when the subroutine returns nothing."""
    @property
    def body(self) -> List[Statement]:
        """The statements making up the subroutine body."""
    def children(self) -> List[QASMNode]: ...
    @property
    def return_type_span(self) -> Span:
        """The span covering the return type as written in the source."""

class CalibrationDefinition(SemanticStatement):
    """A ``defcal`` statement."""

    @property
    def content(self) -> str:
        """The `defcal` block's raw text, which analysis does not interpret."""
    def children(self) -> List[QASMNode]: ...

class DelayInstruction(SemanticStatement):
    """A delay statement."""

    @property
    def duration(self) -> Expression:
        """The delay's duration."""
    @property
    def qubits(self) -> List[Expression]:
        """The qubits the delay applies to."""
    def children(self) -> List[QASMNode]: ...

class EndStatement(SemanticStatement):
    """An end statement."""

    def children(self) -> List[QASMNode]: ...

class ExpressionStatement(SemanticStatement):
    """An expression statement."""

    @property
    def expr(self) -> Expression:
        """The evaluated expression."""
    def children(self) -> List[QASMNode]: ...

class ExternDeclaration(SemanticStatement):
    """An extern declaration statement."""

    @property
    def name(self) -> Optional[str]:
        """The external subroutine's name, when analysis resolved one."""
    @property
    def type(self) -> Type:
        """The resolved signature, whose `parameter_types` and `return_type` describe the call."""
    def children(self) -> List[QASMNode]: ...

class ForInLoop(SemanticStatement):
    """A ``for`` loop statement."""

    @property
    def name(self) -> Optional[str]:
        """The loop variable's name, when analysis resolved one."""
    @property
    def type(self) -> Type:
        """The loop variable's resolved type."""
    @property
    def iterable(self) -> QASMNode:
        """The range, set, or expression being iterated."""
    @property
    def body(self) -> Statement:
        """The loop body."""
    def children(self) -> List[QASMNode]: ...

class QuantumGate(SemanticStatement):
    """A gate call statement."""

    @property
    def name(self) -> Optional[str]:
        """The gate's name, when analysis resolved one."""
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
    @property
    def gate_name_span(self) -> Span:
        """The span covering the gate's name."""

class BranchingStatement(SemanticStatement):
    """An ``if`` statement."""

    @property
    def condition(self) -> Expression:
        """The branch condition."""
    @property
    def then_block(self) -> Statement:
        """The block run when the condition holds."""
    @property
    def else_block(self) -> Optional[Statement]:
        """The `else` block, when written."""
    def children(self) -> List[QASMNode]: ...

class IndexedClassicalAssignment(SemanticStatement):
    """An indexed assignment statement."""

    @property
    def lhs(self) -> Expression:
        """The base expression being assigned into."""
    @property
    def indices(self) -> List[Expression]:
        """The indices selecting the assigned element, outermost first."""
    @property
    def rhs(self) -> Expression:
        """The assigned value, cast to the element's type when needed."""
    def children(self) -> List[QASMNode]: ...

class InputDeclaration(SemanticStatement):
    """An input declaration statement."""

    @property
    def name(self) -> Optional[str]:
        """The input's name, when analysis resolved one."""
    @property
    def type(self) -> Type:
        """The declared, resolved type."""
    def children(self) -> List[QASMNode]: ...

class OutputDeclaration(SemanticStatement):
    """An output declaration statement."""

    @property
    def name(self) -> Optional[str]:
        """The output's name, when analysis resolved one."""
    @property
    def type(self) -> Type:
        """The declared, resolved type."""
    @property
    def init_expr(self) -> Expression:
        """The default value analysis assigned to the output."""
    def children(self) -> List[QASMNode]: ...
    @property
    def ty_span(self) -> Span:
        """The span covering the type as written in the source."""

class Pragma(SemanticStatement):
    """A pragma statement.

    ``command`` is authoritative; ``name`` and ``value`` are derived
    compatibility views.
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

class QuantumGateDefinition(SemanticStatement):
    """A quantum gate definition statement."""

    @property
    def name(self) -> Optional[str]:
        """The gate's name, when analysis resolved one."""
    @property
    def params(self) -> List[GateParameter]:
        """The classical parameters, in source order."""
    @property
    def qubits(self) -> List[GateParameter]:
        """The qubit parameters, in source order."""
    @property
    def body(self) -> List[Statement]:
        """The statements making up the gate body."""
    def children(self) -> List[QASMNode]: ...
    @property
    def name_span(self) -> Span:
        """The span covering the gate's name."""

class QubitDeclaration(SemanticStatement):
    """A qubit declaration statement."""

    @property
    def name(self) -> Optional[str]:
        """The qubit's name, when analysis resolved one."""
    def children(self) -> List[QASMNode]: ...

class QubitArrayDeclaration(SemanticStatement):
    """A qubit array declaration statement."""

    @property
    def name(self) -> Optional[str]:
        """The register's name, when analysis resolved one."""
    @property
    def size(self) -> Expression:
        """The register width."""
    def children(self) -> List[QASMNode]: ...
    @property
    def size_span(self) -> Span:
        """The span covering the width as written in the source."""

class QuantumReset(SemanticStatement):
    """A reset statement."""

    @property
    def qubits(self) -> List[Expression]:
        """The qubits being reset."""
    def children(self) -> List[QASMNode]: ...
    @property
    def reset_token_span(self) -> Span:
        """The span covering the `reset` keyword."""

class ReturnStatement(SemanticStatement):
    """A return statement."""

    @property
    def value(self) -> Optional[Expression]:
        """The returned expression, when the subroutine returns a value."""
    def children(self) -> List[QASMNode]: ...

class SwitchStatement(SemanticStatement):
    """A switch statement."""

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

class WhileLoop(SemanticStatement):
    """A ``while`` loop statement."""

    @property
    def condition(self) -> Expression:
        """The loop condition, tested before each iteration."""
    @property
    def body(self) -> Statement:
        """The loop body."""
    def children(self) -> List[QASMNode]: ...

class ErrorStatement(SemanticStatement):
    """A statement that could not be resolved."""

    def children(self) -> List[QASMNode]: ...

class AnalysisResult:
    """The result of a semantic :func:`analyze`."""

    @property
    def program(self) -> Program:
        """The root of the analyzed semantic program."""
    @property
    def document(self) -> SourceDocument:
        """The immutable source document for this analysis snapshot."""
    @property
    def symbols(self) -> SymbolTable:
        """The resolved symbol table produced during analysis."""
    @property
    def diagnostics(self) -> List[Diagnostic]:
        """All diagnostics (syntax and semantic errors) produced while analyzing."""
    @property
    def has_errors(self) -> bool:
        """Whether any errors were produced."""

def analyze(
    source: str,
    path: str = ...,
    includes: Optional[Union[Dict[str, str], Callable[[str], Optional[str]]]] = ...,
) -> AnalysisResult:
    """Parses and semantically analyzes `OpenQASM` source text."""
    ...
