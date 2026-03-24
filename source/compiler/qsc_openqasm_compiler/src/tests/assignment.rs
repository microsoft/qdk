// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod alias;

use crate::tests::{check_qasm_to_qsharp, compile_fragments, fail_on_compilation_errors};
use expect_test::expect;
use miette::Report;

#[test]
fn classical() -> miette::Result<(), Vec<Report>> {
    let source = "
        bit[2] a;
        creg b[2];
        qubit[3] q;
        int[10] x = 12;
        a[0] = b[1];
        x += int[10](a[1]);
        measure q[1] -> a[0];
        a = measure q[1:2];
        measure q[0];
        b = a == 0;
    ";

    let unit = compile_fragments(source)?;
    fail_on_compilation_errors(&unit);
    Ok(())
}

#[test]
fn quantum() -> miette::Result<(), Vec<Report>> {
    let source = "
        bit[2] a;
        creg b[2];
        qubit[3] q;
        int[10] x = 12;
        a[0] = b[1];
        x += int[10](a[1]);
        measure q[1] -> a[0];
        a = measure q[1:2];
        measure q[0];
        b = a == 0;
    ";

    let unit = compile_fragments(source)?;
    fail_on_compilation_errors(&unit);
    Ok(())
}

#[test]
fn classical_old_style_decls() -> miette::Result<(), Vec<Report>> {
    let source = "
        bit[2] a;
        creg b[2];
        qubit[3] q;
        int[10] x = 12;
        a[0] = b[1];
        x += int[10](a[1]);
        measure q[1] -> a[0];
        a = measure q[1:2];
        measure q[0];
        b = a == 0;
    ";

    let unit = compile_fragments(source)?;
    fail_on_compilation_errors(&unit);
    Ok(())
}

#[test]
fn indexed_uint() {
    let source = r#"
        uint[4] a = 0b1111;
        a[1] = 0;
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 15;
            set a = {
                mutable bitarray = Std.OpenQASM.Convert.IntAsResultArrayBE(a, 4);
                set bitarray[2] = Std.OpenQASM.Convert.IntAsResult(0);
                Std.OpenQASM.Convert.ResultArrayAsIntBE(bitarray)
            };
        "#]],
    );
}

#[test]
fn indexed_uint_with_step() {
    let source = r#"
        uint[4] a = 0b1111;
        a[0:2:3] = "00";
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 15;
            set a = {
                mutable bitarray = Std.OpenQASM.Convert.IntAsResultArrayBE(a, 4);
                set bitarray[3..-2..0] = [Zero, Zero];
                Std.OpenQASM.Convert.ResultArrayAsIntBE(bitarray)
            };
        "#]],
    );
}

#[test]
fn indexed_angle() {
    let source = r#"
        angle[4] a = pi;
        a[1] = 0;
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI(), 4);
            set a = {
                mutable bitarray = Std.OpenQASM.Angle.AngleAsResultArrayBE(a);
                set bitarray[2] = Std.OpenQASM.Convert.IntAsResult(0);
                Std.OpenQASM.Angle.ResultArrayAsAngleBE(bitarray)
            };
        "#]],
    );
}

#[test]
fn indexed_angle_with_step() {
    let source = r#"
        angle[4] a = pi;
        a[0:2:3] = "00";
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI(), 4);
            set a = {
                mutable bitarray = Std.OpenQASM.Angle.AngleAsResultArrayBE(a);
                set bitarray[3..-2..0] = [Zero, Zero];
                Std.OpenQASM.Angle.ResultArrayAsAngleBE(bitarray)
            };
        "#]],
    );
}

#[test]
fn index_into_array_and_then_into_int() {
    let source = r#"
        array[int[4], 3] a = {1, 2, 3};
        a[1][1] = 1;
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [1, 2, 3];
            set a[1] = {
                mutable bitarray = Std.OpenQASM.Convert.IntAsResultArrayBE(a[1], 4);
                set bitarray[2] = Std.OpenQASM.Convert.IntAsResult(1);
                Std.OpenQASM.Convert.ResultArrayAsIntBE(bitarray)
            };
        "#]],
    );
}

#[test]
fn angle_shl_assign() {
    let source = "
        angle[8] a;
        a <<= 1;
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = new Std.OpenQASM.Angle.Angle {
                Value = 0,
                Size = 53
            };
            set a = Std.OpenQASM.Angle.__AngleShl__(a, 1);
        "#]],
    );
}

#[test]
fn angle_shr_assign() {
    let source = "
        angle[8] a;
        a >>= 1;
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = new Std.OpenQASM.Angle.Angle {
                Value = 0,
                Size = 53
            };
            set a = Std.OpenQASM.Angle.AngleShr(a, 1);
        "#]],
    );
}
