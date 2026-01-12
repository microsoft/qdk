// Be Primitive 0
operation Be_Gate_0(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    I(q0);
    I(q1);
    I(q2);
    I(q3);
    I(q4);
    I(q5);
    Be_Gate_0_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_0_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 2

operation Be_Gate_2(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    SWAP(q5, q1);
    SWAP(q4, q0);
    CNOT(q3, q4);
    CNOT(q1, q5);
    CNOT(q1, q2);
    CNOT(q0, q4);
    Be_Gate_2_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_2_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}


// Be Primitive 3
operation Be_Gate_3(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    SWAP(q5, q1);
    SWAP(q4, q0);
    CNOT(q2, q4);
    CNOT(q1, q5);
    CNOT(q1, q3);
    CNOT(q0, q4);
    Be_Gate_3_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_3_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 4
operation Be_Gate_4(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q5, q3);
    SWAP(q5, q1);
    SWAP(q4, q0);
    CNOT(q3, q4);
    CNOT(q2, q4);
    CNOT(q1, q2);
    Be_Gate_4_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_4_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 5
operation Be_Gate_5(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q3, q4);
    SWAP(q5, q1);
    SWAP(q4, q0);
    CNOT(q5, q3);
    CNOT(q5, q2);
    CNOT(q2, q0);
    Be_Gate_5_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_5_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 7
operation Be_Gate_7(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q2, q4);
    CNOT(q1, q3);
    CNOT(q0, q4);
    CNOT(q5, q1);
    CNOT(q4, q0);
    CNOT(q3, q0);
    CNOT(q1, q2);
    SWAP(q5, q1);
    Be_Gate_7_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_7_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 10
operation Be_Gate_10(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q5, q2);
    CNOT(q5, q1);
    CNOT(q4, q0);
    CNOT(q3, q0);
    Be_Gate_10_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_10_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 13
operation Be_Gate_13(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q1, q3);
    CNOT(q3, q4);
    CNOT(q2, q4);
    CNOT(q1, q2);
    Be_Gate_13_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_13_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 14
operation Be_Gate_14(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q3, q4);
    CNOT(q1, q2);
    CNOT(q5, q1);
    CNOT(q4, q0);
    CNOT(q2, q4);
    CNOT(q1, q3);
    Be_Gate_14_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_14_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 15
operation Be_Gate_15(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q2, q4);
    CNOT(q1, q3);
    CNOT(q5, q1);
    CNOT(q4, q0);
    CNOT(q3, q4);
    CNOT(q1, q2);
    Be_Gate_15_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_15_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 16
operation Be_Gate_16(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q3, q0);
    CNOT(q2, q3);
    CNOT(q1, q3);
    CNOT(q5, q2);
    CNOT(q3, q4);
    CNOT(q2, q3);
    CNOT(q2, q0);
    CNOT(q1, q2);
    Be_Gate_16_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_16_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 18
operation Be_Gate_18(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q5, q2);
    CNOT(q3, q0);
    CNOT(q1, q5);
    CNOT(q5, q3);
    CNOT(q4, q0);
    CNOT(q2, q4);
    CNOT(q0, q4);
    SWAP(q5, q1);
    Be_Gate_18_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_18_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 19
operation Be_Gate_19(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q2, q4);
    CNOT(q1, q3);
    CNOT(q5, q1);
    CNOT(q4, q0);
    CNOT(q1, q5);
    CNOT(q0, q4);
    Be_Gate_19_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_19_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 28
operation Be_Gate_28(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q5, q2);
    SWAP(q5, q0);
    SWAP(q4, q1);
    SWAP(q3, q2);
    CNOT(q3, q5);
    CNOT(q2, q5);
    CNOT(q0, q2);
    H(q5);
    H(q4);
    H(q3);
    H(q2);
    H(q1);
    H(q0);
    Be_Gate_28_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_28_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 29
operation Be_Gate_29(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q2, q4);
    SWAP(q5, q0);
    SWAP(q4, q1);
    SWAP(q3, q2);
    CNOT(q4, q3);
    CNOT(q4, q2);
    CNOT(q2, q1);
    H(q5);
    H(q4);
    H(q3);
    H(q2);
    H(q1);
    H(q0);
    Be_Gate_29_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_29_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 34
operation Be_Gate_34(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    SWAP(q5, q4);
    SWAP(q3, q2);
    SWAP(q1, q0);
    CNOT(q5, q1);
    CNOT(q4, q3);
    CNOT(q4, q0);
    CNOT(q2, q1);
    H(q5);
    H(q4);
    H(q3);
    H(q2);
    H(q1);
    H(q0);
    Be_Gate_34_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_34_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 36
operation Be_Gate_36(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q2, q0);
    SWAP(q5, q4);
    SWAP(q3, q2);
    SWAP(q1, q0);
    CNOT(q4, q3);
    CNOT(q4, q2);
    CNOT(q2, q1);
    H(q5);
    H(q4);
    H(q3);
    H(q2);
    H(q1);
    H(q0);
    Be_Gate_36_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_36_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 37
operation Be_Gate_37(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q1, q2);
    SWAP(q5, q4);
    SWAP(q3, q2);
    SWAP(q1, q0);
    CNOT(q3, q5);
    CNOT(q2, q5);
    CNOT(q0, q2);
    H(q5);
    H(q4);
    H(q3);
    H(q2);
    H(q1);
    H(q0);
    Be_Gate_37_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_37_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 40
operation Be_Gate_40(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q3, q0);
    CNOT(q2, q3);
    CNOT(q1, q3);
    CNOT(q5, q2);
    CNOT(q3, q4);
    CNOT(q2, q3);
    CNOT(q2, q0);
    SWAP(q5, q4);
    SWAP(q3, q2);
    SWAP(q1, q0);
    CNOT(q0, q3);
    H(q5);
    H(q4);
    H(q3);
    H(q2);
    H(q1);
    H(q0);
    Be_Gate_40_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_40_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 41
operation Be_Gate_41(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    SWAP(q5, q4);
    SWAP(q3, q2);
    SWAP(q1, q0);
    CNOT(q2, q5);
    CNOT(q1, q5);
    CNOT(q0, q4);
    CNOT(q0, q3);
    H(q5);
    H(q4);
    H(q3);
    H(q2);
    H(q1);
    H(q0);
    Be_Gate_41_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_41_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 43
operation Be_Gate_43(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q5, q2);
    CNOT(q3, q0);
    CNOT(q1, q5);
    CNOT(q0, q4);
    SWAP(q3, q2);
    CNOT(q5, q2);
    CNOT(q3, q0);
    SWAP(q5, q0);
    SWAP(q4, q1);
    H(q5);
    H(q4);
    H(q3);
    H(q2);
    H(q1);
    H(q0);
    Be_Gate_43_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_43_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 46
operation Be_Gate_46(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CNOT(q5, q2);
    CNOT(q3, q0);
    CNOT(q1, q5);
    CNOT(q0, q4);
    SWAP(q3, q2);
    CNOT(q5, q2);
    CNOT(q3, q0);
    SWAP(q5, q0);
    SWAP(q4, q1);
    H(q5);
    H(q4);
    H(q3);
    H(q2);
    H(q1);
    H(q0);
    Be_Gate_46_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_46_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 47
operation Be_Gate_47(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    SWAP(q5, q0);
    SWAP(q4, q1);
    SWAP(q3, q2);
    CNOT(q5, q1);
    CNOT(q4, q2);
    CNOT(q4, q0);
    CNOT(q3, q1);
    H(q5);
    H(q4);
    H(q3);
    H(q2);
    H(q1);
    H(q0);
    Be_Gate_47_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_47_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 50
operation Be_Gate_50(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CZ(q5, q4);
    CZ(q5, q3);
    CZ(q5, q0);
    CZ(q4, q1);
    CZ(q3, q2);
    Be_Gate_50_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_50_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 59
operation Be_Gate_59(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    I(q0);
    I(q1);
    I(q2);
    I(q3);
    I(q4);
    I(q5);
    Be_Gate_59_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_59_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 69
operation Be_Gate_69(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    CZ(q5, q3);
    CZ(q5, q0);
    CZ(q3, q1);
    CZ(q2, q1);
    S(q2);
    S(q1);
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    Be_Gate_69_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_69_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 75
operation Be_Gate_75(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    CZ(q5, q3);
    CZ(q3, q1);
    S(q5);
    S(q3);
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    Be_Gate_75_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_75_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 79
operation Be_Gate_79(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    CZ(q5, q3);
    CZ(q3, q2);
    CZ(q3, q1);
    CZ(q3, q0);
    CZ(q2, q0);
    S(q3);
    S(q1);
    CZ(q5, q0);
    CZ(q1, q0);
    S(q0);
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    Be_Gate_79_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_79_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 83
operation Be_Gate_83(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CZ(q5, q3);
    CZ(q5, q2);
    CZ(q4, q3);
    CZ(q3, q2);
    CZ(q3, q0);
    S(q3);
    S(q2);
    CZ(q5, q0);
    CZ(q4, q0);
    CZ(q2, q0);
    S(q5);
    S(q4);
    S(q0);
    Be_Gate_83_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_83_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 85
operation Be_Gate_85(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CZ(q5, q2);
    CZ(q4, q3);
    CZ(q4, q2);
    CZ(q4, q0);
    CZ(q3, q0);
    S(q3);
    S(q2);
    CZ(q5, q4);
    S(q5);
    S(q4);
    S(q0);
    Be_Gate_85_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_85_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 86
operation Be_Gate_86(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CZ(q5, q3);
    CZ(q5, q2);
    CZ(q5, q0);
    CZ(q4, q2);
    CZ(q4, q1);
    CZ(q4, q0);
    CZ(q3, q2);
    S(q5);
    S(q4);
    S(q0);
    Be_Gate_86_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_86_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 107
operation Be_Gate_107(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CZ(q5, q4);
    CZ(q5, q3);
    CZ(q5, q0);
    CZ(q4, q0);
    CZ(q2, q0);
    S(q5);
    S(q0);
    S(q4);
    Be_Gate_107_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_107_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 138
operation Be_Gate_138(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    CZ(q5, q2);
    CZ(q4, q3);
    CZ(q4, q2);
    CZ(q3, q1);
    CZ(q3, q0);
    S(q3);
    S(q2);
    CZ(q5, q1);
    CZ(q5, q0);
    CZ(q2, q1);
    CZ(q1, q0);
    S(q5);
    S(q4);
    S(q1);
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    Be_Gate_138_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_138_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 146
operation Be_Gate_146(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CZ(q4, q3);
    CZ(q4, q2);
    CZ(q3, q2);
    CZ(q3, q1);
    CZ(q2, q0);
    S(q3);
    S(q1);
    CZ(q1, q0);
    S(q4);
    Be_Gate_146_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_146_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 147
operation Be_Gate_147(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CZ(q4, q2);
    CZ(q3, q2);
    CZ(q3, q0);
    CZ(q1, q0);
    S(q3);
    S(q0);
    Be_Gate_147_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_147_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 158
operation Be_Gate_158(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    CZ(q5, q3);
    CZ(q4, q3);
    CZ(q4, q2);
    CZ(q4, q1);
    CZ(q3, q1);
    CZ(q1, q0);
    S(q3);
    S(q5);
    S(q4);
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    Be_Gate_158_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_158_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 167
operation Be_Gate_167(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CZ(q5, q3);
    CZ(q5, q2);
    CZ(q4, q2);
    CZ(q4, q1);
    CZ(q3, q2);
    CZ(q2, q0);
    S(q2);
    CZ(q5, q0);
    S(q5);
    S(q0);
    Be_Gate_167_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_167_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 170
operation Be_Gate_170(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    CZ(q5, q3);
    CZ(q4, q2);
    CZ(q4, q1);
    CZ(q3, q2);
    CZ(q3, q1);
    CZ(q2, q1);
    S(q2);
    CZ(q5, q4);
    S(q1);
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    Be_Gate_170_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_170_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 182
operation Be_Gate_182(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    CZ(q5, q3);
    CZ(q4, q3);
    CZ(q4, q1);
    CZ(q3, q0);
    CZ(q2, q0);
    S(q3);
    S(q1);
    CZ(q5, q4);
    S(q0);
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    Be_Gate_182_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_182_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 192
operation Be_Gate_192(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    CZ(q5, q3);
    CZ(q5, q2);
    CZ(q4, q3);
    CZ(q3, q0);
    CZ(q2, q1);
    S(q3);
    S(q2);
    CZ(q4, q1);
    CZ(q4, q0);
    CZ(q2, q0);
    CZ(q1, q0);
    S(q5);
    S(q4);
    S(q0);
    Be_Gate_192_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_192_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive 197
operation Be_Gate_197(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    CZ(q5, q3);
    CZ(q4, q3);
    CZ(q3, q1);
    CZ(q3, q0);
    CZ(q2, q1);
    CZ(q2, q0);
    S(q2);
    CZ(q5, q4);
    CZ(q5, q0);
    S(q4);
    S(q1);
    S(q0);
    H(q0);
    H(q1);
    H(q2);
    H(q3);
    H(q4);
    H(q5);
    Be_Gate_197_noise(q0, q1, q2, q3, q4, q5);
}

@SimulatableIntrinsic()
operation Be_Gate_197_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit) : Unit {
    body intrinsic;
}

// Be Primitive Transversal CNOT
operation Be_Gate_CX(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit, r0 : Qubit, r1 : Qubit, r2 : Qubit, r3 : Qubit, r4 : Qubit, r5 : Qubit) : Unit {
    CNOT(q0, r0);
    CNOT(q1, r1);
    CNOT(q2, r2);
    CNOT(q3, r3);
    CNOT(q4, r4);
    CNOT(q5, r5);
    Be_Gate_CX_noise(q0, q1, q2, q3, q4, q5, r0, r1, r2, r3, r4, r5);
}

@SimulatableIntrinsic()
operation Be_Gate_CX_noise(q0 : Qubit, q1 : Qubit, q2 : Qubit, q3 : Qubit, q4 : Qubit, q5 : Qubit, r0 : Qubit, r1 : Qubit, r2 : Qubit, r3 : Qubit, r4 : Qubit, r5 : Qubit) : Unit {
    body intrinsic;
}
