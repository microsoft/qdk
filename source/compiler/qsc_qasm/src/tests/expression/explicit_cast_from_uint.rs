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
fn uint_to_bool() {
    let source = "
        uint a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            if a == 0 {
                false
            } else {
                true
            };
        "#]],
    );
}

#[test]
fn sized_uint_to_bool() {
    let source = "
        uint[32] a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            if a == 0 {
                false
            } else {
                true
            };
        "#]],
    );
}

//===================
// Casts to duration
//===================

#[test]
fn uint_to_duration_fails() {
    let source = "
        uint a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint to type duration
               ,-[Test.qasm:3:9]
             2 |         uint a;
             3 |         duration(a);
               :         ^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_uint_to_duration_fails() {
    let source = "
        uint[32] a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type duration
               ,-[Test.qasm:3:9]
             2 |         uint[32] a;
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
fn uint_to_int() {
    let source = "
        uint a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            a;
        "#]],
    );
}

#[test]
fn uint_to_sized_int() {
    let source = "
        uint a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            a;
        "#]],
    );
}

#[test]
fn sized_uint_to_int() {
    let source = "
        uint[32] a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            a;
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_int() {
    let source = "
        uint[32] a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            a;
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_int_truncating() {
    let source = "
        uint[32] a;
        int[16](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            a;
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_int_expanding() {
    let source = "
        uint[32] a;
        int[64](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            a;
        "#]],
    );
}

//===========================
// Casts to uint and uint[n]
//===========================

#[test]
fn uint_to_uint() {
    let source = "
        uint a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            a;
        "#]],
    );
}

#[test]
fn uint_to_sized_uint() {
    let source = "
        uint a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            a;
        "#]],
    );
}

#[test]
fn sized_uint_to_uint() {
    let source = "
        uint[32] a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            a;
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_uint() {
    let source = "
        uint[32] a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            a;
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_uint_truncating() {
    let source = "
        uint[32] a;
        uint[16](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            a;
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_uint_expanding() {
    let source = "
        uint[32] a;
        uint[64](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            a;
        "#]],
    );
}

//=============================
// Casts to float and float[n]
//=============================

#[test]
fn uint_to_float() {
    let source = "
        uint a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Convert.IntAsDouble(a);
        "#]],
    );
}

#[test]
fn uint_to_sized_float() {
    let source = "
        uint a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Convert.IntAsDouble(a);
        "#]],
    );
}

#[test]
fn sized_uint_to_float() {
    let source = "
        uint[32] a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Convert.IntAsDouble(a);
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_float() {
    let source = "
        uint[32] a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Convert.IntAsDouble(a);
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_float_truncating() {
    let source = "
        uint[32] a;
        float[16](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Convert.IntAsDouble(a);
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_float_expanding() {
    let source = "
        uint[32] a;
        float[64](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Convert.IntAsDouble(a);
        "#]],
    );
}

//=============================
// Casts to angle and angle[n]
//=============================

#[test]
fn uint_to_angle_fails() {
    let source = "
        uint a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint to type angle
               ,-[Test.qasm:3:9]
             2 |         uint a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn uint_to_sized_angle_fails() {
    let source = "
        uint a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint to type angle[32]
               ,-[Test.qasm:3:9]
             2 |         uint a;
             3 |         angle[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_uint_to_angle_fails() {
    let source = "
        uint[32] a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type angle
               ,-[Test.qasm:3:9]
             2 |         uint[32] a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_angle_fails() {
    let source = "
        uint[32] a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type angle[32]
               ,-[Test.qasm:3:9]
             2 |         uint[32] a;
             3 |         angle[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_angle_truncating_fails() {
    let source = "
        uint[32] a;
        angle[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type angle[16]
               ,-[Test.qasm:3:9]
             2 |         uint[32] a;
             3 |         angle[16](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_angle_expanding_fails() {
    let source = "
        uint[32] a;
        angle[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type angle[64]
               ,-[Test.qasm:3:9]
             2 |         uint[32] a;
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
fn uint_to_complex() {
    let source = "
        uint a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Math.Complex(Std.Convert.IntAsDouble(a), 0.);
        "#]],
    );
}

#[test]
fn uint_to_sized_complex() {
    let source = "
        uint a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Math.Complex(Std.Convert.IntAsDouble(a), 0.);
        "#]],
    );
}

#[test]
fn sized_uint_to_complex() {
    let source = "
        uint[32] a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Math.Complex(Std.Convert.IntAsDouble(a), 0.);
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_complex() {
    let source = "
        uint[32] a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Math.Complex(Std.Convert.IntAsDouble(a), 0.);
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_complex_truncating() {
    let source = "
        uint[32] a;
        complex[float[16]](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Math.Complex(Std.Convert.IntAsDouble(a), 0.);
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_complex_expanding() {
    let source = "
        uint[32] a;
        complex[float[64]](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Math.Complex(Std.Convert.IntAsDouble(a), 0.);
        "#]],
    );
}

//=================================
// Casts to bit and bit[n]
//=================================

#[test]
fn uint_to_bit() {
    let source = "
        uint a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Convert.IntAsResult(a);
        "#]],
    );
}

#[test]
fn uint_to_bitarray_fails() {
    let source = "
        uint a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint to type bit[32]
               ,-[Test.qasm:3:9]
             2 |         uint a;
             3 |         bit[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_uint_to_bit() {
    let source = "
        uint[32] a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.Convert.IntAsResult(a);
        "#]],
    );
}

#[test]
fn sized_uint_to_bitarray() {
    let source = "
        uint[32] a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.OpenQASM.Convert.IntAsResultArrayBE(a, 32);
        "#]],
    );
}

#[test]
fn sized_uint_to_bitarray_truncating_fails() {
    let source = "
        uint[32] a;
        bit[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type bit[16]
               ,-[Test.qasm:3:9]
             2 |         uint[32] a;
             3 |         bit[16](a);
               :         ^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sized_uint_to_bitarray_expanding_fails() {
    let source = "
        uint[32] a;
        bit[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type bit[64]
               ,-[Test.qasm:3:9]
             2 |         uint[32] a;
             3 |         bit[64](a);
               :         ^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}
