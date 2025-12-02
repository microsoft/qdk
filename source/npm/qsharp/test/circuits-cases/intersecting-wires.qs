operation Main() : Unit {
    use qs = Qubit[5];
    M(qs[0]);
    // Crossing a qubit wire
    Foo([qs[2], qs[4]]);
    // Crossing a classical wire
    Foo(qs[0..1]);
    // Crossing both qubit and classical wires
    Foo([qs[2], qs[0]]);
    // Spanning adjacent qubits
    Foo(qs[3..4]);
    // Crossing classical wire and adjacent qubits
    Foo(qs[0..2]);

    // Some more classical wires to intersect
    M(qs[2]);
    M(qs[2]);

    // Spanning all qubit wires, one classical wire extending
    // from the box, and crossing the other classical wires
    BoxWithMeasurements(qs)
}

operation BoxWithMeasurements(qs: Qubit[]) : Unit {
    M(qs[2]);
    Foo(qs);
}

operation Foo(qs : Qubit[]) : Unit {
    body intrinsic;
}
