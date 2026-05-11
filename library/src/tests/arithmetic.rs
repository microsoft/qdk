// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{test_expression, test_expression_with_lib};
use qsc::interpret::Value;

// Tests for Std.Arithmetic namespace

#[test]
fn check_maj() {
    test_expression(
        {
            "{
                import Std.Arithmetic.*;
                use q = Qubit[3];
                mutable r = [];
                for i in 0..7 {
                    ApplyXorInPlace(i, q);
                    MAJ(q[0],q[1],q[2]);
                    set r += [MeasureInteger(q)];
                    ResetAll(q);
                }
                r
            }"
        },
        &Value::Array(
            vec![
                Value::Int(0),
                Value::Int(1),
                Value::Int(2),
                Value::Int(7),
                Value::Int(3),
                Value::Int(6),
                Value::Int(5),
                Value::Int(4),
            ]
            .into(),
        ),
    );
}

#[test]
fn check_reflect_about_integer() {
    test_expression(
        {
            "{
                import Std.Arithmetic.*;
                import Std.Diagnostics.*;
                operation ManuallyReflectAboutFive(register : Qubit[]) : Unit is Adj + Ctl {
                    within {
                        X(register[1]);
                    } apply {
                        Controlled Z(register[0..1], register[2]);
                    }
                }
                CheckOperationsAreEqual(3,
                    ReflectAboutInteger(5, _),
                    ManuallyReflectAboutFive
                )
            }"
        },
        &Value::Bool(true),
    );
}

// ============================ Adders  ============================

//
// IncByLE
//

const INC_BY_LE_TEST_LIB: &str = include_str!("resources/src/inc_by_le.qs");

#[test]
fn check_inc_by_le_exhaustive_bitwidth_1() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check IncByLE\",
            Std.Arithmetic.IncByLE, 1)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_inc_by_le_exhaustive_bitwidth_2() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check IncByLE\",
            Std.Arithmetic.IncByLE, 2)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_inc_by_le_exhaustive_bitwidth_3() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check IncByLE\",
            Std.Arithmetic.IncByLE, 3)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_inc_by_le_general() {
    test_expression(
        {
            "{  // General cases for IncByLE
                import Std.Arithmetic.*;
                use x1 = Qubit[10];
                use y1 = Qubit[10];
                ApplyXorInPlace(279, x1);
                ApplyXorInPlace(383, y1);
                IncByLE(x1,y1); // 383 += 279
                let i = MeasureInteger(y1);
                ResetAll(x1+y1);

                return i;
        }"
        },
        &Value::Int(383 + 279),
    );
}

//
// RippleCarryTTKIncByLE
//

#[test]
fn check_ripple_carry_ttk_inc_by_le_exhaustive_bitwidth_1() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check RippleCarryTTKIncByLE\",
           Std.Arithmetic.RippleCarryTTKIncByLE, 1)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_ttk_inc_by_le_exhaustive_bitwidth_2() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check RippleCarryTTKIncByLE\",
            Std.Arithmetic.RippleCarryTTKIncByLE, 2)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_ttk_inc_by_le_exhaustive_bitwidth_3() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check RippleCarryTTKIncByLE\",
            Std.Arithmetic.RippleCarryTTKIncByLE, 3)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_ttk_inc_by_le_exhaustive_bitwidth_4() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check RippleCarryTTKIncByLE\",
            Std.Arithmetic.RippleCarryTTKIncByLE, 4)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_ttk_inc_by_le_general() {
    test_expression(
        {
            "{  // General cases for RippleCarryTTKIncByLE
                import Std.Arithmetic.*;

                use x1 = Qubit[10];
                use y1 = Qubit[10];
                ApplyXorInPlace(245, x1);
                ApplyXorInPlace(674, y1);
                RippleCarryTTKIncByLE(x1,y1); // 674 += 245
                let i = MeasureInteger(y1);
                ResetAll(x1+y1);
                return i;
        }"
        },
        &Value::Int(674 + 245),
    );
}

//
// RippleCarryCGIncByLE
//

