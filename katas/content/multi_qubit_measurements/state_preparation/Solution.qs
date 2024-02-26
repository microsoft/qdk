namespace Kata {
    open Microsoft.Quantum.Measurement;

    operation PostSelection(qs : Qubit[]) : Unit {
        // Initialize the extra qubit
        use anc = Qubit();
        // Using the repeat-until-success pattern to prepare the right state
        mutable res = Zero;
        repeat {
            ApplyToEach(H, qs);
            Controlled X(qs, anc);
            set res = MResetZ(anc);
        }
        until (res == Zero)
        fixup {
            ResetAll(qs);
        }
    }

}
