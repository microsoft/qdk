namespace Test {
    /// Demonstrates successful negative indexing as part of a loop during execution.
    /// Because the array is iterated backwards skipping the last element, the expected
    /// output is [One, One, Zero].
    operation Main() : Result[] {
        use qs = Qubit[3];
        for i in -2..-1..-Length(qs) {
            X(qs[i]);
        }
        MResetEachZ(qs)
    }
}
