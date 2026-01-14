// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! These unit tests check six properties for each target type.
//! Let's call the type we are casting from `T` and the type we are casting to `Q`.
//! We want to test that for each type `Q` we correctly:
//!   1. cast from T to Q.
//!   2. cast from T to Q[n].
//!   3. cast from T[n] to Q.
//!   4. cast from T[n] to Q[n].
//!   5. cast from T[n] to Q[m] when n > m; a truncating cast.
//!   6. cast from T[n] to Q[m] when n < m; an expanding cast.

use crate::tests::check_qasm_to_qsharp as check;
use expect_test::expect;

//===============
// Casts to bool
//===============

#[test]
fn complex_to_bool_fails() {
    let source = "
        complex a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type bool
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         bool(a);
               :         ^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_bool_fails() {
    let source = "
        complex[float[32]] a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type bool
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         bool(a);
               :         ^^^^^^^
             4 |     
               `----
        "#]],
    );
}

//===================
// Casts to duration
//===================

#[test]
fn complex_to_duration_fails() {
    let source = "
        complex a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type duration
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         duration(a);
               :         ^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_duration_fails() {
    let source = "
        complex[float[32]] a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type duration
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         duration(a);
               :         ^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

//=========================
// Casts to int and int[n]
//=========================

#[test]
fn complex_to_int_fails() {
    let source = "
        complex a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type int
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         int(a);
               :         ^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn complex_to_sized_int_fails() {
    let source = "
        complex a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type int[32]
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         int[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_int_fails() {
    let source = "
        complex[float[32]] a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type int
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         int(a);
               :         ^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_int_fails() {
    let source = "
        complex[float[32]] a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type int[32]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         int[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_int_truncating_fails() {
    let source = "
        complex[float[32]] a;
        int[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type int[16]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         int[16](a);
               :         ^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_int_expanding_fails() {
    let source = "
        complex[float[32]] a;
        int[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type int[64]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         int[64](a);
               :         ^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

//===========================
// Casts to uint and uint[n]
//===========================

#[test]
fn complex_to_uint_fails() {
    let source = "
        complex a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type uint
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         uint(a);
               :         ^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn complex_to_sized_uint_fails() {
    let source = "
        complex a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type uint[32]
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         uint[32](a);
               :         ^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_uint_fails() {
    let source = "
        complex[float[32]] a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type uint
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         uint(a);
               :         ^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_uint_fails() {
    let source = "
        complex[float[32]] a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type uint[32]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         uint[32](a);
               :         ^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_uint_truncating_fails() {
    let source = "
        complex[float[32]] a;
        uint[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type uint[16]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         uint[16](a);
               :         ^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_uint_expanding_fails() {
    let source = "
        complex[float[32]] a;
        uint[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type uint[64]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         uint[64](a);
               :         ^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

//=============================
// Casts to float and float[n]
//=============================

#[test]
fn complex_to_float_fails() {
    let source = "
        complex a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type float
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         float(a);
               :         ^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn complex_to_sized_float_fails() {
    let source = "
        complex a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type float[32]
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         float[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_float_fails() {
    let source = "
        complex[float[32]] a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type float
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         float(a);
               :         ^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_float_fails() {
    let source = "
        complex[float[32]] a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type float[32]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         float[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_float_truncating_fails() {
    let source = "
        complex[float[32]] a;
        float[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type float[16]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         float[16](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_float_expanding_fails() {
    let source = "
        complex[float[32]] a;
        float[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type float[64]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         float[64](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

//=============================
// Casts to angle and angle[n]
//=============================

#[test]
fn complex_to_angle_fails() {
    let source = "
        complex a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type angle
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn complex_to_sized_angle_fails() {
    let source = "
        complex a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type angle[32]
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         angle[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_angle_fails() {
    let source = "
        complex[float[32]] a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type angle
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_angle_fails() {
    let source = "
        complex[float[32]] a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type angle[32]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         angle[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_angle_truncating_fails() {
    let source = "
        complex[float[32]] a;
        angle[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type angle[16]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         angle[16](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_angle_expanding_fails() {
    let source = "
        complex[float[32]] a;
        angle[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type angle[64]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         angle[64](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

//=================================
// Casts to complex and complex[n]
//=================================

#[test]
fn complex_to_complex() {
    let source = "
        complex a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = Std.Math.Complex(0., 0.);
            a;
        "#]],
    );
}

#[test]
fn complex_to_sized_complex() {
    let source = "
        complex a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Compiler.Unimplemented

              x this statement is not yet handled during OpenQASM 3 import: cast complex
              | expressions
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         complex[float[32]](a);
               :         ^^^^^^^^^^^^^^^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x casting complex[float] to complex[float[32]] type are not supported
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         complex[float[32]](a);
               :         ^^^^^^^^^^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_complex() {
    let source = "
        complex[float[32]] a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Compiler.Unimplemented

              x this statement is not yet handled during OpenQASM 3 import: cast complex
              | expressions
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         complex(a);
               :         ^^^^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x casting complex[float[32]] to complex[float] type are not supported
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         complex(a);
               :         ^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_complex() {
    let source = "
        complex[float[32]] a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = Std.Math.Complex(0., 0.);
            a;
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_complex_truncating() {
    let source = "
        complex[float[32]] a;
        complex[float[16]](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = Std.Math.Complex(0., 0.);
            a;
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_complex_expanding() {
    let source = "
        complex[float[32]] a;
        complex[float[64]](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Compiler.Unimplemented

              x this statement is not yet handled during OpenQASM 3 import: cast complex
              | expressions
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         complex[float[64]](a);
               :         ^^^^^^^^^^^^^^^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x casting complex[float[32]] to complex[float[64]] type are not supported
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         complex[float[64]](a);
               :         ^^^^^^^^^^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

//=================================
// Casts to bit and bit[n]
//=================================

#[test]
fn complex_to_bit_fails() {
    let source = "
        complex a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type bit
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         bit(a);
               :         ^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn complex_to_bitarray_fails() {
    let source = "
        complex a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type bit[32]
               ,-[Test.qasm:3:9]
             2 |         complex a;
             3 |         bit[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_bit_fails() {
    let source = "
        complex[float[32]] a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type bit
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         bit(a);
               :         ^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_bitarray_fails() {
    let source = "
        complex[float[32]] a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type bit[32]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         bit[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_bitarray_truncating_fails() {
    let source = "
        complex[float[32]] a;
        bit[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type bit[16]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         bit[16](a);
               :         ^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_complex_to_bitarray_expanding_fails() {
    let source = "
        complex[float[32]] a;
        bit[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type bit[64]
               ,-[Test.qasm:3:9]
             2 |         complex[float[32]] a;
             3 |         bit[64](a);
               :         ^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}