#[test]
fn check_ripple_carry_cg_inc_by_le_exhaustive_bitwidth_1() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check RippleCarryCGIncByLE\",
            Std.Arithmetic.RippleCarryCGIncByLE, 1)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_cg_inc_by_le_exhaustive_bitwidth_2() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check RippleCarryCGIncByLE\",
            Std.Arithmetic.RippleCarryCGIncByLE, 2)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_cg_inc_by_le_exhaustive_bitwidth_3() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check RippleCarryCGIncByLE\",
            Std.Arithmetic.RippleCarryCGIncByLE, 3)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_cg_inc_by_le_exhaustive_bitwidth_4() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check RippleCarryCGIncByLE\",
            Std.Arithmetic.RippleCarryCGIncByLE, 4)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_inc_by_le_ctl_exhaustive_bitwidth_1() {
    test_expression_with_lib(
        "Test.TestIncByLECtl(\"Check RippleCarryCGIncByLE(Ctl)\",
            Std.Arithmetic.RippleCarryCGIncByLE, 1)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_inc_by_le_ctl_exhaustive_bitwidth_2() {
    test_expression_with_lib(
        "Test.TestIncByLECtl(\"Check RippleCarryCGIncByLE(Ctl)\",
            Std.Arithmetic.RippleCarryCGIncByLE, 2)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_inc_by_le_ctl_exhaustive_bitwidth_3() {
    test_expression_with_lib(
        "Test.TestIncByLECtl(\"Check RippleCarryCGIncByLE(Ctl)\",
            Std.Arithmetic.RippleCarryCGIncByLE, 3)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_inc_by_le_ctl_exhaustive_bitwidth_4() {
    test_expression_with_lib(
        "Test.TestIncByLECtl(\"Check RippleCarryCGIncByLE(Ctl)\",
            Std.Arithmetic.RippleCarryCGIncByLE, 4)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_cg_inc_by_le_general() {
    test_expression(
        {
            "{  // General cases for RippleCarryCGIncByLE
                import Std.Arithmetic.*;

                use x1 = Qubit[10];
                use y1 = Qubit[10];
                ApplyXorInPlace(743, x1);
                ApplyXorInPlace(112, y1);
                RippleCarryTTKIncByLE(x1,y1); // 112 += 743
                let i = MeasureInteger(y1);
                ResetAll(x1+y1);

                return i;
            }"
        },
        &Value::Int(112 + 743),
    );
}

//
// FourierTDIncByLE
//

#[test]
fn check_fourier_td_inc_by_le_exhaustive_bitwidth_1() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check FourierTDIncByLE\",
            Std.Arithmetic.FourierTDIncByLE, 1)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_fourier_td_inc_by_le_exhaustive_bitwidth_2() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check FourierTDIncByLE\",
            Std.Arithmetic.FourierTDIncByLE, 2)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_fourier_td_inc_by_le_exhaustive_bitwidth_3() {
    test_expression_with_lib(
        "Test.TestIncByLE(\"Check FourierTDIncByLE\",
            Std.Arithmetic.FourierTDIncByLE, 3)",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

//
// IncByLEUsingAddLE
//

#[test]
fn check_inc_by_le_using_add_le_exhaustive_bitwidth_1() {
    test_expression_with_lib(
        "{import Std.Arithmetic.*;
        Test.TestIncByLE2(\"Check IncByLEUsingAddLE\",
            IncByLEUsingAddLE(LookAheadDKRSAddLE,RippleCarryCGAddLE,_,_),
            1, 1)}",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_inc_by_le_using_add_le_exhaustive_bitwidth_2() {
    test_expression_with_lib(
        "{import Std.Arithmetic.*;
            Test.TestIncByLE2(\"Check IncByLEUsingAddLE\",
                IncByLEUsingAddLE(LookAheadDKRSAddLE,RippleCarryCGAddLE,_,_),
                2, 2)}",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_inc_by_le_using_add_le_exhaustive_bitwidth_3() {
    test_expression_with_lib(
        "{import Std.Arithmetic.*;
            Test.TestIncByLE2(\"Check IncByLEUsingAddLE\",
                IncByLEUsingAddLE(LookAheadDKRSAddLE,RippleCarryCGAddLE,_,_),
                3, 3)}",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_inc_by_le_using_add_le_exhaustive_bitwidth_4() {
    test_expression_with_lib(
        "{import Std.Arithmetic.*;
            Test.TestIncByLE2(\"Check IncByLEUsingAddLE\",
                IncByLEUsingAddLE(LookAheadDKRSAddLE,RippleCarryCGAddLE,_,_),
                4, 4)}",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_inc_by_le_using_add_le_ctl_exhaustive_bitwidth_1() {
    test_expression_with_lib(
        "{import Std.Arithmetic.*;
            Test.TestIncByLECtl2(\"Check IncByLEUsingAddLE(Ctl)\",
                IncByLEUsingAddLE(LookAheadDKRSAddLE,RippleCarryCGAddLE,_,_),
                1, 1)}",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_inc_by_le_using_add_le_ctl_exhaustive_bitwidth_2() {
    test_expression_with_lib(
        "{import Std.Arithmetic.*;
            Test.TestIncByLECtl2(\"Check IncByLEUsingAddLE(Ctl)\",
                IncByLEUsingAddLE(LookAheadDKRSAddLE,RippleCarryCGAddLE,_,_),
                2, 2)}",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_inc_by_le_using_add_le_ctl_exhaustive_bitwidth_3() {
    test_expression_with_lib(
        "{import Std.Arithmetic.*;
            Test.TestIncByLECtl2(\"Check IncByLEUsingAddLE(Ctl)\",
                IncByLEUsingAddLE(LookAheadDKRSAddLE,RippleCarryCGAddLE,_,_),
                3, 3)}",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_inc_by_le_using_add_le_ctl_exhaustive_bitwidth_4() {
    test_expression_with_lib(
        "{import Std.Arithmetic.*;
            Test.TestIncByLECtl2(\"Check IncByLEUsingAddLE(Ctl)\",
                IncByLEUsingAddLE(LookAheadDKRSAddLE,RippleCarryCGAddLE,_,_),
                4, 4)}",
        INC_BY_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_inc_by_le_using_add_le_general() {
    test_expression(
        {
            "{  // General cases for IncByLEUsingAddLE
                import Std.Arithmetic.*;

                use x1 = Qubit[10];
                use y1 = Qubit[10];
                ApplyXorInPlace(743, x1);
                ApplyXorInPlace(112, y1);
                IncByLEUsingAddLE(LookAheadDKRSAddLE,RippleCarryCGAddLE,x1,y1); // 112 += 743
                let i = MeasureInteger(y1);
                ResetAll(x1+y1);

                return i;
            }"
        },
        &Value::Int(112 + 743),
    );
}

//
// IncByI
//

#[test]
fn check_inc_by_i_general() {
    test_expression(
        {
            "{ // General cases for IncByI
                import Std.Arithmetic.*;

                use y0 = Qubit[1];
                IncByI(0,y0); // 0 += 0
                let i0 = MeasureInteger(y0);
                ResetAll(y0);

                use y1 = Qubit[1];
                IncByI(1,y1); // 0 += 1
                let i1 = MeasureInteger(y1);
                ResetAll(y1);

                use y2 = Qubit[1];
                X(y2[0]);
                IncByI(0,y2); // 1 += 0
                let i2 = MeasureInteger(y2);
                ResetAll(y2);

                use y3 = Qubit[1];
                X(y3[0]);
                IncByI(1,y3); // 1 += 1
                let i3 = MeasureInteger(y3);
                ResetAll(y3);

                use y4 = Qubit[20];
                ApplyXorInPlace(279, y4);
                IncByI(7895,y4); // 279 += 7895
                let i4 = MeasureInteger(y4);
                ResetAll(y4);

                return (i0, i1, i2, i3, i4);
        }"
        },
        &Value::Tuple(
            vec![
                Value::Int(0),
                Value::Int(1),
                Value::Int(1),
                Value::Int(0),
                Value::Int(279 + 7_895),
            ]
            .into(),
            None,
        ),
    );
}

//
// IncByIUsingIncByLE
//

#[test]
fn check_ripple_carry_cg_inc_by_i_general() {
    test_expression(
        {
            "{  // General cases for IncByIUsingIncByLE
                import Std.Arithmetic.*;

                use y0 = Qubit[10];
                ApplyXorInPlace(172, y0);
                IncByIUsingIncByLE(RippleCarryCGIncByLE, 128, y0);
                let i0 = MeasureInteger(y0);
                ResetAll(y0);

                use y1 = Qubit[10];
                ApplyXorInPlace(172, y1);
                IncByIUsingIncByLE(RippleCarryCGIncByLE, 0, y1);
                let i1 = MeasureInteger(y1);
                ResetAll(y1);

                use y2 = Qubit[10];
                ApplyXorInPlace(172, y2);
                IncByIUsingIncByLE(RippleCarryCGIncByLE, 14, y2);
                let i2 = MeasureInteger(y2);
                ResetAll(y2);

                return (i0, i1, i2);
            }"
        },
        &Value::Tuple(
            vec![Value::Int(300), Value::Int(172), Value::Int(186)].into(),
            None,
        ),
    );
}

