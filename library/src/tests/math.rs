// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::test_expression;
use core::f64::consts::E;
use num_bigint::BigInt;
use qsc::interpret::Value;
use std::{f64::consts::PI, str::FromStr};

#[test]
fn check_pi() {
    test_expression(
        "Std.Math.PI()",
        &Value::Double(std::f64::consts::PI),
    );
}

#[test]
fn check_e() {
    test_expression(
        "Std.Math.E()",
        &Value::Double(std::f64::consts::E),
    );
}

#[test]
fn check_log_of_2() {
    test_expression(
        "Std.Math.LogOf2()",
        &Value::Double(std::f64::consts::LN_2),
    );
}

//
// Special numbers in IEEE floating-point representation
//

#[test]
fn check_is_nan() {
    test_expression(
        "Std.Math.IsNaN(0.0 / 0.0)",
        &Value::Bool(true),
    );
    test_expression("Std.Math.IsNaN(1.0)", &Value::Bool(false));
    test_expression(
        "Std.Math.IsNaN(Std.Math.ArcSin(2.0))",
        &Value::Bool(true),
    );
}

#[test]
fn check_is_infinite() {
    test_expression(
        "Std.Math.IsInfinite(1.0 / 0.0)",
        &Value::Bool(true),
    );
    test_expression(
        "Std.Math.IsInfinite(0.0 / 0.0)",
        &Value::Bool(false),
    );
    test_expression(
        "Std.Math.IsInfinite(-1.0 / 0.0)",
        &Value::Bool(true),
    );
}

//
// Sign, Abs, Min, Max, etc.
//

#[test]
fn check_sign_i() {
    test_expression("Std.Math.SignI(0)", &Value::Int(0));
    test_expression("Std.Math.SignI(1000)", &Value::Int(1));
    test_expression("Std.Math.SignI(-1000)", &Value::Int(-1));
}

#[test]
fn check_sign_d() {
    test_expression("Std.Math.SignD(0.0)", &Value::Int(0));
    test_expression("Std.Math.SignD(0.005)", &Value::Int(1));
    test_expression("Std.Math.SignD(-0.005)", &Value::Int(-1));
}

#[test]
fn check_sign_l() {
    test_expression("Std.Math.SignL(0L)", &Value::Int(0));
    test_expression(
        "Std.Math.SignL(9999999999999999999999999999999999999999L)",
        &Value::Int(1),
    );
    test_expression(
        "Std.Math.SignL(-9999999999999999999999999999999999999999L)",
        &Value::Int(-1),
    );
}

#[test]
fn check_abs_i() {
    test_expression("Std.Math.AbsI(0)", &Value::Int(0));
    test_expression("Std.Math.AbsI(1000)", &Value::Int(1000));
    test_expression("Std.Math.AbsI(-1000)", &Value::Int(1000));
    test_expression(
        "Std.Math.AbsI(-0x8000_0000_0000_0000)",
        &Value::Int(-0x8000_0000_0000_0000),
    );
}

#[test]
fn check_abs_d() {
    test_expression("Std.Math.AbsD(0.0)", &Value::Double(0.0));
    test_expression("Std.Math.AbsD(0.005)", &Value::Double(0.005));
    test_expression("Std.Math.AbsD(-0.005)", &Value::Double(0.005));
}

#[test]
fn check_abs_l() {
    test_expression(
        "Std.Math.AbsL(0L)",
        &Value::BigInt(BigInt::from(0)),
    );
    test_expression(
        "Std.Math.AbsL(9999L)",
        &Value::BigInt(BigInt::from(9999)),
    );
    test_expression(
        "Std.Math.AbsL(-9999L)",
        &Value::BigInt(BigInt::from(9999)),
    );
}

#[test]
fn check_max_i() {
    test_expression("Std.Math.MaxI(-5,7)", &Value::Int(7));
    test_expression("Std.Math.MaxI(-7,0)", &Value::Int(0));
}

#[test]
fn check_max_d() {
    test_expression("Std.Math.MaxD(-5.0,7.0)", &Value::Double(7.0));
    test_expression("Std.Math.MaxD(-7.0,0.0)", &Value::Double(0.0));
}

#[test]
fn check_max_l() {
    test_expression(
        "Std.Math.MaxL(-5L,7L)",
        &Value::BigInt(BigInt::from(7)),
    );
    test_expression(
        "Std.Math.MaxL(-7L,0L)",
        &Value::BigInt(BigInt::from(0)),
    );
}

#[test]
fn check_min_i() {
    test_expression("Std.Math.MinI(-5,7)", &Value::Int(-5));
    test_expression("Std.Math.MinI(-7,0)", &Value::Int(-7));
}

