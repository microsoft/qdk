namespace Kata.Verification {
    import KatasUtils.*;

    operation AllEvenVectors_Reference(qs : Qubit[]) : Unit is Adj + Ctl {
        for q in qs[...Length(qs) - 2] {
            H(q);
        }
    }

    @EntryPoint()
    operation CheckSolution() : Bool {
        let n = 3;
        if not CheckOperationsEquivalenceOnZeroState(Kata.AllEvenVectors, AllEvenVectors_Reference, n) {
            Message($"Incorrect for n = {n}.");
            return false;
        }

        Message("Correct!");
        true
    }
}
