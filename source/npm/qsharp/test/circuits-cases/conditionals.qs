operation Main() : Unit {
    use qs = Qubit[32];

    ResultComparisonToLiteral(qs[0]);
    ResultComparisonToLiteralZero(qs[1]);
    ElseBlockOnly(qs[2]);
    ResultComparisonToResult(qs[3], qs[4]);
    ResultComparisonEmptyBlock(qs[5], qs[6]);
    IfElse(qs[7], qs[8]);
    SequentialIfs(qs[9], qs[10], qs[11]);
    NestedIfs(qs[12], qs[13], qs[14]);
    MultiplePossibleFloatValuesInUnitaryArg(qs[15], qs[16]);
    CustomIntrinsicVariableArg(qs[17]);
    BranchOnDynamicDouble(qs[18], qs[19]);
    BranchOnDynamicBool(qs[20], qs[21]);
    NestedCallablesInBranch(qs[22], qs[23]);
    ConditionalInLoop(qs[24], qs[25]);
    NestedConditionalsInCallable(qs[26], qs[27], qs[28]);
    MeasurementInConditional(qs[29], qs[30], qs[31]);

    ResetAll(qs);
}

operation ResultComparisonToLiteral(q : Qubit) : Result[] {
    H(q);
    let r1 = M(q);
    if (r1 == One) {
        X(q);
    }
    [r1]
}

// Test-case operations mirroring source/compiler/qsc/src/interpret/cond_tests.rs

operation ResultComparisonToLiteralZero(q : Qubit) : Result[] {
    H(q);
    let r1 = M(q);
    if (r1 == Zero) {
        X(q);
    }
    [r1]
}

operation ElseBlockOnly(q : Qubit) : Result[] {
    H(q);
    let r1 = M(q);
    if (r1 == Zero) {} else {
        X(q);
    }
    [r1]
}

operation ResultComparisonToResult(q1 : Qubit, q2 : Qubit) : Result[] {
    H(q1);
    H(q2);
    let r1 = M(q1);
    let r2 = M(q2);
    if (r1 == r2) {
        X(q1);
    }
    [r1, r2]
}

operation ResultComparisonEmptyBlock(q1 : Qubit, q2 : Qubit) : Int {
    H(q1);
    H(q2);
    let r1 = M(q1);
    let r2 = M(q2);
    mutable i = 4;
    if (r1 == r2) {
        set i = 5;
    }
    i
}

operation IfElse(q0 : Qubit, q1 : Qubit) : Result[] {
    H(q0);
    let r = M(q0);
    if r == One {
        X(q1);
    } else {
        Y(q1);
    }
    let r1 = M(q1);
    [r, r1]
}

operation SequentialIfs(q0 : Qubit, q1 : Qubit, q2 : Qubit) : Result[] {
    H(q0);
    H(q1);
    let r0 = M(q0);
    let r1 = M(q1);
    if r0 == One {
        X(q2);
    } else {
        Z(q2);
    }
    if r1 == One {
        X(q2);
    } else {
        Y(q2);
    }
    let r2 = M(q2);
    [r0, r1, r2]
}

operation NestedIfs(q0 : Qubit, q1 : Qubit, q2 : Qubit) : Result[] {
    H(q0);
    H(q1);
    let r0 = M(q0);
    let r1 = M(q1);
    if r0 == One {
        if r1 == One {
            X(q2);
        } else {
            Y(q2);
        }
    } else {
        Z(q2);
    }
    let r2 = M(q2);
    [r0, r1, r2]
}

operation MultiplePossibleFloatValuesInUnitaryArg(q0 : Qubit, q1 : Qubit) : Result[] {
    H(q0);
    let r = M(q0);
    mutable theta = 1.0;
    if r == One {
        set theta = 2.0;
    };
    Rx(theta, q1);
    let r1 = M(q1);
    [r, r1]
}

@SimulatableIntrinsic()
operation foo(q : Qubit, x : Int) : Unit {
    for i in 1..x {
        H(q);
    }
}

operation CustomIntrinsicVariableArg(q : Qubit) : Unit {
    mutable x = 4;
    H(q);
    if (M(q) == One) {
        set x = 5;
    }
    foo(q, x);
}

operation BranchOnDynamicDouble(q0 : Qubit, q1 : Qubit) : Result[] {
    H(q0);
    let r = M(q0);
    mutable theta = 1.0;
    if r == One {
        set theta = 2.0;
    };
    if theta > 1.5 {
        set theta = 3.0;
    } else {
        set theta = 4.0;
    }
    Rx(theta, q1);
    let r1 = M(q1);
    [r, r1]
}

operation BranchOnDynamicBool(q0 : Qubit, q1 : Qubit) : Result[] {
    H(q0);
    let r = M(q0);
    mutable cond = true;
    if r == One {
        set cond = false;
    };
    if cond {
        set cond = false;
    } else {
        set cond = true;
    }
    if cond {
        X(q1);
    }
    let r1 = M(q1);
    [r, r1]
}

operation Foo(q : Qubit) : Unit {
    Bar(q);
}

operation Bar(q : Qubit) : Unit {
    X(q);
    Y(q);
}

operation NestedCallablesInBranch(q : Qubit, q1 : Qubit) : Unit {
    Foo(q);
    H(q1);
    if (M(q1) == One) {
        Foo(q);
    }
}

operation ConditionalInLoop(q0 : Qubit, q1 : Qubit) : Unit {
    let results = [MResetZ(q1), MResetZ(q1)];

    for j in 0..1 {
        if results[j] == One {
            X(q0);
        }
    }

    for j in 0..1 {
        Baz(q0);
    }
}

operation Baz(q : Qubit) : Unit {
    H(q);
}

operation NestedConditionalsInCallable(q : Qubit, q0 : Qubit, q1 : Qubit) : Unit {
    let r0 = MResetZ(q0);
    let r1 = MResetZ(q1);
    Quux(q, r0, r1);
}

operation Quux(q : Qubit, r0 : Result, r1 : Result) : Unit {
    if r0 == One {} else {
        if r1 == One {
            X(q);
        } else {
            Z(q);
        }
    }
}

operation MeasurementInConditional(q0 : Qubit, q1 : Qubit, q2: Qubit) : Unit {
    H(q1);
    H(q1);
    H(q1);
    let r1 = M(q1);

    if r1 == One {
        H(q0);
        let r0 = M(q2);
    }
}