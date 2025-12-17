// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::check_qasm;
use expect_test::expect;

const SOURCE: &str = include_str!("../resources/openqasm_lowerer_errors_test.qasm");

#[allow(clippy::too_many_lines)]
#[test]
fn check_lowerer_error_spans_are_correct() {
    check_qasm(
        SOURCE,
        &expect![[r#"
              x Not Found: Could not resolve include file: missing_file
                ,-[Test.qasm:21:1]
             20 | // lowerer, so that we can construct the error with the right span.
             21 | include "missing_file";
                : ^^^^^^^^^^^^^^^^^^^^^^^
             22 | 
                `----

            Qasm.Lowerer.UnsupportedVersion

              x unsupported version: '4.0'
               ,-[Test.qasm:4:10]
             3 | // UnsupportedVersion
             4 | OPENQASM 4.0;
               :          ^^^
             5 | 
               `----

            Qasm.Lowerer.IncludeNotInGlobalScope

              x include stdgates.inc must be declared in global scope
                ,-[Test.qasm:16:5]
             15 | {
             16 |     include "stdgates.inc";
                :     ^^^^^^^^^^^^^^^^^^^^^^^
             17 | }
                `----

            Qasm.Lowerer.StdGateCalledButNotIncluded

              x A standard gate was called but not included
                ,-[Test.qasm:24:1]
             23 | // StdGateCalledButNotIncluded
             24 | x q;
                : ^^^^
             25 | 
                `----
              help: Did you mean to include the standard gate library 'stdgates.inc'?

            Qasm.Lowerer.RedefinedSymbol

              x redefined symbol: x
                ,-[Test.qasm:28:5]
             27 | include "stdgates.inc";
             28 | int x = 2;
                :     ^
             29 | 
                `----

            Qasm.Lowerer.RedefinedSymbol

              x redefined symbol: rxx
                ,-[Test.qasm:32:5]
             31 | rxx(2.0) q, q;
             32 | int rxx = 3;
                :     ^^^
             33 | 
                `----

            Qasm.Lowerer.UndefinedSymbol

              x undefined symbol: undefined_symbol
                ,-[Test.qasm:35:1]
             34 | // UndefinedSymbol
             35 | undefined_symbol;
                : ^^^^^^^^^^^^^^^^
             36 | 
                `----

            Qasm.Lowerer.InconsistentTypesInAlias

              x inconsistent types in alias expression: qubit[2], bit[5]
                ,-[Test.qasm:40:15]
             39 | bit[5] alias_component_2;
             40 | let alias_1 = alias_component_1 ++ alias_component_2;
                :               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             41 | 
                `----

            Qasm.Lowerer.InvalidTypeInAlias

              x invalid type in alias expression: int
                ,-[Test.qasm:45:36]
             44 | int alias_component_4;
             45 | let alias_2 = alias_component_3 ++ alias_component_4;
                :                                    ^^^^^^^^^^^^^^^^^
             46 | 
                `----
              help: aliases can only be applied to quantum bits and registers

            Qasm.Lowerer.CannotUpdateConstVariable

              x cannot update const variable const_variable
                ,-[Test.qasm:49:1]
             48 | const int const_variable = 1;
             49 | const_variable = 2;
                : ^^^^^^^^^^^^^^
             50 | 
                `----
              help: mutable variables must be declared without the keyword `const`

            Qasm.Lowerer.CannotUpdateConstVariable

              x cannot update const variable const_bitarray
                ,-[Test.qasm:53:1]
             52 | const bit[2] const_bitarray = "11";
             53 | const_bitarray[1] = 0;
                : ^^^^^^^^^^^^^^
             54 | 
                `----
              help: mutable variables must be declared without the keyword `const`

            Qasm.Lowerer.CannotUpdateConstVariable

              x cannot update const variable const_variable
                ,-[Test.qasm:56:1]
             55 | // CannotUpdateConstVariable in simple assign_op
             56 | const_variable += 2;
                : ^^^^^^^^^^^^^^
             57 | 
                `----
              help: mutable variables must be declared without the keyword `const`

            Qasm.Lowerer.CannotUpdateConstVariable

              x cannot update const variable const_bitarray
                ,-[Test.qasm:59:1]
             58 | // CannotUpdateConstVariable in indexed assign_op
             59 | const_bitarray[1] += 7;
                : ^^^^^^^^^^^^^^
             60 | 
                `----
              help: mutable variables must be declared without the keyword `const`

            Qasm.Lowerer.ExprMustBeConst

              x a captured variable must be a const expression
                ,-[Test.qasm:64:13]
             63 | def try_capture_non_const_global_variable() {
             64 |     int a = non_const_global_variable;
                :             ^^^^^^^^^^^^^^^^^^^^^^^^^
             65 | }
                `----

            Qasm.Lowerer.ArrayDeclarationTypeError

              x expected an array of size 2 but found one of size 3
                ,-[Test.qasm:68:51]
             67 | // ArrayDeclarationTypeError
             68 | array[int, 2] array_initialized_with_wrong_size = {1, 2, 3};
                :                                                   ^^^^^^^^^
             69 | 
                `----

            Qasm.Lowerer.ArrayDeclarationTypeError

              x expected an array of size 2 but found Int(2)
                ,-[Test.qasm:71:54]
             70 | // ArrayDeclarationTypeError
             71 | array[int, 2] array_initialized_with_wrong_literal = 2;
                :                                                      ^
             72 | 
                `----

            Qasm.Lowerer.CannotCastLiteral

              x cannot cast literal expression of type const int to type array[int, 2]
                ,-[Test.qasm:71:1]
             70 | // ArrayDeclarationTypeError
             71 | array[int, 2] array_initialized_with_wrong_literal = 2;
                : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             72 | 
                `----

            Qasm.Lowerer.NotSupported

              x string literals are not supported
                ,-[Test.qasm:74:1]
             73 | // NotSupported string literals
             74 | "string_literal";
                : ^^^^^^^^^^^^^^^^
             75 | 
                `----

            Qasm.Lowerer.TypeDoesNotSupportedUnaryNegation

              x unary negation is not allowed for instances of bool
                ,-[Test.qasm:79:2]
             78 | bool binary_negation_not_supported = true;
             79 | ~binary_negation_not_supported;
                :  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             80 | 
                `----

            Qasm.Lowerer.NotSupported

              x arrays with more than 7 dimensions are not supported
                ,-[Test.qasm:82:1]
             81 | // NotSupported arrays with more than 7 dimensions
             82 | array[int, 1, 2, 3, 1, 2, 3, 1, 2] array_with_more_than_7_dims;
                : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             83 | 
                `----

            Qasm.Lowerer.ClassicalStmtInBox

              x invalid classical statement in box
                ,-[Test.qasm:86:5]
             85 |     // ClassicalStmtInBox
             86 |     2;
                :     ^^
             87 | }
                `----

            Qasm.Lowerer.InvalidScope

              x break can only appear in loop scopes
                ,-[Test.qasm:90:1]
             89 | // InvalidScope break outside loop
             90 | break;
                : ^^^^^^
             91 | 
                `----

            Qasm.Lowerer.InvalidScope

              x continue can only appear in loop scopes
                ,-[Test.qasm:93:1]
             92 | // InvalidScope continue outside loop
             93 | continue;
                : ^^^^^^^^^
             94 | 
                `----

            Qasm.Lowerer.InvalidScope

              x return statements can only appear in subroutine scopes
                ,-[Test.qasm:96:1]
             95 | // InvalidScope return outside def
             96 | return;
                : ^^^^^^^
             97 | 
                `----

            Qasm.Lowerer.MissingTargetExpressionInReturnStmt

              x return statements on a non-void subroutine should have a target expression
                 ,-[Test.qasm:100:5]
              99 | def missing_target_in_return() -> int {
             100 |     return;
                 :     ^^^^^^^
             101 | }
                 `----

            Qasm.Lowerer.ReturningExpressionFromVoidSubroutine

              x cannot return an expression from a void subroutine
                 ,-[Test.qasm:105:12]
             104 | def returning_from_void_subroutine() {
             105 |     return 2;
                 :            ^
             106 | }
                 `----

            Qasm.Lowerer.ExprMustBeConst

              x const decl init expr must be a const expression
                 ,-[Test.qasm:116:23]
             115 | int non_const_val = 2;
             116 | const int const_val = non_const_val;
                 :                       ^^^^^^^^^^^^^
             117 | 
                 `----

              x array declarations are only allowed in global scope
                 ,-[Test.qasm:120:5]
             119 | {
             120 |     array[int, 1, 2] arr;
                 :     ^^^^^^^^^^^^^^^^^^^^^
             121 | }
                 `----

            Qasm.Lowerer.DefDeclarationInNonGlobalScope

              x def declarations must be done in global scope
                 ,-[Test.qasm:125:5]
             124 | {
             125 |     def f() {}
                 :     ^^^^^^^^^^
             126 | }
                 `----

            Qasm.Lowerer.GateDeclarationInNonGlobalScope

              x gate declarations must be done in global scope
                 ,-[Test.qasm:130:5]
             129 | {
             130 |     gate g q {}
                 :     ^^^^^^^^^^^
             131 | }
                 `----

            Qasm.Lowerer.QubitDeclarationInNonGlobalScope

              x qubit declarations must be done in global scope
                 ,-[Test.qasm:135:5]
             134 | {
             135 |     qubit non_global_qubit;
                 :     ^^^^^^^^^^^^^^^^^^^^^^^
             136 | }
                 `----

            Qasm.Lowerer.NonVoidDefShouldAlwaysReturn

              x non-void def should always return
                 ,-[Test.qasm:139:37]
             138 | // NonVoidDefShouldAlwaysReturn
             139 | def non_void_def_should_return() -> int {
                 :                                     ^^^
             140 | 
                 `----

            Qasm.Lowerer.ExternDeclarationInNonGlobalScope

              x extern declarations must be done in global scope
                 ,-[Test.qasm:149:5]
             148 | {
             149 |     extern f(int);
                 :     ^^^^^^^^^^^^^^
             150 | }
                 `----

            Qasm.Lowerer.InvalidNumberOfClassicalArgs

              x gate expects 2 classical arguments, but 1 were provided
                 ,-[Test.qasm:154:1]
             153 | def invalid_arity_call(int a, int b) {}
             154 | invalid_arity_call(2);
                 : ^^^^^^^^^^^^^^^^^^^^^
             155 | 
                 `----

            Qasm.Lowerer.GateCalledLikeFunc

              x gate called like function: gate(0, 1)
                 ,-[Test.qasm:157:1]
             156 | // CannotCallNonFunction
             157 | x(2);
                 : ^^^^
             158 | 
                 `----
              help: ensure that qubit arguments are provided to the gate call

            Qasm.Lowerer.InvalidNumberOfClassicalArgs

              x gate expects 1 classical arguments, but 2 were provided
                 ,-[Test.qasm:160:1]
             159 | // InvalidNumberOfClassicalArgs in gate call
             160 | rx(2.0, 3.0) q;
                 : ^^^^^^^^^^^^^^^
             161 | 
                 `----

            Qasm.Lowerer.InvalidNumberOfQubitArgs

              x gate expects 1 qubit arguments, but 2 were provided
                 ,-[Test.qasm:163:1]
             162 | // InvalidNumberOfQubitArgs
             163 | rx(2.0) q, q;
                 : ^^^^^^^^^^^^^
             164 | 
                 `----

            Qasm.Lowerer.BroadcastCallQuantumArgsDisagreeInSize

              x first quantum register is of type qubit[1] but found an argument of type
              | qubit[2]
                 ,-[Test.qasm:166:18]
             165 | // BroadcastCallQuantumArgsDisagreeInSize
             166 | ryy(2.0) qreg_1, qreg_2;
                 :                  ^^^^^^
             167 | 
                 `----

            Qasm.Lowerer.ExprMustFitInU32

              x ctrl modifier argument must fit in a u32
                 ,-[Test.qasm:174:6]
             173 | // ExprMustFitInU32
             174 | ctrl(5000000000) @ x q;
                 :      ^^^^^^^^^^
             175 | 
                 `----

            Qasm.Lowerer.CannotCastLiteral

              x cannot cast literal expression of type const float to type const uint
                 ,-[Test.qasm:181:12]
             180 | // ArraySizeMustBeNonNegativeConstExpr
             181 | array[int, 2.0] non_int_array_size;
                 :            ^^^
             182 | 
                 `----

            Qasm.Lowerer.ExprMustBeNonNegativeInt

              x array size must be a non-negative integer
                 ,-[Test.qasm:184:12]
             183 | // ArraySizeMustBeNonNegativeConstExpr
             184 | array[int, -2] negative_array_size;
                 :            ^^
             185 | 
                 `----

            Qasm.Lowerer.DesignatorTooLarge

              x designator is too large
                 ,-[Test.qasm:187:12]
             186 | // DesignatorTooLarge
             187 | array[int, 5000000000] arr_size_too_large;
                 :            ^^^^^^^^^^
             188 | 
                 `----

            Qasm.Lowerer.CannotCastLiteral

              x cannot cast literal expression of type const float to type const uint
                 ,-[Test.qasm:190:5]
             189 | // TypeWidthMustBePositiveIntConstExpr
             190 | int[2.0] non_int_width;
                 :     ^^^
             191 | 
                 `----

            Qasm.Lowerer.ExprMustBePositiveInt

              x type width must be a positive integer
                 ,-[Test.qasm:193:5]
             192 | // TypeWidthMustBePositiveIntConstExpr
             193 | int[0] zero_width;
                 :     ^
             194 | int[-2] negative_width;
                 `----

            Qasm.Lowerer.ExprMustBePositiveInt

              x type width must be a positive integer
                 ,-[Test.qasm:194:5]
             193 | int[0] zero_width;
             194 | int[-2] negative_width;
                 :     ^^
             195 | 
                 `----

            Qasm.Lowerer.DesignatorTooLarge

              x designator is too large
                 ,-[Test.qasm:197:5]
             196 | // DesignatorTooLarge
             197 | int[5000000000] width_too_large;
                 :     ^^^^^^^^^^
             198 | 
                 `----

            Qasm.Lowerer.TypeMaxWidthExceeded

              x float max width is 64 but 65 was provided
                 ,-[Test.qasm:200:1]
             199 | // TypeMaxWidthExceeded
             200 | float[65] float_width_too_large;
                 : ^^^^^^^^^
             201 | angle[65] angle_width_too_large;
                 `----

            Qasm.Lowerer.TypeMaxWidthExceeded

              x angle max width is 64 but 65 was provided
                 ,-[Test.qasm:201:1]
             200 | float[65] float_width_too_large;
             201 | angle[65] angle_width_too_large;
                 : ^^^^^^^^^
             202 | 
                 `----

            Qasm.Lowerer.CannotCastLiteral

              x cannot cast literal expression of type const float to type int
                 ,-[Test.qasm:204:1]
             203 | // Invalid literal cast in cast_expr_with_target_type_or_default(...)
             204 | int invalid_lit_cast = 2.0;
                 : ^^^^^^^^^^^^^^^^^^^^^^^^^^^
             205 | 
                 `----

            Qasm.Lowerer.QuantumTypesInBinaryExpression

              x quantum typed values cannot be used in binary expressions
                 ,-[Test.qasm:213:5]
             212 | // QuantumTypesInBinaryExpression
             213 | 1 + q;
                 :     ^
             214 | q + 1;
                 `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type qubit to type const float
                 ,-[Test.qasm:213:5]
             212 | // QuantumTypesInBinaryExpression
             213 | 1 + q;
                 :     ^
             214 | q + 1;
                 `----

            Qasm.Lowerer.QuantumTypesInBinaryExpression

              x quantum typed values cannot be used in binary expressions
                 ,-[Test.qasm:214:1]
             213 | 1 + q;
             214 | q + 1;
                 : ^
             215 | 
                 `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type qubit to type const float
                 ,-[Test.qasm:214:1]
             213 | 1 + q;
             214 | q + 1;
                 : ^
             215 | 
                 `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type angle to type float
                 ,-[Test.qasm:218:1]
             217 | angle uncastable_to_int = 2.0;
             218 | uncastable_to_int + 3;
                 : ^^^^^^^^^^^^^^^^^
             219 | 3 + uncastable_to_int;
                 `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type angle to type const float
                 ,-[Test.qasm:219:5]
             218 | uncastable_to_int + 3;
             219 | 3 + uncastable_to_int;
                 :     ^^^^^^^^^^^^^^^^^
             220 | 
                 `----

            Qasm.Lowerer.OperatorNotAllowedForComplexValues

              x the operator OrB is not allowed for complex values
                 ,-[Test.qasm:222:1]
             221 | // OperatorNotAllowedForComplexValues
             222 | (2 + 1im) | 3im;
                 : ^^^^^^^^^^^^^^^
             223 | 
                 `----

            Qasm.Lowerer.IndexSetOnlyAllowedInAliasStmt

              x index sets are only allowed in alias statements
                 ,-[Test.qasm:225:8]
             224 | // IndexSetOnlyAllowedInAliasStmt
             225 | qreg_2[{0, 1}];
                 :        ^^^^^^
             226 | 
                 `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type const angle to type const int
                 ,-[Test.qasm:229:13]
             228 | array[int, 5] range_error;
             229 | range_error[const_uncastable_to_int:2.2];
                 :             ^^^^^^^^^^^^^^^^^^^^^^^
             230 | 
                 `----

            Qasm.Lowerer.ZeroStepInRange

              x range step cannot be zero
                 ,-[Test.qasm:232:13]
             231 | // ZeroStepInRange
             232 | range_error[1:0:3];
                 :             ^^^^^
             233 | 
                 `----

            Qasm.Lowerer.ZeroSizeArrayAccess

              x zero size array access is not allowed
                 ,-[Test.qasm:236:1]
             235 | array[int, 2, 0, 3] zero_size_array;
             236 | zero_size_array[1];
                 : ^^^^^^^^^^^^^^^^^^
             237 | 
                 `----
              help: array size must be a positive integer const expression

            Qasm.Lowerer.CannotIndexType

              x cannot index variables of type bit
                 ,-[Test.qasm:240:15]
             239 | bit non_indexable;
             240 | non_indexable[1];
                 :               ^
             241 | 
                 `----

            Qasm.Lowerer.CannotIndexType

              x cannot index variables of type qubit
                 ,-[Test.qasm:243:11]
             242 | // TooManyIndices
             243 | qreg_1[1, 2];
                 :           ^
             244 | 
                 `----

            Qasm.Lowerer.UndefinedSymbol

              x undefined symbol: missing_symbol
                 ,-[Test.qasm:246:1]
             245 | // Missing symbol in lower_indexed_ident_expr(...)
             246 | missing_symbol[2];
                 : ^^^^^^^^^^^^^^
             247 | 
                 `----

            Qasm.Lowerer.CannotIndexType

              x cannot index variables of type unknown
                 ,-[Test.qasm:246:16]
             245 | // Missing symbol in lower_indexed_ident_expr(...)
             246 | missing_symbol[2];
                 :                ^
             247 | 
                 `----

            Qasm.Lowerer.EmptyIndexOperator

              x index operator must contain at least one index
                 ,-[Test.qasm:250:13]
             249 | bit[4] empty_index;
             250 | empty_index[];
                 :             ^
             251 | 
                 `----

            Qasm.Lowerer.CannotCallNonFunction

              x cannot call an expression that is not a function
                 ,-[Test.qasm:253:1]
             252 | // CannotCallNonFunction
             253 | empty_index();
                 : ^^^^^^^^^^^^^
             254 | 
                 `----

            Qasm.Lowerer.FuncCalledLikeGate

              x function called like gate: def (qubit) -> void
                 ,-[Test.qasm:256:5]
             255 | // FuncCalledLikeGate
             256 | def func_called_like_gate(qubit q) {}
                 :     ^^^^^^^^^^^^^^^^^^^^^
             257 | func_called_like_gate q;
                 `----
              help: function parameters must be in parentheses

            Qasm.Lowerer.GateCallMissingParams

              x gate call missing parameters: gate(0, 1)
                 ,-[Test.qasm:260:1]
             259 | // GateCallMissingParams
             260 | h;
                 : ^
             261 | 
                 `----
              help: ensure that any classical and quantum arguments are provided to the
                    gate call

            Qasm.Lowerer.FuncMissingParams

              x function call missing parameters: def (qubit) -> void
                 ,-[Test.qasm:263:1]
             262 | // FuncMissingParams
             263 | func_called_like_gate;
                 : ^^^^^^^^^^^^^^^^^^^^^
             264 | 
                 `----
              help: a function call must use parentheses, with any parameters inside
                    those parentheses.

            Qasm.Lowerer.ExternDeclarationCannotReturnStretch

              x extern declarations cannot return stretches
                 ,-[Test.qasm:266:52]
             265 | // ExternDeclarationCannotReturnStretch
             266 | extern extern_function_with_stretch_return(int) -> stretch;
                 :                                                    ^^^^^^^
             267 | 
                 `----

            Qasm.Lowerer.InvalidConcatenationPosition

              x concatenation expressions are not allowed in the rhs of assignment
              | operation statements
                 ,-[Test.qasm:272:18]
             271 | array[int, 6] concat_target;
             272 | concat_target += concat_operand_1 ++ concat_operand_2;
                 :                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             273 | 
                 `----

            Qasm.Lowerer.InvalidConcatenationPosition

              x concatenation expressions are not allowed in return statements
                 ,-[Test.qasm:275:12]
             274 | def return_concat(readonly array[int, 3] a) -> int {
             275 |     return a ++ a;
                 :            ^^^^^^
             276 | }
                 `----

            Qasm.Lowerer.NonVoidDefShouldAlwaysReturn

              x non-void def should always return
                 ,-[Test.qasm:274:48]
             273 | 
             274 | def return_concat(readonly array[int, 3] a) -> int {
                 :                                                ^^^
             275 |     return a ++ a;
                 `----

            Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: array[int[8], 4],
              | array[int[16], 4]
                 ,-[Test.qasm:281:17]
             280 | array[int[16], 4] concat_operand_4;
             281 | concat_target = concat_operand_3 ++ concat_operand_4;
                 :                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             282 | 
                 `----

            Qasm.Lowerer.InvalidTypeInArrayConcatenation

              x invalid type in array concatenation expression: bit[8]
                 ,-[Test.qasm:286:17]
             285 | bit[8] concat_operand_6;
             286 | concat_target = concat_operand_5 ++ concat_operand_6;
                 :                 ^^^^^^^^^^^^^^^^
                 `----
              help: array concatenation can only be applied to arrays

            Qasm.Lowerer.InvalidTypeInArrayConcatenation

              x invalid type in array concatenation expression: bit[8]
                 ,-[Test.qasm:286:37]
             285 | bit[8] concat_operand_6;
             286 | concat_target = concat_operand_5 ++ concat_operand_6;
                 :                                     ^^^^^^^^^^^^^^^^
                 `----
              help: array concatenation can only be applied to arrays
        "#]],
    );
}