#[test]
fn check_min_d() {
    test_expression(
        "Std.Math.MinD(-5.0,7.0)",
        &Value::Double(-5.0),
    );
    test_expression(
        "Std.Math.MinD(-7.0,0.0)",
        &Value::Double(-7.0),
    );
}

#[test]
fn check_min_l() {
    test_expression(
        "Std.Math.MinL(-5L,7L)",
        &Value::BigInt(BigInt::from(-5)),
    );
    test_expression(
        "Std.Math.MinL(-7L,0L)",
        &Value::BigInt(BigInt::from(-7)),
    );
}

#[test]
fn check_min() {
    test_expression(
        "Std.Math.Min([-5, 7, 1, 10])",
        &Value::Int(-5),
    );
    test_expression("Std.Math.Min([5, 7, 1, 10])", &Value::Int(1));
    test_expression("Std.Math.Min([1])", &Value::Int(1));
}

#[test]
fn check_max() {
    test_expression(
        "Std.Math.Max([10, 7, 1, -20])",
        &Value::Int(10),
    );
    test_expression("Std.Math.Max([5, 7, 1, 20])", &Value::Int(20));
    test_expression("Std.Math.Max([1])", &Value::Int(1));
}

//
// Trigonometric functions
//

#[test]
fn check_arccos() {
    test_expression(
        "Std.Math.ArcCos(0.43)",
        &Value::Double(0.43_f64.acos()),
    );
}

#[test]
fn check_arcsin() {
    test_expression(
        "Std.Math.ArcSin(0.43)",
        &Value::Double(0.43_f64.asin()),
    );
}

#[test]
fn check_arctan() {
    test_expression(
        "Std.Math.ArcTan(43.43)",
        &Value::Double(43.43_f64.atan()),
    );
}

#[test]
fn check_arctan2() {
    test_expression(
        "Std.Math.ArcTan2(43.0,17.0)",
        &Value::Double(43.0_f64.atan2(17.0)),
    );
}

#[test]
fn check_cos() {
    test_expression(
        "Std.Math.Cos(1.11)",
        &Value::Double(1.11_f64.cos()),
    );
}

#[test]
fn check_cosh() {
    test_expression(
        "Std.Math.Cosh(1.11)",
        &Value::Double(1.11_f64.cosh()),
    );
}

#[test]
fn check_sin() {
    test_expression(
        "Std.Math.Sin(2.22)",
        &Value::Double(2.22_f64.sin()),
    );
}

#[test]
fn check_sinh() {
    test_expression(
        "Std.Math.Sinh(2.22)",
        &Value::Double(2.22_f64.sinh()),
    );
}

#[test]
fn check_tan() {
    test_expression(
        "Std.Math.Tan(1.23)",
        &Value::Double(1.23_f64.tan()),
    );
}

#[test]
fn check_tanh() {
    test_expression(
        "Std.Math.Tanh(1.23)",
        &Value::Double(1.23_f64.tanh()),
    );
}

#[test]
fn check_arccosh() {
    test_expression(
        "Std.Math.ArcCosh(1.234)",
        &Value::Double(1.234_f64.acosh()),
    );
}

#[test]
fn check_arcsinh() {
    test_expression(
        "Std.Math.ArcSinh(10.0)",
        &Value::Double(10.0_f64.asinh()),
    );
}

#[test]
fn check_arctanh() {
    test_expression(
        "Std.Math.ArcTanh(0.5)",
        &Value::Double(0.5_f64.atanh()),
    );
}

//
// Sqrt, Log, exp, etc.
//

#[test]
fn check_sqrt() {
    test_expression(
        "Std.Math.Sqrt(57121.0)",
        &Value::Double(239.0),
    );
}

#[test]
fn check_log() {
    test_expression(
        "Std.Math.Log(57121.0)",
        &Value::Double(57121.0_f64.ln()),
    );
}

#[test]
fn check_log10() {
    test_expression("Std.Math.Log10(1.0)", &Value::Double(0.0));
    test_expression("Std.Math.Log10(10.0)", &Value::Double(1.0));
}

#[test]
fn check_lg() {
    test_expression("Std.Math.Lg(1.0)", &Value::Double(0.0));
    test_expression("Std.Math.Lg(2.0)", &Value::Double(1.0));
}

//
// Truncation and Rounding
//

#[test]
fn check_truncate() {
    test_expression("Std.Math.Truncate(3.1)", &Value::Int(3));
    test_expression("Std.Math.Truncate(-3.7)", &Value::Int(-3));
}

