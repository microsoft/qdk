namespace Test {
    operation Main() : Result[] {
        use qs = Qubit[2];
        use anc = Qubit();
        repeat {
            ApplyToEach(H, qs);
            Controlled X(qs, anc);
        } until MResetZ(anc) == Zero
        fixup {
            ResetAll(qs);
        }
        MResetEachZ(qs)
    }
}
