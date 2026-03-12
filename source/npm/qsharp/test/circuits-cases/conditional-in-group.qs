operation Main() : Unit {
    use (aliceQubit) = Qubit();

    let sourceBit1 = DrawRandomBit();
    let sourceBit2 = DrawRandomBit();
    Foo(sourceBit1, sourceBit2, aliceQubit);
}

operation DrawRandomBit() : Bool {
    use q = Qubit();
    H(q);
    return MResetZ(q) == One;
}

operation CreateEntangledPair(q1 : Qubit, q2 : Qubit) : Unit {
    H(q1);
    CNOT(q1, q2);
}

operation Foo(bit1 : Bool, bit2 : Bool, qubit : Qubit) : Unit {
    if (bit1) {
        Z(qubit);
    }
    if (bit2) {
        X(qubit);
    }
}
