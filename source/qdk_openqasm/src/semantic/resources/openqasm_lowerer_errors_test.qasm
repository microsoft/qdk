// If there is a squiggle at the beginning of the file, it means some of the spans is incorrect.

// UnsupportedVersion
OPENQASM 4.0;

// Unimplemented pragma
pragma my_pragma;

// Not an error. Utility qubits to be used in the rest of the file.
qubit q;
qubit[1] qreg_1;
qubit[2] qreg_2;

// IncludeNotInGlobalScope
{
    include "stdgates.inc";
}

// This error is currently in the resolver. We should move it to the
// lowerer, so that we can construct the error with the right span.
include "missing_file";

// StdGateCalledButNotIncluded
x q;

// RedefinedSymbol in define_stdgates(...)
include "stdgates.inc";
int x = 2;

// RedefinedSymbol in define_qiskit_standard_gate(...)
rxx(2.0) q, q;
int rxx = 3;

// UndefinedSymbol
undefined_symbol;

// InconsistentTypesInAlias
qubit[2] alias_component_1;
bit[5] alias_component_2;
let alias_1 = alias_component_1 ++ alias_component_2;

// InvalidTypesInAlias
bit[2] alias_component_3;
int alias_component_4;
let alias_2 = alias_component_3 ++ alias_component_4;

// CannotUpdateConstVariable in simple assign
const int const_variable = 1;
const_variable = 2;

// CannotUpdateConstVariable in indexed assign
const bit[2] const_bitarray = "11";
const_bitarray[1] = 0;

// CannotUpdateConstVariable in simple assign_op
const_variable += 2;

// CannotUpdateConstVariable in indexed assign_op
const_bitarray[1] += 7;

// ExprMustBeConst when capturing global variable
int non_const_global_variable = 1;
def try_capture_non_const_global_variable() {
    int a = non_const_global_variable;
}

// ArrayDeclarationTypeError
array[int, 2] array_initialized_with_wrong_size = {1, 2, 3};

// ArrayDeclarationTypeError
array[int, 2] array_initialized_with_wrong_literal = 2;

// NotSupported string literals
"string_literal";

// NOTE: this error should say "bitwise unary negation"
// TypeDoesNotSupportedUnaryNegation
bool binary_negation_not_supported = true;
~binary_negation_not_supported;

// NotSupported arrays with more than 7 dimensions
array[int, 1, 2, 3, 1, 2, 3, 1, 2] array_with_more_than_7_dims;

box [2ns] {
    // ClassicalStmtInBox
    2;
}

// InvalidScope break outside loop
break;

// InvalidScope continue outside loop
continue;

// InvalidScope return outside def
return;

// MissingTargetExpressionInReturnStmt
def missing_target_in_return() -> int {
    return;
}

// ReturningExpressionFromVoidSubroutine
def returning_from_void_subroutine() {
    return 2;
}

// Unimplemented defcal
defcal {}

// Unimplemented defcalgrammar
defcalgrammar "my_grammar";

// ExprMustBeConst const decl init expression
int non_const_val = 2;
const int const_val = non_const_val;

// ArrayDeclarationInNonGlobalScope
{
    array[int, 1, 2] arr;
}

// DefDeclarationInNonGlobalScope
{
    def f() {}
}

// GateDeclarationInNonGlobalScope
{
    gate g q {}
}

// QubitDeclarationInNonGlobalScope
{
    qubit non_global_qubit;
}

// NonVoidDefShouldAlwaysReturn
def non_void_def_should_return() -> int {

}

// Unimplemented delay
delay [2ns] q;

// ExternDeclarationInNonGlobalScope
// Unimplemented
{
    extern f(int);
}

// InvalidNumberOfClassicalArgs in subroutine call
def invalid_arity_call(int a, int b) {}
invalid_arity_call(2);

// CannotCallNonFunction
x(2);

// InvalidNumberOfClassicalArgs in gate call
rx(2.0, 3.0) q;

// InvalidNumberOfQubitArgs
rx(2.0) q, q;

// BroadcastCallQuantumArgsDisagreeInSize
ryy(2.0) qreg_1, qreg_2;

// NOTE: this currently panics.
// CannotCast modifier arg
const angle const_uncastable_to_int = 2.0;
// ctrl(const_uncastable_to_int) @ x q;

// ExprMustFitInU32
ctrl(5000000000) @ x q;

// NOTE: change version to 3.0 to test this
// NotSupported in version 3.0
switch (1) { case 1 {} }

// ArraySizeMustBeNonNegativeConstExpr
array[int, 2.0] non_int_array_size;

// ArraySizeMustBeNonNegativeConstExpr
array[int, -2] negative_array_size;

// DesignatorTooLarge
array[int, 5000000000] arr_size_too_large;

// TypeWidthMustBePositiveIntConstExpr
int[2.0] non_int_width;

// TypeWidthMustBePositiveIntConstExpr
int[0] zero_width;
int[-2] negative_width;

// DesignatorTooLarge
int[5000000000] width_too_large;

// TypeMaxWidthExceeded
float[65] float_width_too_large;
angle[65] angle_width_too_large;

// Invalid literal cast in cast_expr_with_target_type_or_default(...)
int invalid_lit_cast = 2.0;

// InvalidCastValueRange
const uint value_too_large = 1 << 65;
def const_eval_context() {
    uint n = value_too_large;
}

// QuantumTypesInBinaryExpression
1 + q;
q + 1;

// CannotCast bin_op
angle uncastable_to_int = 2.0;
uncastable_to_int + 3;
3 + uncastable_to_int;

// OperatorNotAllowedForComplexValues
(2 + 1im) | 3im;

// IndexSetOnlyAllowedInAliasStmt
qreg_2[{0, 1}];

// CannotCast range
array[int, 5] range_error;
range_error[const_uncastable_to_int:2.2];

// ZeroStepInRange
range_error[1:0:3];

// ZeroSizeArrayAccess
array[int, 2, 0, 3] zero_size_array;
zero_size_array[1];

// CannotIndexType
bit non_indexable;
non_indexable[1];

// TooManyIndices
qreg_1[1, 2];

// Missing symbol in lower_indexed_ident_expr(...)
missing_symbol[2];

// EmptyIndexOperator
bit[4] empty_index;
empty_index[];

// CannotCallNonFunction
empty_index();

// FuncCalledLikeGate
def func_called_like_gate(qubit q) {}
func_called_like_gate q;

// GateCallMissingParams
h;

// FuncMissingParams
func_called_like_gate;

// ExternDeclarationCannotReturnStretch
extern extern_function_with_stretch_return(int) -> stretch;

// InvalidConcatenationPosition
array[int, 3] concat_operand_1;
array[int, 4] concat_operand_2;
array[int, 6] concat_target;
concat_target += concat_operand_1 ++ concat_operand_2;

def return_concat(readonly array[int, 3] a) -> int {
    return a ++ a;
}

// InconsistentTypesInArrayConcatenation
array[int[8], 4] concat_operand_3;
array[int[16], 4] concat_operand_4;
concat_target = concat_operand_3 ++ concat_operand_4;

// InvalidTypeInArrayConcatenation
bit[8] concat_operand_5;
bit[8] concat_operand_6;
concat_target = concat_operand_5 ++ concat_operand_6;
