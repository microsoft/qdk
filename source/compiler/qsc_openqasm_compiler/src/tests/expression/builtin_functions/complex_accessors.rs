// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::check_qasm_to_qsharp as check;
use expect_test::expect;

#[test]
fn real_and_imag_of_const_value_are_folded() {
    let source = "
        const complex value = 1.25 + 2.5 im;
        float re = real(value);
        float im = imag(value);
    ";

    check(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let value = Std.Math.Complex(1.25, 2.5);
        mutable re = 1.25;
        mutable im = 2.5;
    "#]],
    );
}

#[test]
fn real_and_imag_of_mutable_value_generate_field_accessors() {
    let source = "
        complex value = 1.25 + 2.5 im;
        float re = real(value);
        float im = imag(value);
    ";

    check(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable value = Std.Math.PlusC(Std.Math.Complex(1.25, 0.), Std.Math.Complex(0., 2.5));
        mutable re = value.Real;
        mutable im = value.Imag;
    "#]],
    );
}

#[test]
fn real_and_imag_of_reassigned_value_generate_field_accessors() {
    let source = "
        complex value = 1.25 + 2.5 im;
        value = 3.0 + 4.0 im;
        float sum = real(value) + imag(value);
    ";

    check(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable value = Std.Math.PlusC(Std.Math.Complex(1.25, 0.), Std.Math.Complex(0., 2.5));
        set value = Std.Math.PlusC(Std.Math.Complex(3., 0.), Std.Math.Complex(0., 4.));
        mutable sum = value.Real + value.Imag;
    "#]],
    );
}

#[test]
fn real_and_imag_of_sized_input_generate_field_accessors() {
    let source = "
        input complex[float[32]] value;
        float[32] re = real(value);
        float[32] im = imag(value);
    ";

    check(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable re = value.Real;
        mutable im = value.Imag;
    "#]],
    );
}

#[test]
fn real_and_imag_of_def_parameter_generate_field_accessors() {
    let source = "
        def re(complex value) -> float {
            return real(value);
        }
        def im(complex value) -> float {
            return imag(value);
        }
    ";

    check(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        function re(value : Std.Math.Complex) : Double {
            return value.Real;
        }
        function im(value : Std.Math.Complex) : Double {
            return value.Imag;
        }
    "#]],
    );
}

#[test]
fn real_and_imag_of_mutable_value_are_usable_in_conditions() {
    let source = "
        include \"stdgates.inc\";
        qubit q;
        complex value = 1.25 + 2.5 im;
        if (real(value) > imag(value)) {
            x q;
        }
    ";

    check(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        borrow q = Qubit();
        mutable value = Std.Math.PlusC(Std.Math.Complex(1.25, 0.), Std.Math.Complex(0., 2.5));
        if value.Real > value.Imag {
            x(q);
        };
    "#]],
    );
}