#[test]
fn check_ceiling() {
    test_expression("Std.Math.Ceiling(3.1)", &Value::Int(4));
    test_expression("Std.Math.Ceiling(-3.7)", &Value::Int(-3));
}

#[test]
fn check_floor() {
    test_expression("Std.Math.Floor(3.7)", &Value::Int(3));
    test_expression("Std.Math.Floor(-3.1)", &Value::Int(-4));
}

#[test]
fn check_round() {
    test_expression("Std.Math.Round(0.5-1.0e-16)", &Value::Int(0));
    test_expression("Std.Math.Round(0.5)", &Value::Int(0));
    test_expression("Std.Math.Round(0.5+1.0e-16)", &Value::Int(1));
    test_expression("Std.Math.Round(3.1)", &Value::Int(3));
    test_expression("Std.Math.Round(-3.7)", &Value::Int(-4));
    test_expression("Std.Math.Round(-3.5)", &Value::Int(-3));
}

#[test]
fn check_round_away_from_zero() {
    test_expression(
        "Std.Math.RoundHalfAwayFromZero(0.5-1.0e-16)",
        &Value::Int(0),
    );
    test_expression("Std.Math.RoundHalfAwayFromZero(0.5)", &Value::Int(1));
    test_expression(
        "Std.Math.RoundHalfAwayFromZero(0.5+1.0e-16)",
        &Value::Int(1),
    );
    test_expression("Std.Math.RoundHalfAwayFromZero(3.1)", &Value::Int(3));
    test_expression("Std.Math.RoundHalfAwayFromZero(-3.7)", &Value::Int(-4));
    test_expression("Std.Math.RoundHalfAwayFromZero(-3.5)", &Value::Int(-4));
}

//
// Modular arithmetic
//

#[test]
fn check_divrem_i() {
    test_expression(
        "Std.Math.DivRemI(20, 3)",
        &Value::Tuple(vec![Value::Int(6), Value::Int(2)].into(), None),
    );
    test_expression(
        "Std.Math.DivRemI(-20, 3)",
        &Value::Tuple(vec![Value::Int(-6), Value::Int(-2)].into(), None),
    );
}

#[test]
fn check_divrem_l() {
    test_expression(
        "Std.Math.DivRemL(20L, 3L)",
        &Value::Tuple(
            vec![
                Value::BigInt(BigInt::from(6)),
                Value::BigInt(BigInt::from(2)),
            ]
            .into(),
            None,
        ),
    );
    test_expression(
        "Std.Math.DivRemL(-20L, 3L)",
        &Value::Tuple(
            vec![
                Value::BigInt(BigInt::from(-6)),
                Value::BigInt(BigInt::from(-2)),
            ]
            .into(),
            None,
        ),
    );
}

#[test]
fn check_modulus_i() {
    test_expression("Std.Math.ModulusI(20, 3)", &Value::Int(2));
    test_expression("Std.Math.ModulusI(-20, 3)", &Value::Int(1));
}

#[test]
fn check_modulus_l() {
    test_expression(
        "Std.Math.ModulusL(20L, 3L)",
        &Value::BigInt(BigInt::from(2)),
    );
    test_expression(
        "Std.Math.ModulusL(-20L, 3L)",
        &Value::BigInt(BigInt::from(1)),
    );
}

#[test]
fn check_exp_mod_i() {
    test_expression("Std.Math.ExpModI(1,10,10)", &Value::Int(1));
    test_expression("Std.Math.ExpModI(10,0,10)", &Value::Int(1));
    test_expression("Std.Math.ExpModI(2,10,10)", &Value::Int(4));
}

#[test]
fn check_exp_mod_l() {
    test_expression(
        "Std.Math.ExpModL(1L,10L,10L)",
        &Value::BigInt(BigInt::from(1)),
    );
    test_expression(
        "Std.Math.ExpModL(10L,0L,10L)",
        &Value::BigInt(BigInt::from(1)),
    );
    test_expression(
        "Std.Math.ExpModL(2L,10L,10L)",
        &Value::BigInt(BigInt::from(4)),
    );
}

#[test]
fn check_inverse_mod_i() {
    test_expression("Std.Math.InverseModI(2,5)", &Value::Int(3));
    test_expression("Std.Math.InverseModI(3,10)", &Value::Int(7));
    test_expression("Std.Math.InverseModI(-1,5)", &Value::Int(4));
}

#[test]
fn check_inverse_mod_l() {
    test_expression(
        "Std.Math.InverseModL(2L,5L)",
        &Value::BigInt(BigInt::from(3)),
    );
    test_expression(
        "Std.Math.InverseModL(3L,10L)",
        &Value::BigInt(BigInt::from(7)),
    );
    test_expression(
        "Std.Math.InverseModL(-1L,5L)",
        &Value::BigInt(BigInt::from(4)),
    );
}