//
// IncByL
//

#[test]
fn check_inc_by_l_general() {
    test_expression(
        {
            "{ // General cases for IncByL
                import Std.Arithmetic.*;

                use y0 = Qubit[1];
                IncByL(0L,y0); // 0 += 0
                let i0 = MeasureInteger(y0);
                ResetAll(y0);

                use y1 = Qubit[1];
                IncByL(1L,y1); // 0 += 1
                let i1 = MeasureInteger(y1);
                ResetAll(y1);

                use y2 = Qubit[1];
                X(y2[0]);
                IncByL(0L,y2); // 1 += 0
                let i2 = MeasureInteger(y2);
                ResetAll(y2);

                use y3 = Qubit[1];
                X(y3[0]);
                IncByL(1L,y3); // 1 += 1
                let i3 = MeasureInteger(y3);
                ResetAll(y3);

                use y4 = Qubit[20];
                ApplyXorInPlace(279, y4);
                IncByL(7895L,y4); // 279 += 7895
                let i4 = MeasureInteger(y4);
                ResetAll(y4);

                return (i0, i1, i2, i3, i4);
        }"
        },
        &Value::Tuple(
            vec![
                Value::Int(0),
                Value::Int(1),
                Value::Int(1),
                Value::Int(0),
                Value::Int(279 + 7_895),
            ]
            .into(),
            None,
        ),
    );
}

