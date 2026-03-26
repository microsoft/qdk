namespace Kata.Verification {
    import KatasUtils.*;

    operation AllBasisVectors_Reference(qs : Qubit[]) : Unit is Adj + Ctl {
        for q in qs {
            H(q);
        }
    }

    @EntryPoint()
    operation CheckSolution() : Bool {
        let n = 3;
        if not CheckOperationsEquivalenceOnZeroState(Kata.AllBasisVectors, AllBasisVectors_Reference, n) {
            Message($"Incorrect for n = {n}.");
            return false;
        }

        Message("Correct!");
        true
    }
}