//
// GCD, etc.
//
#[test]
fn check_gcd_i() {
    test_expression(
        "Std.Math.GreatestCommonDivisorI(0,0)",
        &Value::Int(0),
    );
    test_expression(
        "Std.Math.GreatestCommonDivisorI(2*3*5,2*3*7)",
        &Value::Int(2 * 3),
    );
    test_expression(
        "Std.Math.GreatestCommonDivisorI(39088169,63245986)",
        &Value::Int(1),
    );
}

#[test]
fn check_gcd_l() {
    test_expression(
        "Std.Math.GreatestCommonDivisorL(0L,0L)",
        &Value::BigInt(BigInt::from(0)),
    );
    test_expression(
        "Std.Math.GreatestCommonDivisorL(2L*3L*5L,2L*3L*7L)",
        &Value::BigInt(BigInt::from(2 * 3)),
    );
    test_expression(
        "Std.Math.GreatestCommonDivisorL(222232244629420445529739893461909967206666939096499764990979600L,359579325206583560961765665172189099052367214309267232255589801L)",
        &Value::BigInt(BigInt::from(1)),
    );
}

#[test]
fn check_is_coprime_i() {
    test_expression(
        "Std.Math.IsCoprimeI(44,35)",
        &Value::Bool(true),
    );
    test_expression(
        "Std.Math.IsCoprimeI(6,9)",
        &Value::Bool(false),
    );
    test_expression(
        "Std.Math.IsCoprimeI(1, -1)",
        &Value::Bool(true),
    );
}

#[test]
fn check_is_coprime_l() {
    test_expression(
        "Std.Math.IsCoprimeL(739696442014594807059393047166976L,7609583501588058567047119140625L)",
        &Value::Bool(true),
    );
    test_expression(
        "Std.Math.IsCoprimeL(6L,9L)",
        &Value::Bool(false),
    );
    test_expression(
        "Std.Math.IsCoprimeL(1L, -1L)",
        &Value::Bool(true),
    );
}

#[test]
fn check_cfc_i() {
    // NOTE: It is not important if the function returns -3/-4 or 3/4,
    // we can ignore this implementation details or update a function
    // to return canonical result.
    test_expression(
        "Std.Math.ContinuedFractionConvergentI((72,100), 2)",
        &Value::Tuple(vec![Value::Int(-1), Value::Int(-1)].into(), None),
    );
    test_expression(
        "Std.Math.ContinuedFractionConvergentI((72,100), 3)",
        &Value::Tuple(vec![Value::Int(2), Value::Int(3)].into(), None),
    );
    test_expression(
        "Std.Math.ContinuedFractionConvergentI((72,100), 4)",
        &Value::Tuple(vec![Value::Int(-3), Value::Int(-4)].into(), None),
    );
    test_expression(
        "Std.Math.ContinuedFractionConvergentI((72,100), 7)",
        &Value::Tuple(vec![Value::Int(5), Value::Int(7)].into(), None),
    );
    test_expression(
        "Std.Math.ContinuedFractionConvergentI((72,100), 25)",
        &Value::Tuple(vec![Value::Int(-18), Value::Int(-25)].into(), None),
    );
}

#[test]
fn check_cfc_l() {
    // NOTE: It is not important if the function returns -3/-4 or 3/4,
    // we can ignore this implementation details or update a function
    // to return canonical result.
    test_expression(
        "Std.Math.ContinuedFractionConvergentL((72L,100L), 2L)",
        &Value::Tuple(
            vec![
                Value::BigInt(BigInt::from(-1)),
                Value::BigInt(BigInt::from(-1)),
            ]
            .into(),
            None,
        ),
    );
    test_expression(
        "Std.Math.ContinuedFractionConvergentL((72L,100L), 3L)",
        &Value::Tuple(
            vec![
                Value::BigInt(BigInt::from(2)),
                Value::BigInt(BigInt::from(3)),
            ]
            .into(),
            None,
        ),
    );
    test_expression(
        "Std.Math.ContinuedFractionConvergentL((72L,100L), 25L)",
        &Value::Tuple(
            vec![
                Value::BigInt(BigInt::from(-18)),
                Value::BigInt(BigInt::from(-25)),
            ]
            .into(),
            None,
        ),
    );
    test_expression(
        "Std.Math.ContinuedFractionConvergentL((170141183460469231731687303715884105727L,331780596164137120496034969018767441441L), 2L)",
        &Value::Tuple(
            vec![
                Value::BigInt(BigInt::from(1)),
                Value::BigInt(BigInt::from(2)),
            ]
            .into(),
            None,
        ),
    );
    test_expression(
        "Std.Math.ContinuedFractionConvergentL((170141183460469231731687303715884105727L,331780596164137120496034969018767441441L), 1000000L)",
        &Value::Tuple(
            vec![
                Value::BigInt(BigInt::from(33_781)),
                Value::BigInt(BigInt::from(65_874)),
            ]
            .into(),
            None,
        ),
    );
}