//
// IncByLUsingIncByLE
//

#[test]
fn check_ripple_carry_cg_inc_by_l_general() {
    test_expression(
        {
            "{  // Branching cases for IncByLUsingIncByLE
                import Std.Arithmetic.*;

                use y0 = Qubit[10];
                ApplyXorInPlace(172, y0);
                IncByLUsingIncByLE(RippleCarryCGIncByLE, 128L, y0);
                let i0 = MeasureInteger(y0);
                ResetAll(y0);

                use y1 = Qubit[10];
                ApplyXorInPlace(172, y1);
                IncByLUsingIncByLE(RippleCarryCGIncByLE, 0L, y1);
                let i1 = MeasureInteger(y1);
                ResetAll(y1);

                use y2 = Qubit[10];
                ApplyXorInPlace(172, y2);
                IncByLUsingIncByLE(RippleCarryCGIncByLE, 14L, y2);
                let i2 = MeasureInteger(y2);
                ResetAll(y2);

                return (i0, i1, i2);
            }"
        },
        &Value::Tuple(
            vec![Value::Int(300), Value::Int(172), Value::Int(186)].into(),
            None,
        ),
    );
}

//
// AddLE
//

const ADD_LE_TEST_LIB: &str = include_str!("resources/src/add_le.qs");

