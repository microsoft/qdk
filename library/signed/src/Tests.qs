// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

import Std.Diagnostics.Fact;
import Operations.Invert2sSI;
import Measurement.MeasureSignedInteger;

/// This entrypoint runs tests for the signed integer library.
operation Main() : Unit {
    UnsignedOpTests();
    MeasureSignedIntTests();
    SignedOpTests();

}

operation MeasureSignedIntTests() : Unit {
    use a = Qubit[4];

    // 0b0001 == 1
    X(a[0]);
    let res = MeasureSignedInteger(a, 4);
    Fact(res == 1, $"Expected 1, received {res}");

    // 0b1111 == -1
    X(a[0]);
    X(a[1]);
    X(a[2]);
    X(a[3]);
    let res = MeasureSignedInteger(a, 4);
    Fact(res == -1, $"Expected -1, received {res}");

    // 0b01000 == 8
    use a = Qubit[5];
    X(a[3]);
    let res = MeasureSignedInteger(a, 5);
    Fact(res == 8, $"Expected 8, received {res}");

    // 0b11110 == -2
    X(a[1]);
    X(a[2]);
    X(a[3]);
    X(a[4]);
    let res = MeasureSignedInteger(a, 5);
    Fact(res == -2, $"Expected -2, received {res}");

    // 0b11000 == -8
    X(a[3]);
    X(a[4]);
    let res = MeasureSignedInteger(a, 5);
    Fact(res == -8, $"Expected -8, received {res}");

}

operation SignedOpTests() : Unit {
    use a = Qubit[32];
    use b = Qubit[32];
    use c = Qubit[64];

    // 0b11111110 (-2 in twos complement) * 0b00000001 == 0b11111110 (-2)
    X(a[1]);
    Operations.Invert2sSI(a);
    X(b[0]);
    TestSignedIntOp(Operations.MultiplySI, a, b, c, -2);

    // 0b11111110 (-2 in twos complement) * 0b11111111 (-1 in twos complement) == 0b00000010 (2)
    X(a[1]);
    Operations.Invert2sSI(a);
    X(b[0]);
    Operations.Invert2sSI(b);
    TestSignedIntOp(Operations.MultiplySI, a, b, c, 2);


    // 0b11111110 (-2 in twos complement) squared is 0b00000100 (4)
    X(a[1]);
    Operations.Invert2sSI(a);
    TestSignedIntOp((a, b, _) => Operations.SquareSI(a, c), a, b, c, 4);

}

operation UnsignedOpTests() : Unit {
    use a = Qubit[2];
    use b = Qubit[2];
    use c = Qubit[4];

    // 0b10 * 0b01 == 0b10 (1 * 2 = 2)
    X(a[0]);
    X(b[1]);
    TestIntOp(Operations.MultiplyI, a, b, c, 2);

    // 0b01 * 0b10 == 0b10 (1 * 2 = 2)
    X(a[1]);
    X(b[0]);
    TestIntOp(Operations.MultiplyI, a, b, c, 2);

    // 0b11 * 0b11 == 0b1001 (3 * 3 = 9)
    X(a[0]);
    X(b[0]);
    X(a[1]);
    X(b[1]);
    TestIntOp(Operations.MultiplyI, a, b, c, 9);


    use a = Qubit[8];
    use b = Qubit[8];
    use c = Qubit[16];

    // 0b00001010 * 0b00001011 == 0b01100100 (10 * 11 = 110)
    X(a[1]);
    X(a[3]);
    X(b[1]);
    X(b[3]);
    X(b[0]);
    TestIntOp(Operations.MultiplyI, a, b, c, 110);

    // 0b00001010 ^ 2 = 0b01100100 (10 ^ 2 = 100)
    X(a[1]);
    X(a[3]);
    TestIntOp((a, b, _) => Operations.SquareI(a, c), a, b, c, 100);

    // 0b00001010 / 0b00000010 == 0b00000101 (10 / 2 = 5)
    X(a[1]);
    X(a[3]);
    X(b[1]);
    // need smaller result register for div, mod, etc
    use d = Qubit[8];
    TestIntOp(Operations.DivideI, a, b, d, 5);
}

operation TestIntOp(op : (Qubit[], Qubit[], Qubit[]) => Unit, a : Qubit[], b : Qubit[], c : Qubit[], expect : Int) : Unit {
    op(a, b, c);
    let res = MeasureInteger(c);
    Fact(res == expect, $"Expected {expect}, got {res}");
    ResetAll(a + b + c);
}

operation TestSignedIntOp(op : (Qubit[], Qubit[], Qubit[]) => Unit, a : Qubit[], b : Qubit[], c : Qubit[], expect : Int) : Unit {
    op(a, b, c);
    let res = MeasureSignedInteger(c, 64);
    Fact(res == expect, $"Expected {expect}, got {res}");
    ResetAll(a + b + c);
}