#[test]
fn check_real_mod() {
    test_expression(
        "{ import Std.Math.*;
        RealMod(5.5 * PI(), 2.0 * PI(), 0.0) }",
        &Value::Double(1.5 * PI),
    );
    test_expression(
        "{ import Std.Math.*;
            RealMod(0.5 * PI(), 2.0 * PI(), -PI()/2.0) }",
        &Value::Double(0.5 * PI),
    );
    test_expression(
        "Std.Math.RealMod(10.5, 2.3, 1.2)",
        &Value::Double(1.3),
    );
    test_expression(
        "Std.Math.RealMod(3.6, 2.4, -1.2)",
        &Value::Double(-1.2),
    );
}

#[test]
fn check_bitsize_i() {
    test_expression("Std.Math.BitSizeI(0)", &Value::Int(0));
    test_expression("Std.Math.BitSizeI(1)", &Value::Int(1));
    test_expression("Std.Math.BitSizeI(2)", &Value::Int(2));
    test_expression("Std.Math.BitSizeI(3)", &Value::Int(2));
    test_expression(
        "Std.Math.BitSizeI(0x7FFFFFFFFFFFFFFF)",
        &Value::Int(63),
    );
}

#[test]
fn check_bitsize_l() {
    test_expression("Std.Math.BitSizeL(0L)", &Value::Int(0));
    test_expression("Std.Math.BitSizeL(1L)", &Value::Int(1));
    test_expression("Std.Math.BitSizeL(2L)", &Value::Int(2));
    test_expression("Std.Math.BitSizeL(3L)", &Value::Int(2));
    test_expression(
        "Std.Math.BitSizeL(0x7FFFFFFFFFFFFFFFL)",
        &Value::Int(63),
    );
    test_expression(
        "Std.Math.BitSizeL(0x8FFFFFFFFFFFFFFFL)",
        &Value::Int(64),
    );
}

#[test]
fn check_trailing_zero_count_i() {
    test_expression(
        "Std.Math.TrailingZeroCountI(7)",
        &Value::Int(0),
    );
    test_expression(
        "Std.Math.TrailingZeroCountI(2)",
        &Value::Int(1),
    );
    test_expression(
        "Std.Math.TrailingZeroCountI(7616)",
        &Value::Int(6),
    );
}

#[test]
fn check_trailing_zero_count_l() {
    test_expression(
        "Std.Math.TrailingZeroCountL(7L)",
        &Value::Int(0),
    );
    test_expression(
        "Std.Math.TrailingZeroCountL(2L)",
        &Value::Int(1),
    );
    test_expression(
        "Std.Math.TrailingZeroCountL(1L<<<163)",
        &Value::Int(163),
    );
}

#[test]
fn check_hamming_weight() {
    test_expression("Std.Math.HammingWeightI(2)", &Value::Int(1));
    test_expression("Std.Math.HammingWeightI(14)", &Value::Int(3));
    test_expression(
        "Std.Math.HammingWeightI(1<<<5)",
        &Value::Int(1),
    );
}

//
// Combinatorics
//

#[test]
fn check_factorial_i() {
    test_expression("Std.Math.FactorialI(0)", &Value::Int(1));
    test_expression("Std.Math.FactorialI(1)", &Value::Int(1));
    test_expression("Std.Math.FactorialI(2)", &Value::Int(2));
    test_expression(
        "Std.Math.FactorialI(10)",
        &Value::Int(3_628_800),
    );
}

#[test]
fn check_factorial_l() {
    test_expression(
        "Std.Math.FactorialL(0)",
        &Value::BigInt(BigInt::from(1)),
    );
    test_expression(
        "Std.Math.FactorialL(1)",
        &Value::BigInt(BigInt::from(1)),
    );
    test_expression(
        "Std.Math.FactorialL(2)",
        &Value::BigInt(BigInt::from(2)),
    );
    test_expression(
        "Std.Math.FactorialL(10)",
        &Value::BigInt(BigInt::from(3_628_800)),
    );
    test_expression(
        "Std.Math.FactorialL(170)",
        &Value::BigInt(BigInt::from_str(
            "7257415615307998967396728211129263114716991681296451376543577798900561843401706157852350749242617459511490991237838520776666022565442753025328900773207510902400430280058295603966612599658257104398558294257568966313439612262571094946806711205568880457193340212661452800000000000000000000000000000000000000000"
        ).expect("Cannot parse static BigInt in Rust code."))
    );
}

