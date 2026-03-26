namespace Kata.Verification {
    import KatasUtils.*;

    operation AlternatingAmplitudes_Reference(qs : Qubit[]) : Unit is Adj + Ctl {
        for q in qs {
            H(q);
        }
        Z(qs[Length(qs) - 1]);
    }

    @EntryPoint()
    operation CheckSolution() : Bool {
        let n = 3;
        if not CheckOperationsEquivalenceOnZeroState(Kata.AlternatingAmplitudes, AlternatingAmplitudes_Reference, n) {
            Message($"Incorrect for n = {n}.");
            return false;
        }

        Message("Correct!");
        true
    }
}
