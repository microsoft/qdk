namespace Kata.Verification {
    open Microsoft.Quantum.Katas;

    operation MinusState_Reference(q : Qubit) : Unit is Adj + Ctl {
        H(q);
        Z(q);
    }

    @EntryPoint()
    operation CheckSolution() : Bool {
        CheckOperationsEquivalenceOnZeroStateWithFeedback(
            ApplyToFirstCA(Kata.MinusState, _), 
            ApplyToFirstCA(MinusState_Reference, _),
            1)
    }
}
