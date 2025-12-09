// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Eq, Error, PartialEq)]
#[error(transparent)]
#[diagnostic(transparent)]
pub struct Error(pub SemanticErrorKind);

/// Represents the kind of semantic error that occurred during lowering of a QASM file(s).
/// For the most part, these errors are fatal and prevent compilation and are
/// safety checks to ensure that the QASM code is valid.
///
/// We can't use the semantics library for this:
///   - it is unsafe to use (heavy use of panic and unwrap)
///   - it is missing many language features
#[derive(Clone, Debug, Diagnostic, Eq, Error, PartialEq)]
pub enum SemanticErrorKind {
    #[error("array declarations are only allowed in global scope")]
    ArrayDeclarationInNonGlobalScope(#[label] Span),
    #[error("{0}")]
    #[diagnostic(code("Qasm.Lowerer.ArrayDeclarationTypeError"))]
    ArrayDeclarationTypeError(String, #[label] Span),
    #[error("annotation missing target statement")]
    #[diagnostic(code("Qasm.Lowerer.AnnotationWithoutStatement"))]
    AnnotationWithoutStatement(#[label] Span),
    #[error("array literals are only allowed in classical declarations")]
    #[diagnostic(code("Qasm.Lowerer.ArrayLiteralInNonClassicalDecl"))]
    ArrayLiteralInNonClassicalDecl(#[label] Span),
    #[error("first quantum register is of type {0} but found an argument of type {1}")]
    #[diagnostic(code("Qasm.Lowerer.BroadcastCallQuantumArgsDisagreeInSize"))]
    BroadcastCallQuantumArgsDisagreeInSize(String, String, #[label] Span),
    #[error("calibration statements are not supported: {0}")]
    #[diagnostic(code("Qasm.Lowerer.CalibrationsNotSupported"))]
    CalibrationsNotSupported(String, #[label] Span),
    #[error("cannot alias type {0}. Only qubit and qubit[] can be aliased")]
    #[diagnostic(code("Qasm.Lowerer.CannotAliasType"))]
    CannotAliasType(String, #[label] Span),
    #[error("cannot apply operator {0} to types {1} and {2}")]
    #[diagnostic(code("Qasm.Lowerer.CannotApplyOperatorToTypes"))]
    CannotApplyOperatorToTypes(String, String, String, #[label] Span),
    #[error("cannot assign a value of {0} type to a classical variable of {1} type")]
    #[diagnostic(code("Qasm.Lowerer.CannotAssignToType"))]
    CannotAssignToType(String, String, #[label] Span),
    #[error("cannot call an expression that is not a function")]
    #[diagnostic(code("Qasm.Lowerer.CannotCallNonFunction"))]
    CannotCallNonFunction(#[label] Span),
    #[error("cannot call a gate that is not a gate")]
    #[diagnostic(code("Qasm.Lowerer.CannotCallNonGate"))]
    CannotCallNonGate(#[label] Span),
    #[error("cannot cast expression of type {0} to type {1}")]
    #[diagnostic(code("Qasm.Lowerer.CannotCast"))]
    CannotCast(String, String, #[label] Span),
    #[error("cannot cast literal expression of type {0} to type {1}")]
    #[diagnostic(code("Qasm.Lowerer.CannotCastLiteral"))]
    CannotCastLiteral(String, String, #[label] Span),
    #[error("cannot index variables of type {0}")]
    #[diagnostic(code("Qasm.Lowerer.CannotIndexType"))]
    CannotIndexType(String, #[label] Span),
    #[error("cannot update const variable {0}")]
    #[diagnostic(help("mutable variables must be declared without the keyword `const`"))]
    #[diagnostic(code("Qasm.Lowerer.CannotUpdateConstVariable"))]
    CannotUpdateConstVariable(String, #[label] Span),
    #[error("cannot update readonly array reference {0}")]
    #[diagnostic(help("mutable array references must be declared with the keyword `mutable`"))]
    #[diagnostic(code("Qasm.Lowerer.CannotUpdateReadonlyArrayRef"))]
    CannotUpdateReadonlyArrayRef(String, #[label] Span),
    #[error("cannot cast expression of type {0} to type {1} as it would cause truncation")]
    #[diagnostic(code("Qasm.Lowerer.CastWouldCauseTruncation"))]
    CastWouldCauseTruncation(String, String, #[label] Span),
    #[error("invalid classical statement in box")]
    #[diagnostic(code("Qasm.Lowerer.ClassicalStmtInBox"))]
    ClassicalStmtInBox(#[label] Span),
    #[error("complex numbers in assignment binary expressions are not yet supported")]
    #[diagnostic(code("Qasm.Lowerer.ComplexBinaryAssignment"))]
    ComplexBinaryAssignment(#[label] Span),
    #[error("def declarations must be done in global scope")]
    #[diagnostic(code("Qasm.Lowerer.DefDeclarationInNonGlobalScope"))]
    DefDeclarationInNonGlobalScope(#[label] Span),
    #[error("designator must be a positive duration")]
    #[diagnostic(code("Qasm.Lowerer.DesignatorMustBePositiveDuration"))]
    DesignatorMustBePositiveDuration(#[label] Span),
    #[error("designator must be a positive literal integer")]
    #[diagnostic(code("Qasm.Lowerer.DesignatorMustBePositiveIntLiteral"))]
    DesignatorMustBePositiveIntLiteral(#[label] Span),
    #[error("designator is too large")]
    #[diagnostic(code("Qasm.Lowerer.DesignatorTooLarge"))]
    DesignatorTooLarge(#[label] Span),
    #[error("duration must be known at compile time")]
    #[diagnostic(code("Qasm.Lowerer.DurationMustBeKnownAtCompileTime"))]
    DurationMustBeKnownAtCompileTime(#[label] Span),
    #[error("index operator must contain at least one index")]
    #[diagnostic(code("Qasm.Lowerer.EmptyIndexOperator"))]
    EmptyIndexOperator(#[label] Span),
    #[error("{0} must be a const expression")]
    #[diagnostic(code("Qasm.Lowerer.ExprMustBeConst"))]
    ExprMustBeConst(String, #[label] Span),
    #[error("{0} must be an integer")]
    #[diagnostic(code("Qasm.Lowerer.ExprMustBeInt"))]
    ExprMustBeInt(String, #[label] Span),
    #[error("expression must be a duration")]
    #[diagnostic(code("Qasm.Lowerer.ExprMustBeDuration"))]
    ExprMustBeDuration(#[label] Span),
    #[error("{0} must be a non-negative integer")]
    #[diagnostic(code("Qasm.Lowerer.ExprMustBeNonNegativeInt"))]
    ExprMustBeNonNegativeInt(String, #[label] Span),
    #[error("{0} must be a positive integer")]
    #[diagnostic(code("Qasm.Lowerer.ExprMustBePositiveInt"))]
    ExprMustBePositiveInt(String, #[label] Span),
    #[error("{0} must fit in a u32")]
    #[diagnostic(code("Qasm.Lowerer.ExprMustFitInU32"))]
    ExprMustFitInU32(String, #[label] Span),
    #[error("extern declarations cannot return stretches")]
    #[diagnostic(code("Qasm.Lowerer.ExternDeclarationCannotReturnStretch"))]
    ExternDeclarationCannotReturnStretch(#[label] Span),
    #[error("extern declarations must be done in global scope")]
    #[diagnostic(code("Qasm.Lowerer.ExternDeclarationInNonGlobalScope"))]
    ExternDeclarationInNonGlobalScope(#[label] Span),
    #[error("failed to compile all expressions in expression list")]
    #[diagnostic(code("Qasm.Lowerer.FailedToCompileExpressionList"))]
    FailedToCompileExpressionList(#[label] Span),
    #[error("for iterable must have a set expression, range expression, or iterable expression")]
    #[diagnostic(code("Qasm.Lowerer.ForIterableInvalidExpression"))]
    ForIterableInvalidExpression(#[label] Span),
    #[error("for statements must have a body or statement")]
    #[diagnostic(code("Qasm.Lowerer.ForStatementsMustHaveABodyOrStatement"))]
    ForStatementsMustHaveABodyOrStatement(#[label] Span),
    #[error("function called like gate: {0}")]
    #[diagnostic(help("function parameters must be in parentheses"))]
    #[diagnostic(code("Qasm.Lowerer.FuncCalledLikeGate"))]
    FuncCalledLikeGate(String, #[label] Span),
    #[error("function call missing parameters: {0}")]
    #[diagnostic(help(
        "a function call must use parentheses, with any parameters inside those parentheses."
    ))]
    #[diagnostic(code("Qasm.Lowerer.FuncMissingParams"))]
    FuncMissingParams(String, #[label] Span),
    #[error("gate called like function: {0}")]
    #[diagnostic(help("ensure that qubit arguments are provided to the gate call"))]
    #[diagnostic(code("Qasm.Lowerer.GateCalledLikeFunc"))]
    GateCalledLikeFunc(String, #[label] Span),
    #[error("gate call missing parameters: {0}")]
    #[diagnostic(help(
        "ensure that any classical and quantum arguments are provided to the gate call"
    ))]
    #[diagnostic(code("Qasm.Lowerer.GateCallMissingParams"))]
    GateCallMissingParams(String, #[label] Span),
    #[error("gate declarations must be done in global scope")]
    #[diagnostic(code("Qasm.Lowerer.GateDeclarationInNonGlobalScope"))]
    GateDeclarationInNonGlobalScope(#[label] Span),
    #[error("if statement missing {0} expression")]
    #[diagnostic(code("Qasm.Lowerer.IfStmtMissingExpression"))]
    IfStmtMissingExpression(String, #[label] Span),
    #[error("include {0} must be declared in OPENQASM {1} programs")]
    #[diagnostic(code("Qasm.Lowerer.IncludeNotInLanguageVersion"))]
    IncludeNotInLanguageVersion(String, String, #[label] Span),
    #[error("include {0} could not be found")]
    #[diagnostic(code("Qasm.Lowerer.IncludeNotFound"))]
    IncludeNotFound(String, #[label] Span),
    #[error("include {0} must be declared in global scope")]
    #[diagnostic(code("Qasm.Lowerer.IncludeNotInGlobalScope"))]
    IncludeNotInGlobalScope(String, #[label] Span),
    #[error("include {0} must be declared in global scope")]
    #[diagnostic(code("Qasm.Lowerer.IncludeStatementMissingPath"))]
    IncludeStatementMissingPath(#[label] Span),
    #[error("inconsistent types in alias expression: {0}")]
    #[diagnostic(code("Qasm.Lowerer.InconsistentTypesInAlias"))]
    InconsistentTypesInAlias(String, #[label] Span),
    #[error("inconsistent types in array concatenation expression: {0}")]
    #[diagnostic(code("Qasm.Lowerer.InconsistentTypesInArrayConcatenation"))]
    InconsistentTypesInArrayConcatenation(String, #[label] Span),
    #[error("indexed must be a single expression")]
    #[diagnostic(code("Qasm.Lowerer.IndexMustBeSingleExpr"))]
    IndexMustBeSingleExpr(#[label] Span),
    #[error("index must be in the range [{0}, {1}] but it was {2}")]
    #[diagnostic(code("Qasm.Lowerer.IndexOutOfBounds"))]
    IndexOutOfBounds(i64, i64, i64, #[label] Span),
    #[error("index sets are only allowed in alias statements")]
    #[diagnostic(code("Qasm.Lowerer.IndexSetOnlyAllowedInAliasStmt"))]
    IndexSetOnlyAllowedInAliasStmt(#[label] Span),
    #[error("assigning {0} values to {1} must be in a range that can be converted to {1}")]
    #[diagnostic(code("Qasm.Lowerer.InvalidCastValueRange"))]
    InvalidCastValueRange(String, String, #[label] Span),
    #[error("concatenation expressions are not allowed in {0}")]
    #[diagnostic(code("Qasm.Lowerer.InvalidConcatenationPosition"))]
    InvalidConcatenationPosition(String, #[label] Span),
    #[error("gate operands other than qubits or qubit arrays are not supported")]
    #[diagnostic(code("Qasm.Lowerer.InvalidGateOperand"))]
    InvalidGateOperand(#[label] Span),
    #[error("control counts must be integer literals")]
    #[diagnostic(code("Qasm.Lowerer.InvalidControlCount"))]
    InvalidControlCount(#[label] Span),
    #[error("gate operands other than qubit arrays are not supported")]
    #[diagnostic(code("Qasm.Lowerer.InvalidIndexedGateOperand"))]
    InvalidIndexedGateOperand(#[label] Span),
    #[error("gate expects {0} classical arguments, but {1} were provided")]
    #[diagnostic(code("Qasm.Lowerer.InvalidNumberOfClassicalArgs"))]
    InvalidNumberOfClassicalArgs(usize, usize, #[label] Span),
    #[error("gate expects {0} qubit arguments, but {1} were provided")]
    #[diagnostic(code("Qasm.Lowerer.InvalidNumberOfQubitArgs"))]
    InvalidNumberOfQubitArgs(usize, usize, #[label] Span),
    #[error("{0} can only appear in {1} scopes")]
    #[diagnostic(code("Qasm.Lowerer.InvalidScope"))]
    InvalidScope(String, String, #[label] Span),
    #[error("invalid type in alias expression: {0}")]
    #[diagnostic(code("Qasm.Lowerer.InvalidTypeInAlias"))]
    #[diagnostic(help("aliases can only be applied to quantum bits and registers"))]
    InvalidTypeInAlias(String, #[label] Span),
    #[error("invalid type in array concatenation expression: {0}")]
    #[diagnostic(code("Qasm.Lowerer.InvalidTypeInArrayConcatenation"))]
    #[diagnostic(help("array concatenation can only be applied to arrays"))]
    InvalidTypeInArrayConcatenation(String, #[label] Span),
    #[error("measure statements must have a name")]
    #[diagnostic(code("Qasm.Lowerer.MeasureExpressionsMustHaveName"))]
    MeasureExpressionsMustHaveName(#[label] Span),
    #[error("measure statements must have a gate operand name")]
    #[diagnostic(code("Qasm.Lowerer.MeasureExpressionsMustHaveGateOperand"))]
    MeasureExpressionsMustHaveGateOperand(#[label] Span),
    #[error("return statements on a non-void subroutine should have a target expression")]
    #[diagnostic(code("Qasm.Lowerer.MissingTargetExpressionInReturnStmt"))]
    MissingTargetExpressionInReturnStmt(#[label] Span),
    #[error("control counts must be positive integers")]
    #[diagnostic(code("Qasm.Lowerer.NegativeControlCount"))]
    NegativeControlCount(#[label] Span),
    #[error("non-void def should always return")]
    #[diagnostic(code("Qasm.Lowerer.NonVoidDefShouldAlwaysReturn"))]
    NonVoidDefShouldAlwaysReturn(#[label] Span),
    #[error("{0} are not supported")]
    #[diagnostic(code("Qasm.Lowerer.NotSupported"))]
    NotSupported(String, #[label] Span),
    #[error("{0} were introduced in version {1}")]
    #[diagnostic(code("Qasm.Lowerer.NotSupportedInThisVersion"))]
    NotSupportedInThisVersion(String, String, #[label] Span),
    #[error("the operator {0} is not allowed for complex values")]
    #[diagnostic(code("Qasm.Lowerer.OperatorNotAllowedForComplexValues"))]
    OperatorNotAllowedForComplexValues(String, #[label] Span),
    #[error("pow gate modifiers must have an exponent")]
    #[diagnostic(code("Qasm.Lowerer.PowModifierMustHaveExponent"))]
    PowModifierMustHaveExponent(#[label] Span),
    #[error("qubit declarations must be done in global scope")]
    #[diagnostic(code("Qasm.Lowerer.QubitDeclarationInNonGlobalScope"))]
    QubitDeclarationInNonGlobalScope(#[label] Span),
    #[error("quantum typed values cannot be used in binary expressions")]
    #[diagnostic(code("Qasm.Lowerer.QuantumTypesInBinaryExpression"))]
    QuantumTypesInBinaryExpression(#[label] Span),
    #[error("range expressions must have a start when used in for loops")]
    #[diagnostic(code("Qasm.Lowerer.RangeExpressionsMustHaveStart"))]
    RangeExpressionsMustHaveStart(#[label] Span),
    #[error("range expressions must have a stop when used in for loops")]
    #[diagnostic(code("Qasm.Lowerer.RangeExpressionsMustHaveStop"))]
    RangeExpressionsMustHaveStop(#[label] Span),
    #[error("redefined builtin function: {0}")]
    #[help("builtin functions cannot be redefined, try choosing another name")]
    #[diagnostic(code("Qasm.Lowerer.RedefinedBuiltinFunction"))]
    RedefinedBuiltinFunction(String, #[label] Span),
    #[error("redefined symbol: {0}")]
    #[diagnostic(code("Qasm.Lowerer.RedefinedSymbol"))]
    RedefinedSymbol(String, #[label] Span),
    #[error("reset expression must have a gate operand")]
    #[diagnostic(code("Qasm.Lowerer.ResetExpressionMustHaveGateOperand"))]
    ResetExpressionMustHaveGateOperand(#[label] Span),
    #[error("reset expression must have a name")]
    #[diagnostic(code("Qasm.Lowerer.ResetExpressionMustHaveName"))]
    ResetExpressionMustHaveName(#[label] Span),
    #[error("cannot return an expression from a void subroutine")]
    #[diagnostic(code("Qasm.Lowerer.ReturningExpressionFromVoidSubroutine"))]
    ReturningExpressionFromVoidSubroutine(#[label] Span),
    #[error("return statements are only allowed within subroutines")]
    #[diagnostic(code("Qasm.Lowerer.ReturnNotInSubroutine"))]
    ReturnNotInSubroutine(#[label] Span),
    #[error("A standard gate was called but not included")]
    #[diagnostic(help("Did you mean to include the standard gate library '{0}'?"))]
    #[diagnostic(code("Qasm.Lowerer.StdGateCalledButNotIncluded"))]
    StdGateCalledButNotIncluded(String, #[label] Span),
    #[error("switch statement must have at least one non-default case")]
    #[diagnostic(code("Qasm.Lowerer.SwitchStatementMustHaveAtLeastOneCase"))]
    SwitchStatementMustHaveAtLeastOneCase(#[label] Span),
    #[error("too many controls specified")]
    #[diagnostic(code("Qasm.Lowerer.TooManyControls"))]
    TooManyControls(#[label] Span),
    #[error("too many indices specified")]
    #[diagnostic(code("Qasm.Lowerer.TooManyIndices"))]
    TooManyIndices(#[label] Span),
    #[error("bitwise not `~` is not allowed for instances of {0}")]
    #[diagnostic(code("Qasm.Lowerer.TypeDoesNotSupportBitwiseNot"))]
    TypeDoesNotSupportBitwiseNot(String, #[label] Span),
    #[error("unary negation is not allowed for instances of {0}")]
    #[diagnostic(code("Qasm.Lowerer.TypeDoesNotSupportedUnaryNegation"))]
    TypeDoesNotSupportedUnaryNegation(String, #[label] Span),
    #[error("{0} max width is {1} but {2} was provided")]
    #[diagnostic(code("Qasm.Lowerer.TypeMaxWidthExceeded"))]
    TypeMaxWidthExceeded(String, usize, usize, #[label] Span),
    #[error("types differ by dimensions and are incompatible")]
    #[diagnostic(code("Qasm.Lowerer.TypeRankError"))]
    TypeRankError(#[label] Span),
    #[error("undefined symbol: {0}")]
    #[diagnostic(code("Qasm.Lowerer.UndefinedSymbol"))]
    UndefinedSymbol(String, #[label] Span),
    #[error("unexpected parser error: {0}")]
    #[diagnostic(code("Qasm.Lowerer.UnexpectedParserError"))]
    UnexpectedParserError(String, #[label] Span),
    #[error("this statement is not yet handled during OpenQASM 3 import: {0}")]
    #[diagnostic(code("Qasm.Lowerer.Unimplemented"))]
    Unimplemented(String, #[label] Span),
    #[error("unsupported version: '{0}'")]
    #[diagnostic(code("Qasm.Lowerer.UnsupportedVersion"))]
    UnsupportedVersion(String, #[label] Span),
    #[error("while statement missing {0} expression")]
    #[diagnostic(code("Qasm.Lowerer.WhileStmtMissingExpression"))]
    WhileStmtMissingExpression(String, #[label] Span),
    #[error("zero size array access is not allowed")]
    #[diagnostic(code("Qasm.Lowerer.ZeroSizeArrayAccess"))]
    #[diagnostic(help("array size must be a positive integer const expression"))]
    ZeroSizeArrayAccess(#[label] Span),
    #[error("range step cannot be zero")]
    #[diagnostic(code("Qasm.Lowerer.ZeroStepInRange"))]
    ZeroStepInRange(#[label] Span),
}

impl From<Error> for crate::error::Error {
    fn from(val: Error) -> Self {
        crate::error::Error(crate::error::ErrorKind::Semantic(val))
    }
}