#[test]
fn check_approximate_factorial() {
    test_expression(
        "Std.Math.ApproximateFactorial(0)",
        &Value::Double(1.0),
    );
    test_expression(
        "Std.Math.ApproximateFactorial(2)",
        &Value::Double(2.0),
    );
    // NOTE: Tests for larger numbers can be added
    // when approximate comparison is implemented.
}

#[test]
fn check_log_gamma_d() {
    test_expression(
        "Std.Math.LogGammaD(3.14)",
        &Value::Double(0.826_138_704_777_028),
    );
    test_expression(
        "Std.Math.LogGammaD(0.782)",
        &Value::Double(0.169_806_721_914_044),
    );
    test_expression(
        "Std.Math.LogGammaD(1234.567)",
        &Value::Double(7_551.027_809_984_276),
    );
}

#[test]
fn check_log_factorial_d() {
    test_expression(
        "Std.Math.LogFactorialD(2000)",
        &Value::Double(13_206.524_350_513_8),
    );
    test_expression(
        "Std.Math.LogFactorialD(4321)",
        &Value::Double(31_856.241_848_248_7),
    );
}

#[test]
fn check_binom() {
    test_expression(
        "Std.Math.Binom(31, 7)",
        &Value::Int(2_629_575),
    );
    test_expression("Std.Math.Binom(23, 9)", &Value::Int(817_190));
    test_expression("Std.Math.Binom(13, 5)", &Value::Int(1_287));
    test_expression("Std.Math.Binom(4, 0)", &Value::Int(1));
    test_expression("Std.Math.Binom(4, 4)", &Value::Int(1));
}

#[test]
fn check_square_norm() {
    test_expression(
        "Std.Math.SquaredNorm([2.0])",
        &Value::Double(4.0),
    );
    test_expression(
        "Std.Math.SquaredNorm([-1.0, 1.0])",
        &Value::Double(2.0),
    );
    test_expression(
        "Std.Math.SquaredNorm([3.0, 4.0])",
        &Value::Double(25.0),
    );
}

#[test]
fn check_p_norm() {
    test_expression(
        "Std.Math.PNorm(1.0, [-0.1, 0.2, 0.3])",
        &Value::Double(0.6),
    );
    test_expression(
        "Std.Math.PNorm(1.5, [0.1, -0.2, 0.3])",
        &Value::Double(0.433_462_287_211_361),
    );
    test_expression(
        "Std.Math.PNorm(2.0, [0.1, 0.2, -0.3])",
        &Value::Double(0.374_165_738_677_394_17),
    );
    test_expression(
        "Std.Math.PNorm(3.0, [0.0, 0.0])",
        &Value::Double(0.0),
    );
}

#[test]
fn check_p_normalized() {
    test_expression(
        "Std.Math.PNormalized(1.0, [-0.1, 0.2, 0.5])",
        &Value::Array(
            vec![
                Value::Double(-0.125),
                Value::Double(0.25),
                Value::Double(0.625),
            ]
            .into(),
        ),
    );
    test_expression(
        "Std.Math.PNormalized(2.0, [3.0, 4.0])",
        &Value::Array(vec![Value::Double(0.6), Value::Double(0.8)].into()),
    );
    test_expression(
        "Std.Math.PNormalized(3.0, [0.0, 0.0])",
        &Value::Array(vec![Value::Double(0.0), Value::Double(0.0)].into()),
    );
}

//
// Complex numbers
//

#[test]
fn check_abs_squared_complex() {
    test_expression(
        "{
        import Std.Math.*;
        AbsSquaredComplex(Complex(1.0,1.0))}",
        &Value::Double(2.0),
    );
    test_expression(
        "{
        import Std.Math.*;
        AbsSquaredComplex(Complex(-3.0,4.0))}",
        &Value::Double(25.0),
    );
}

#[test]
fn check_abs_complex() {
    test_expression(
        "{
        import Std.Math.*;
        AbsComplex(Complex(1.0,1.0))}",
        &Value::Double(2.0_f64.sqrt()),
    );
    test_expression(
        "{
        import Std.Math.*;
        AbsComplex(Complex(-3.0,4.0))}",
        &Value::Double(5.0),
    );
}

#[test]
fn check_arg_complex() {
    test_expression(
        "{
        import Std.Math.*;
        ArgComplex(Complex(100.0,0.0))}",
        &Value::Double(0.0),
    );
    test_expression(
        "{
        import Std.Math.*;
        ArgComplex(Complex(1.0,1.0))}",
        &Value::Double(PI / 4.0),
    );
}