#[test]
fn check_add_le_exhaustive_bitwidth_1() {
    test_expression_with_lib(
        "Test.TestAddLE(\"Check AddLE\",
            Std.Arithmetic.AddLE, 1)",
        ADD_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_add_le_exhaustive_bitwidth_2() {
    test_expression_with_lib(
        "Test.TestAddLE(\"Check AddLE\",
            Std.Arithmetic.AddLE, 2)",
        ADD_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_add_le_exhaustive_bitwidth_3() {
    test_expression_with_lib(
        "Test.TestAddLE(\"Check AddLE\",
            Std.Arithmetic.AddLE, 3)",
        ADD_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_add_le_exhaustive_bitwidth_4() {
    test_expression_with_lib(
        "Test.TestAddLE(\"Check AddLE\",
            Std.Arithmetic.AddLE, 4)",
        ADD_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_add_le_general() {
    test_expression(
        {
            "{   // General cases for AddLE
                import Std.Arithmetic.*;

                use x1 = Qubit[10];
                use y1 = Qubit[10];
                use z1 = Qubit[10];
                ApplyXorInPlace(279, x1);
                ApplyXorInPlace(383, y1);
                AddLE(x1,y1,z1);  // z = 279 + 383
                let i1 = MeasureInteger(z1);
                ResetAll(x1);
                ResetAll(y1);
                ResetAll(z1);

                return i1;
        }"
        },
        &Value::Int(279 + 383),
    );
}

//
// RippleCarryCGAddLE
//

#[test]
fn check_ripple_carry_cg_add_le_exhaustive_bitwidth_1() {
    test_expression_with_lib(
        "Test.TestAddLE(\"Check RippleCarryCGAddLE\",
            Std.Arithmetic.RippleCarryCGAddLE, 1)",
        ADD_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_cg_add_le_exhaustive_bitwidth_2() {
    test_expression_with_lib(
        "Test.TestAddLE(\"Check RippleCarryCGAddLE\",
            Std.Arithmetic.RippleCarryCGAddLE, 2)",
        ADD_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_cg_add_le_exhaustive_bitwidth_3() {
    test_expression_with_lib(
        "Test.TestAddLE(\"Check RippleCarryCGAddLE\",
            Std.Arithmetic.RippleCarryCGAddLE, 3)",
        ADD_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_cg_add_le_exhaustive_bitwidth_4() {
    test_expression_with_lib(
        "Test.TestAddLE(\"Check RippleCarryCGAddLE\",
            Std.Arithmetic.RippleCarryCGAddLE, 4)",
        ADD_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_ripple_carry_cg_add_le_general() {
    test_expression(
        {
            "{  // General cases for RippleCarryAddLE
                import Std.Arithmetic.*;

                use x1 = Qubit[10];
                use y1 = Qubit[10];
                use z1 = Qubit[15];
                ApplyXorInPlace(978, x1);
                ApplyXorInPlace(456, y1);
                RippleCarryCGAddLE(x1,y1,z1);
                let i1 = MeasureInteger(z1);
                ResetAll(x1+y1+z1);

                return i1;
        }"
        },
        &Value::Int(978 + 456),
    );
}

//
// LookAheadDKRSAddLE
//

#[test]
fn check_lookahead_dkrs_add_le_exhaustive_bitwidth_1() {
    test_expression_with_lib(
        "Test.TestAddLE(\"Check LookAheadDKRSAddLE\",
            Std.Arithmetic.LookAheadDKRSAddLE, 1)",
        ADD_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_lookahead_dkrs_add_le_exhaustive_bitwidth_2() {
    test_expression_with_lib(
        "Test.TestAddLE(\"Check LookAheadDKRSAddLE\",
            Std.Arithmetic.LookAheadDKRSAddLE, 2)",
        ADD_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_lookahead_dkrs_add_le_exhaustive_bitwidth_3() {
    test_expression_with_lib(
        "Test.TestAddLE(\"Check LookAheadDKRSAddLE\",
            Std.Arithmetic.LookAheadDKRSAddLE, 3)",
        ADD_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_lookahead_dkrs_add_le_exhaustive_bitwidth_4() {
    test_expression_with_lib(
        "Test.TestAddLE(\"Check LookAheadDKRSAddLE\",
            Std.Arithmetic.LookAheadDKRSAddLE, 4)",
        ADD_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_lookahead_dkrs_add_le_general() {
    test_expression(
        {
            "{  // General cases for LookAheadDKRSAddLE
                import Std.Arithmetic.*;

                use x1 = Qubit[10];
                use y1 = Qubit[10];
                use z1 = Qubit[15];
                ApplyXorInPlace(939, x1);
                ApplyXorInPlace(578, y1);
                LookAheadDKRSAddLE(x1,y1,z1);
                let i1 = MeasureInteger(z1);
                ResetAll(x1+y1+z1);

                return i1;
        }"
        },
        &Value::Int(939 + 578),
    );
}

const COMPARE_TEST_LIB: &str = include_str!("resources/src/compare.qs");

#[test]
fn check_apply_if_less_l_exhaustive() {
    test_expression_with_lib(
        "Test.CompareWithBigInt(\"Check ApplyIfLessL\", 3,
            Std.Arithmetic.ApplyIfLessL(X,_,_,_),
            (a, b) -> {a < b} )",
        COMPARE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_apply_if_less_or_equal_l_exhaustive() {
    test_expression_with_lib(
        "Test.CompareWithBigInt(\"Check ApplyIfLessOrEqualL\", 3,
            Std.Arithmetic.ApplyIfLessOrEqualL(X,_,_,_),
            (a, b) -> {a <= b} )",
        COMPARE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_apply_if_equal_l_exhaustive() {
    test_expression_with_lib(
        "Test.CompareWithBigInt(\"Check ApplyIfEqualL\", 3,
            Std.Arithmetic.ApplyIfEqualL(X,_,_,_),
            (a, b) -> {a == b} )",
        COMPARE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_apply_if_greater_or_equal_l_exhaustive() {
    test_expression_with_lib(
        "Test.CompareWithBigInt(\"Check ApplyIfGreaterOrEqualL\", 3,
            Std.Arithmetic.ApplyIfGreaterOrEqualL(X,_,_,_),
            (a, b) -> {a >= b} )",
        COMPARE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_apply_if_greater_l_exhaustive() {
    test_expression_with_lib(
        "Test.CompareWithBigInt(\"Check ApplyIfGreaterL\", 3,
            Std.Arithmetic.ApplyIfGreaterL(X,_,_,_),
            (a, b) -> {a > b} )",
        COMPARE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_apply_if_less_le_exhaustive() {
    test_expression_with_lib(
        "Test.CompareWithLE(\"Check ApplyIfLessLE\", 3,
            Std.Arithmetic.ApplyIfLessLE(X,_,_,_),
            (a, b) -> {a < b} )",
        COMPARE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_apply_if_less_or_equal_le_exhaustive() {
    test_expression_with_lib(
        "Test.CompareWithLE(\"Check ApplyIfLessOrEqualLE\", 3,
            Std.Arithmetic.ApplyIfLessOrEqualLE(X,_,_,_),
            (a, b) -> {a <= b} )",
        COMPARE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_apply_if_equal_le_exhaustive() {
    test_expression_with_lib(
        "Test.CompareWithLE(\"Check ApplyIfEqualLE\", 3,
            Std.Arithmetic.ApplyIfEqualLE(X,_,_,_),
            (a, b) -> {a == b} )",
        COMPARE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_apply_if_greater_or_equal_le_exhaustive() {
    test_expression_with_lib(
        "Test.CompareWithLE(\"Check ApplyIfGreaterOrEqualLE\", 3,
            Std.Arithmetic.ApplyIfGreaterOrEqualLE(X,_,_,_),
            (a, b) -> {a >= b} )",
        COMPARE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_apply_if_greater_le_exhaustive() {
    test_expression_with_lib(
        "Test.CompareWithLE(\"Check ApplyIfGreaterLE\", 3,
            Std.Arithmetic.ApplyIfGreaterLE(X,_,_,_),
            (a, b) -> {a > b} )",
        COMPARE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_apply_if_less_l_non_x_action() {
    test_expression(
        "{
            import Std.Arithmetic.*;
            use input = Qubit[10];
            use output1 = Qubit[10];
            use output2 = Qubit[10];
            ApplyXorInPlace(569, input);
            ApplyXorInPlace(753, output1);
            ApplyXorInPlace(753, output2);
            ApplyIfGreaterL(IncByI(5, _), 572L, input, output1);
            ApplyIfLessL(IncByI(5, _), 572L, input, output2);
            let result1 = MeasureInteger(output1);
            let result2 = MeasureInteger(output2);
            ResetAll(input);
            ResetAll(output1);
            ResetAll(output2);
            (result1, result2)
        }",
        &Value::Tuple(vec![Value::Int(758), Value::Int(753)].into(), None),
    );
}
