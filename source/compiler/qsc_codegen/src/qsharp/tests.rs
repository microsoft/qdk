// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

use expect_test::expect;
use indoc::indoc;

use super::test_utils::check;

#[test]
fn simple_entry_program_is_valid() {
    check(
        indoc! {r#"
            namespace Sample {
                @EntryPoint()
                operation Entry() : Result {
                    use q = Qubit();
                    H(q);
                    M(q)
                }
            }namespace Sample {}"#},
        None,
        &expect![[r#"
            namespace Sample {
                @EntryPoint()
                operation Entry() : Result {
                    use q = Qubit();
                    H(q);
                    M(q)
                }
            }
            namespace Sample {}"#]],
    );
}

#[test]
fn open() {
    check(
        indoc! {r#"
            namespace Sample {
                import Std.Intrinsic as sics;

                import Std.Diagnostics.*;
                import Std.Intrinsic as intrin;
                @EntryPoint()
                operation Entry() : Unit {
                }
            }"#},
        None,
        &expect![[r#"
            namespace Sample {
                import Std.Intrinsic as sics;
                import Std.Diagnostics.*;
                import Std.Intrinsic as intrin;
                @EntryPoint()
                operation Entry() : Unit {}
            }"#]],
    );
}

#[test]
fn newtype() {
    check(
        indoc! {r#"
            namespace Sample {
                newtype A = (First : Int, (Second : Double, Third : Bool));
                newtype B = (First : Result, Second : BigInt);
                newtype C = (Int, Bool);
                newtype D = (First : Int, Second: C);
                newtype E = (Real : Double, Imag : Double);
                newtype F = (Real : Double, Imaginary : Double, Bool);
                @EntryPoint()
                operation Entry() : Unit {
                }
            }"#},
        None,
        &expect![[r#"
            namespace Sample {
                newtype A = (First : Int, (Second : Double, Third : Bool));
                newtype B = (First : Result, Second : BigInt);
                newtype C = (Int, Bool);
                newtype D = (First : Int, Second : C);
                newtype E = (Real : Double, Imag : Double);
                newtype F = (Real : Double, Imaginary : Double, Bool);
                @EntryPoint()
                operation Entry() : Unit {}
            }"#]],
    );
}

#[test]
fn struct_decl() {
    check(
        indoc! {r#"
        namespace Sample {
            struct A {}
            struct B { Only : Int }
            struct C { First : Int, Second : Double, Third : Bool }
            struct D { First : Int, Second: B }
        }"#},
        None,
        &expect![[r#"
            namespace Sample {
                struct A {}
                struct B {
                    Only : Int
                }
                struct C {
                    First : Int,
                    Second : Double,
                    Third : Bool
                }
                struct D {
                    First : Int,
                    Second : B
                }
            }"#]],
    );
}

#[test]
fn struct_cons() {
    check(
        indoc! {r#"
        namespace Sample {
            struct A {}
            struct B { Only : Int }
            struct C { First : Int, Second : Double, Third : Bool }
            struct D { First : Int, Second: B }
            function Foo() : Unit {
                let a = new A {};
                let b = new B { Only = 1 };
                let c = new C { Third = true, First = 1, Second = 2.0 };
                let d = new D { First = 1, Second = new B { Only = 2 } };
            }
        }"#},
        None,
        &expect![[r#"
            namespace Sample {
                struct A {}
                struct B {
                    Only : Int
                }
                struct C {
                    First : Int,
                    Second : Double,
                    Third : Bool
                }
                struct D {
                    First : Int,
                    Second : B
                }
                function Foo() : Unit {
                    let a = new A {};
                    let b = new B {
                        Only = 1
                    };
                    let c = new C {
                        Third = true,
                        First = 1,
                        Second = 2.
                    };
                    let d = new D {
                        First = 1,
                        Second = new B {
                            Only = 2
                        }

                    };
                }
            }"#]],
    );
}

#[test]
fn struct_copy_cons() {
    check(
        indoc! {r#"
        namespace Sample {
            struct A { First : Int, Second : Double, Third : Bool }
            function Foo() : Unit {
                let a = new A { First = 1, Second = 2.0, Third = true };
                let b = new A { ...a };
                let c = new A { ...a, Second = 3.0 };
                let d = new A { ...a, Second = 3.0, Third = false };
            }
        }"#},
        None,
        &expect![[r#"
            namespace Sample {
                struct A {
                    First : Int,
                    Second : Double,
                    Third : Bool
                }
                function Foo() : Unit {
                    let a = new A {
                        First = 1,
                        Second = 2.,
                        Third = true
                    };
                    let b = new A {
                        ...a
                    };
                    let c = new A {
                        ...a,
                        Second = 3.
                    };
                    let d = new A {
                        ...a,
                        Second = 3.,
                        Third = false
                    };
                }
            }"#]],
    );
}

#[test]
fn statements() {
    check(
        indoc! {r#"
            namespace A {
                @EntryPoint()
                operation Entry() : Unit {
                    mutable x = 7;
                    let y = 5;
                    set x = y;
                    let z = [Zero, One];
                    mutable w = z;
                    let mask = [false, size = 10];

                    for i in Length(mask)-2 .. -1 .. 0 {
                        let nbPair = mask
                            w/ i     <- true
                            w/ i + 1 <- true;
                    }
                }
                function RichTrippleFor(func : Int[]) : Int[] {
                    mutable res = func;
                    for m in 0..(Length(func) - 1) {
                        mutable s = 1 <<< m >>> 2;
                        set s >>>= 2;
                        set s <<<= 2;
                        for i in 0..(2 * s)..Length(func) - 1 {
                            mutable k = i + s;
                            for j in i..i + s - 1 {
                                mutable t = res[j];
                                set res w/= j <- res[j] + res[k];
                                set res w/= k <- t - res[k];
                                set k = k + 1;
                            }
                        }
                    }
                    return res;
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                @EntryPoint()
                operation Entry() : Unit {
                    mutable x = 7;
                    let y = 5;
                    set x = y;
                    let z = [Zero, One];
                    mutable w = z;
                    let mask = [false, size = 10];
                    for i in Length(mask) - 2..-1..0 {
                        let nbPair = mask w/ i <- true w/ i + 1 <- true;
                    }
                }
                function RichTrippleFor(func : Int[]) : Int[] {
                    mutable res = func;
                    for m in 0..(Length(func) - 1) {
                        mutable s = 1 <<< m >>> 2;
                        set s >>>= 2;
                        set s <<<= 2;
                        for i in 0..(2 * s)..Length(func) - 1 {
                            mutable k = i + s;
                            for j in i..i + s - 1 {
                                mutable t = res[j];
                                set res w/= j <- res[j] + res[k];
                                set res w/= k <- t - res[k];
                                set k = k + 1;
                            }
                        }
                    }
                    return res;
                }
            }"#]],
    );
}

#[test]
fn qubits() {
    check(
        indoc! {r#"
            namespace A {
                operation B() : Unit {
                    use q = Qubit();
                    borrow q = Qubit();
                    use (q1, q2) = (Qubit(), Qubit());
                    borrow (q1, q2) = (Qubit(), Qubit());
                    use qubits = Qubit[2];
                    borrow qubits = Qubit[2];
                    let inputSize = 5;
                    use (control, target) = (Qubit[inputSize], Qubit[inputSize]);
                    borrow (control, target) = (Qubit[inputSize], Qubit[inputSize]);
                    use (q,) = (Qubit(),);
                    borrow (q,) = (Qubit(),);
                    use q = Qubit() {
                        X(q);
                        X(q);
                    }
                    borrow q = Qubit() {
                        X(q);
                        X(q);
                    }
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation B() : Unit {
                    use q = Qubit();
                    borrow q = Qubit();
                    use (q1, q2) = (Qubit(), Qubit());
                    borrow (q1, q2) = (Qubit(), Qubit());
                    use qubits = Qubit[2];
                    borrow qubits = Qubit[2];
                    let inputSize = 5;
                    use (control, target) = (Qubit[inputSize], Qubit[inputSize]);
                    borrow (control, target) = (Qubit[inputSize], Qubit[inputSize]);
                    use (q, ) = (Qubit(), );
                    borrow (q, ) = (Qubit(), );
                    use q = Qubit() {
                        X(q);
                        X(q);
                    }
                    borrow q = Qubit() {
                        X(q);
                        X(q);
                    }
                }
            }"#]],
    );
}

#[test]
fn boolean_ops() {
    check(
        indoc! {r#"
            namespace A {
                operation B() : Unit {
                    let a = true and false or true and (false or true);
                    let b = not a;
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation B() : Unit {
                    let a = true and false or true and (false or true);
                    let b = not a;
                }
            }"#]],
    );
}

#[test]
fn unary_ops() {
    check(
        indoc! {r#"
            namespace A {
                newtype Pair = (Int, Int);
                operation B() : Unit {
                    let a = -1;
                    let b = not false;
                    let c = +1;
                    let f = ~~~1;
                    let g = Pair(a, c);
                    let (h, i) = g!;
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                newtype Pair = (Int, Int);
                operation B() : Unit {
                    let a = -1;
                    let b = not false;
                    let c = + 1;
                    let f = ~~~1;
                    let g = Pair(a, c);
                    let (h, i) = g!;
                }
            }"#]],
    );
}

#[test]
fn binary_ops() {
    check(
        indoc! {r#"
            namespace A {
                operation B() : Unit {
                    let a = 1 + 2 - 3 * 4 / 5 % 6 ^ 7;
                    let b = (1 < 2);
                    let c =  a <= 3 and a > 4 and a >= 5 and a == 6 or a != 7;
                    let d = 1 &&& 2 ||| 3 ^^^ 4 <<< 5 >>> 6;
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation B() : Unit {
                    let a = 1 + 2 - 3 * 4 / 5 % 6^7;
                    let b = (1 < 2);
                    let c = a <= 3 and a > 4 and a >= 5 and a == 6 or a != 7;
                    let d = 1 &&& 2 ||| 3 ^^^ 4 <<< 5 >>> 6;
                }
            }"#]],
    );
}

#[test]
fn assign_update() {
    check(
        indoc! {r#"
            namespace A {
                operation B() : Unit {
                    mutable a = 1;
                    set a += 1;
                    set a &&&= a;
                    set a /= a;
                    set a /= a;
                    set a ^= a;
                    set a %= a;
                    set a *= a;
                    set a |||= a;
                    set a <<<= a;
                    set a >>>= a;
                    set a ^^^= a;
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation B() : Unit {
                    mutable a = 1;
                    set a += 1;
                    set a &&&= a;
                    set a /= a;
                    set a /= a;
                    set a ^= a;
                    set a %= a;
                    set a *= a;
                    set a |||= a;
                    set a <<<= a;
                    set a >>>= a;
                    set a ^^^= a;
                }
            }"#]],
    );
}

#[test]
fn lambda_fns() {
    check(
        indoc! {r#"
            namespace A {
                import Std.Arrays.*;
                operation B() : Unit {
                    let add = (x, y) -> x + y;
                    let intArray = [1, 2, 3, 4, 5];
                    let sum = Fold(add, 0, intArray);
                    let incremented = Mapped(x -> x + 1, intArray);

                    use control = Qubit();
                    let cnotOnControl = q => CNOT(control, q);
                    use q = Qubit();
                    cnotOnControl(q);
                    let incrementByOne = Add(_, 1);
                    let incrementByOneLambda = x -> Add(x, 1);
                    let five = incrementByOne(4);
                    let sumAndAddOne = AddMany(_, _, _, 1);
                    let sumAndAddOneLambda = (a, b, c) -> AddMany(a, b, c, 1);
                    let intArray = [1, 2, 3, 4, 5];
                    let incremented = Mapped(Add(_, 1), intArray);
                }
                function Add(x : Int, y : Int) : Int {
                    return x + y;
                }
                function AddMany(a : Int, b : Int, c : Int, d : Int) : Int {
                    return a + b + c + d;
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                import Std.Arrays.*;
                operation B() : Unit {
                    let add = (x, y) -> x + y;
                    let intArray = [1, 2, 3, 4, 5];
                    let sum = Fold(add, 0, intArray);
                    let incremented = Mapped(x -> x + 1, intArray);
                    use control = Qubit();
                    let cnotOnControl = q => CNOT(control, q);
                    use q = Qubit();
                    cnotOnControl(q);
                    let incrementByOne = Add(_, 1);
                    let incrementByOneLambda = x -> Add(x, 1);
                    let five = incrementByOne(4);
                    let sumAndAddOne = AddMany(_, _, _, 1);
                    let sumAndAddOneLambda = (a, b, c) -> AddMany(a, b, c, 1);
                    let intArray = [1, 2, 3, 4, 5];
                    let incremented = Mapped(Add(_, 1), intArray);
                }
                function Add(x : Int, y : Int) : Int {
                    return x + y;
                }
                function AddMany(a : Int, b : Int, c : Int, d : Int) : Int {
                    return a + b + c + d;
                }
            }"#]],
    );
}

#[test]
fn ranges() {
    check(
        indoc! {r#"
            namespace A {
                import Std.Arrays.*;
                operation B() : Unit {
                    let range = 1..3;
                    let range = 2..2..5;
                    let range = 2..2..6;
                    let range = 6..-2..2;
                    let range = 2..-2..2;
                    let range = 2..1;
                    mutable array = [];
                    for i in 0..10 {
                        set array += [i^2];
                    }
                    let newArray = array[0..2..10];
                    let newArray = array[...4];
                    let newArray = array[5...];
                    let newArray = array[2..3...];
                    let newArray = array[...3..7];
                    let newArray = array[...];
                    let newArray = array[...-3...];
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                import Std.Arrays.*;
                operation B() : Unit {
                    let range = 1..3;
                    let range = 2..2..5;
                    let range = 2..2..6;
                    let range = 6..-2..2;
                    let range = 2..-2..2;
                    let range = 2..1;
                    mutable array = [];
                    for i in 0..10 {
                        set array += [i^2];
                    }
                    let newArray = array[0..2..10];
                    let newArray = array[...4];
                    let newArray = array[5...];
                    let newArray = array[2..3...];
                    let newArray = array[...3..7];
                    let newArray = array[...];
                    let newArray = array[...-3...];
                }
            }"#]],
    );
}

#[test]
fn unary_functors() {
    check(
        indoc! {r#"
            namespace A {
                operation B() : Unit {
                    let v = q => H(q);
                    use qubit = Qubit();
                    Adjoint v(qubit);
                    Controlled Adjoint v([qubit], qubit);
                    Adjoint Controlled v([qubit], qubit);
                    Controlled Controlled Adjoint v([qubit], ([qubit], qubit));
                    Controlled Adjoint Controlled v([qubit], ([qubit], qubit));
                    Adjoint Controlled Controlled v([qubit], ([qubit], qubit));
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation B() : Unit {
                    let v = q => H(q);
                    use qubit = Qubit();
                    Adjoint v(qubit);
                    Controlled Adjoint v([qubit], qubit);
                    Adjoint Controlled v([qubit], qubit);
                    Controlled Controlled Adjoint v([qubit], ([qubit], qubit));
                    Controlled Adjoint Controlled v([qubit], ([qubit], qubit));
                    Adjoint Controlled Controlled v([qubit], ([qubit], qubit));
                }
            }"#]],
    );
}

#[test]
fn field_access_and_string_interning() {
    check(
        indoc! {r#"
            namespace A {
                import Std.Math.*;
                function ComplexAsString(x : Complex) : String {
                    if x.Imag < 0.0 {
                        $"{x.Real} - {AbsD(x.Imag)}i"
                    } else {
                        $"{x.Real} + {x.Imag}i"
                    }
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                import Std.Math.*;
                function ComplexAsString(x : Complex) : String {
                    if x.Imag < 0. {
                        $"{x.Real} - {AbsD(x.Imag)}i"
                    } else {
                        $"{x.Real} + {x.Imag}i"
                    }
                }
            }"#]],
    );
}

#[test]
fn if_exprs() {
    check(
        indoc! {r#"
            namespace A {
                function A() : Unit {
                    mutable x = 0;
                    // if
                    if true or false {
                        set x = 1;
                    }
                    // if else
                    if true and false {
                        set x = 2;
                    } else {
                        set x = 3;
                    }
                    // if elif
                    if true and false {
                        set x = 4;
                    } elif true or false {
                        set x = 5;
                    }
                    // if elif else
                    if true and false {
                        set x = 4;
                    } elif true or false {
                        set x = 5;
                    } else {
                        set x = 6;
                    }
                    // if elif elif else
                    if true and false {
                        set x = 4;
                    } elif true or false {
                        set x = 5;
                    } elif true or false {
                        set x = 5;
                    } else {
                        set x = 6;
                    }
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                function A() : Unit {
                    mutable x = 0;
                    if true or false {
                        set x = 1;
                    }
                    if true and false {
                        set x = 2;
                    } else {
                        set x = 3;
                    }
                    if true and false {
                        set x = 4;
                    } elif true or false {
                        set x = 5;
                    }
                    if true and false {
                        set x = 4;
                    } elif true or false {
                        set x = 5;
                    } else {
                        set x = 6;
                    }
                    if true and false {
                        set x = 4;
                    } elif true or false {
                        set x = 5;
                    } elif true or false {
                        set x = 5;
                    } else {
                        set x = 6;
                    }
                }
            }"#]],
    );
}

#[test]
fn copy_update_range_indices() {
    check(
        indoc! {r#"
            namespace A {
                operation A() : Result[] {
                    let mask = [false, size = 6];
                    for i in Length(mask) - 2 ..-1.. 0 {
                        let nbPair = mask w/ i... <- [true, true];
                        Message($"{nbPair}");
                    }
                    return [];
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation A() : Result[] {
                    let mask = [false, size = 6];
                    for i in Length(mask) - 2..-1..0 {
                        let nbPair = mask w/ i... <- [true, true];
                        Message($"{nbPair}");
                    }
                    return [];
                }
            }"#]],
    );
}

#[test]
fn for_loops() {
    check(
        indoc! {r#"
            namespace A {
                operation A() : Unit {
                    // For loop over `Range`
                    for i in 0..5 {
                        for j in 0..4 {
                            for k in 0..3 {
                                let x = i * j * k;
                            }
                        }
                    }
                    // For loop over `Array`
                    for element in [10, 11, 12] {
                        let x = 7 * element;
                    }
                    // For loop over array slice
                    let array = [1.0, 2.0, 3.0, 4.0];
                    for element in array[2...] {
                        let x = 2.0 * element;
                    }
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation A() : Unit {
                    for i in 0..5 {
                        for j in 0..4 {
                            for k in 0..3 {
                                let x = i * j * k;
                            }
                        }
                    }
                    for element in [10, 11, 12] {
                        let x = 7 * element;
                    }
                    let array = [1., 2., 3., 4.];
                    for element in array[2...] {
                        let x = 2. * element;
                    }
                }
            }"#]],
    );
}

#[test]
fn while_loops() {
    check(
        indoc! {r#"
            namespace A {
                operation A() : Unit {
                    mutable x = 0;
                    while x < 30 {
                        mutable y = 0;
                        while y < 3 {
                            mutable z = 0;
                            while z < 1 {
                                set z += 1;
                                set x += 1;
                            }
                            set y += 1;
                        }
                    }
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation A() : Unit {
                    mutable x = 0;
                    while x < 30 {
                        mutable y = 0;
                        while y < 3 {
                            mutable z = 0;
                            while z < 1 {
                                set z += 1;
                                set x += 1;
                            }
                            set y += 1;
                        }
                    }
                }
            }"#]],
    );
}

#[test]
fn repeat_loops() {
    check(
        indoc! {r#"
            namespace A {
                operation A() : Unit {
                    mutable x = 0;
                    repeat {
                        set x += 1;
                    } until x > 3;
                    use qubit = Qubit();
                    repeat {
                        H(qubit);
                    } until M(qubit) == Zero
                    fixup {
                        Reset(qubit);
                    }
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation A() : Unit {
                    mutable x = 0;
                    repeat {
                        set x += 1;
                    } until x > 3;
                    use qubit = Qubit();
                    repeat {
                        H(qubit);
                    } until M(qubit) == Zero
                    fixup {
                        Reset(qubit);
                    }
                }
            }"#]],
    );
}

#[test]
fn ternary() {
    check(
        indoc! {r#"
            namespace A {
                operation A() : Unit {
                    let fahrenheit = 40;
                    let absoluteValue = fahrenheit > 0 ? fahrenheit | fahrenheit * -1;
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation A() : Unit {
                    let fahrenheit = 40;
                    let absoluteValue = fahrenheit > 0 ? fahrenheit | fahrenheit * -1;
                }
            }"#]],
    );
}

#[test]
fn within_apply() {
    check(
        indoc! {r#"
            namespace A {
                operation A() : Unit {
                    use qubit = Qubit();
                    within {
                        H(qubit);
                    } apply {
                        X(qubit);
                    }
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation A() : Unit {
                    use qubit = Qubit();
                    within {
                        H(qubit);
                    } apply {
                        X(qubit);
                    }
                }
            }"#]],
    );
}

#[test]
fn type_decls() {
    check(
        indoc! {r#"
            namespace A {
                operation A() : Unit {
                    newtype Point3d = (X : Double, Y : Double, Z : Double);
                    newtype DoubleInt = (Double, ItemName : Int);
                    newtype Nested = (Double, (ItemName : Int, String));
                    let point = Point3d(1.0, 2.0, 3.0);
                    let x : Double = point.X;
                    let (x, _, _) = point!;
                    let unwrappedTuple = point!;
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation A() : Unit {
                    newtype Point3d = (X : Double, Y : Double, Z : Double);
                    newtype DoubleInt = (Double, ItemName : Int);
                    newtype Nested = (Double, (ItemName : Int, String));
                    let point = Point3d(1., 2., 3.);
                    let x : Double = point.X;
                    let (x, _, _) = point!;
                    let unwrappedTuple = point!;
                }
            }"#]],
    );
}

#[test]
fn pauli() {
    check(
        indoc! {r#"
            namespace A {
                operation A() : Unit {
                    use q = Qubit();
                    mutable pauliDimension = PauliX;
                    // Measuring along a dimension returns a `Result`:
                    let result = Measure([pauliDimension], [q]);
                    set pauliDimension = PauliY;
                    let result = Measure([pauliDimension], [q]);
                    set pauliDimension = PauliZ;
                    let result = Measure([pauliDimension], [q]);
                    set pauliDimension = PauliI;
                    let result = Measure([pauliDimension], [q]);
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation A() : Unit {
                    use q = Qubit();
                    mutable pauliDimension = PauliX;
                    let result = Measure([pauliDimension], [q]);
                    set pauliDimension = PauliY;
                    let result = Measure([pauliDimension], [q]);
                    set pauliDimension = PauliZ;
                    let result = Measure([pauliDimension], [q]);
                    set pauliDimension = PauliI;
                    let result = Measure([pauliDimension], [q]);
                }
            }"#]],
    );
}

#[test]
fn bases_and_readable_values() {
    check(
        indoc! {r#"
            namespace A {
                operation A() : Unit {
                    let foo = 0x42;
                    let foo = 0o42;
                    let foo = 42;
                    let foo = 0b101010;
                    let integer : Int = 42;
                    let unit : Unit = ();
                    let binaryBigInt : BigInt = 0b101010L;
                    let octalBigInt = 0o52L;
                    let decimalBigInt = 42L;
                    let hexadecimalBigInt = 0x2aL;
                    let foo : BigInt = 2L^74;
                    let foo = foo + 1L;
                    let foo = foo % 2L;
                    let foo = foo^2;
                    let foo = 1e-9;
                    let foo = 1E-15;
                    let foo = 1000_0000;
                }
            }"#},
        None,
        &expect![[r#"
            namespace A {
                operation A() : Unit {
                    let foo = 66;
                    let foo = 34;
                    let foo = 42;
                    let foo = 42;
                    let integer : Int = 42;
                    let unit : Unit = ();
                    let binaryBigInt : BigInt = 42L;
                    let octalBigInt = 42L;
                    let decimalBigInt = 42L;
                    let hexadecimalBigInt = 42L;
                    let foo : BigInt = 2L^74;
                    let foo = foo + 1L;
                    let foo = foo % 2L;
                    let foo = foo^2;
                    let foo = 0.000000001;
                    let foo = 0.000000000000001;
                    let foo = 10000000;
                }
            }"#]],
    );
}

#[test]
fn complex_literals() {
    check(
        "function Foo() : Complex { 3.0 + 4.0i }",
        None,
        &expect![[r#"
            namespace test {
                function Foo() : Complex {
                    3. + 4.i
                }
            }"#]],
    );
}