#[test]
fn check_abs_squared_complex_polar() {
    test_expression(
        "{
        import Std.Math.*;
        AbsSquaredComplexPolar(ComplexPolar(1.0,2.0))}",
        &Value::Double(1.0),
    );
    test_expression(
        "{
        import Std.Math.*;
        AbsSquaredComplexPolar(ComplexPolar(5.0,-1.0))}",
        &Value::Double(25.0),
    );
}

#[test]
fn check_abs_complex_polar() {
    test_expression(
        "{
        import Std.Math.*;
        AbsComplexPolar(ComplexPolar(1.0,2.0))}",
        &Value::Double(1.0),
    );
    test_expression(
        "{
        import Std.Math.*;
        AbsComplexPolar(ComplexPolar(5.0,-1.0))}",
        &Value::Double(5.0),
    );
}

#[test]
fn check_arg_complex_polar() {
    test_expression(
        "{
        import Std.Math.*;
        ArgComplexPolar(ComplexPolar(1.0,2.0))}",
        &Value::Double(2.0),
    );
    test_expression(
        "{
        import Std.Math.*;
        ArgComplexPolar(ComplexPolar(5.0,-1.0))}",
        &Value::Double(-1.0),
    );
}

#[test]
fn check_negation_c() {
    test_expression(
        "{
        import Std.Math.*;
        NegationC(Complex(1.0,2.0))}",
        &Value::Tuple(vec![Value::Double(-1.0), Value::Double(-2.0)].into(), None),
    );
    test_expression(
        "{
        import Std.Math.*;
        NegationC(Complex(5.0,-1.0))}",
        &Value::Tuple(vec![Value::Double(-5.0), Value::Double(1.0)].into(), None),
    );
}

#[test]
fn check_negation_cp() {
    test_expression(
        "{
        import Std.Math.*;
        NegationCP(ComplexPolar(1.0,0.0))}",
        &Value::Tuple(vec![Value::Double(1.0), Value::Double(PI)].into(), None),
    );
    test_expression(
        "{
        import Std.Math.*;
        NegationCP(ComplexPolar(5.0,-PI()/2.0))}",
        &Value::Tuple(
            vec![Value::Double(5.0), Value::Double(PI / 2.0)].into(),
            None,
        ),
    );
}

#[test]
fn check_plus_c() {
    test_expression(
        "{
        import Std.Math.*;
        PlusC(Complex(1.0,0.0), Complex(0.0,1.0))}",
        &Value::Tuple(vec![Value::Double(1.0), Value::Double(1.0)].into(), None),
    );
    test_expression(
        "{
        import Std.Math.*;
        PlusC(Complex(10.0,10.0), Complex(-10.0,10.0))}",
        &Value::Tuple(vec![Value::Double(0.0), Value::Double(20.0)].into(), None),
    );
}

#[test]
fn check_plus_cp() {
    test_expression(
        "{
        import Std.Math.*;
        PlusCP(ComplexPolar(1.0,0.0), ComplexPolar(1.0,PI()/2.0))}",
        &Value::Tuple(
            vec![Value::Double(2.0_f64.sqrt()), Value::Double(PI / 4.0)].into(),
            None,
        ),
    );
    test_expression(
        "{
        import Std.Math.*;
        PlusCP(ComplexPolar(10.0,PI()/4.0), ComplexPolar(10.0,3.0*PI()/4.0))}",
        &Value::Tuple(
            vec![Value::Double(200.0_f64.sqrt()), Value::Double(PI / 2.0)].into(),
            None,
        ),
    );
}

#[test]
fn check_minus_c() {
    test_expression(
        "{
        import Std.Math.*;
        MinusC(Complex(1.0,0.0), Complex(0.0,1.0))}",
        &Value::Tuple(vec![Value::Double(1.0), Value::Double(-1.0)].into(), None),
    );
    test_expression(
        "{
        import Std.Math.*;
        MinusC(Complex(10.0,10.0), Complex(-10.0,10.0))}",
        &Value::Tuple(vec![Value::Double(20.0), Value::Double(0.0)].into(), None),
    );
}

#[test]
fn check_minus_cp() {
    test_expression(
        "{
        import Std.Math.*;
        MinusCP(ComplexPolar(4.0,0.0), ComplexPolar(1.0,-PI()))}",
        &Value::Tuple(vec![Value::Double(5.0), Value::Double(0.0)].into(), None),
    );
}

#[test]
fn check_times_c() {
    test_expression(
        "{
        import Std.Math.*;
        TimesC(Complex(2.0,0.0), Complex(3.0,0.0))}",
        &Value::Tuple(vec![Value::Double(6.0), Value::Double(0.0)].into(), None),
    );
    test_expression(
        "{
        import Std.Math.*;
        TimesC(Complex(3.0,0.0), Complex(0.0,1.0))}",
        &Value::Tuple(vec![Value::Double(0.0), Value::Double(3.0)].into(), None),
    );
    test_expression(
        "{
        import Std.Math.*;
        TimesC(Complex(1.0,2.0), Complex(3.0,4.0))}",
        &Value::Tuple(vec![Value::Double(-5.0), Value::Double(10.0)].into(), None),
    );
}

#[test]
fn check_times_cp() {
    test_expression(
        "{
        import Std.Math.*;
        TimesCP(ComplexPolar(1.0,0.0), ComplexPolar(1.0,PI()/2.0))}",
        &Value::Tuple(
            vec![Value::Double(1.0), Value::Double(PI / 2.0)].into(),
            None,
        ),
    );
    test_expression(
        "{
        import Std.Math.*;
        TimesCP(ComplexPolar(1.0,PI()/4.0), ComplexPolar(2.0,3.0*PI()/4.0))}",
        &Value::Tuple(vec![Value::Double(2.0), Value::Double(PI)].into(), None),
    );
}

#[test]
fn check_pow_c() {
    test_expression(
        "{
        import Std.Math.*;
        PowC(Complex(2.0,0.0), Complex(3.0,0.0))}",
        &Value::Tuple(vec![Value::Double(8.0), Value::Double(0.0)].into(), None),
    );
    test_expression(
        "{
        import Std.Math.*;
        PowC(Complex(0.0,1.0), Complex(0.0,1.0))}",
        &Value::Tuple(
            vec![Value::Double(E.powf(-PI / 2.0)), Value::Double(0.0)].into(),
            None,
        ),
    );
}

#[test]
fn check_pow_cp() {
    test_expression(
        "{
        import Std.Math.*;
        PowCP(ComplexPolar(2.0,0.0), ComplexPolar(3.0,0.0))}",
        &Value::Tuple(vec![Value::Double(8.0), Value::Double(0.0)].into(), None),
    );
}

#[test]
fn check_divide_by_c() {
    test_expression(
        "{
        import Std.Math.*;
        DividedByC(Complex(1.0,0.0), Complex(2.0,0.0))}",
        &Value::Tuple(vec![Value::Double(0.5), Value::Double(0.0)].into(), None),
    );
    test_expression(
        "{
        import Std.Math.*;
        DividedByC(Complex(3.0,0.0), Complex(0.0,1.0))}",
        &Value::Tuple(vec![Value::Double(0.0), Value::Double(-3.0)].into(), None),
    );
    test_expression(
        "{
        import Std.Math.*;
        DividedByC(Complex(1.0,2.0), Complex(3.0,4.0))}",
        &Value::Tuple(vec![Value::Double(0.44), Value::Double(0.08)].into(), None),
    );
}

#[test]
fn check_devide_by_cp() {
    test_expression(
        "{
        import Std.Math.*;
        DividedByCP(ComplexPolar(1.0,0.0), ComplexPolar(1.0,PI()/2.0))}",
        &Value::Tuple(
            vec![Value::Double(1.0), Value::Double(-PI / 2.0)].into(),
            None,
        ),
    );
    test_expression(
        "{
        import Std.Math.*;
        DividedByCP(ComplexPolar(1.0,PI()/4.0), ComplexPolar(2.0,3.0*PI()/4.0))}",
        &Value::Tuple(
            vec![Value::Double(0.5), Value::Double(-PI / 2.0)].into(),
            None,
        ),
    );
}

//
// Fixed point
//

#[test]
fn check_smallest_fixed_point() {
    test_expression(
        "Std.Math.SmallestFixedPoint(1,0)",
        &Value::Double(-1.0),
    );
    test_expression(
        "Std.Math.SmallestFixedPoint(0,1)",
        &Value::Double(-0.5),
    );
    test_expression(
        "Std.Math.SmallestFixedPoint(10,5)",
        &Value::Double(-512.0),
    );
}

#[test]
fn check_largest_fixed_point() {
    test_expression(
        "Std.Math.LargestFixedPoint(1,0)",
        &Value::Double(0.0),
    );
    test_expression(
        "Std.Math.LargestFixedPoint(0,1)",
        &Value::Double(0.0),
    );
    test_expression(
        "Std.Math.LargestFixedPoint(10,5)",
        &Value::Double(511.96875),
    );
}
