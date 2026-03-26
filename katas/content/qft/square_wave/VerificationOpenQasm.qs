namespace Kata.Verification {
    import KatasUtils.*;

    operation SquareWave_Reference(qs : Qubit[]) : Unit is Adj + Ctl {
        for q in qs {
            H(q);
        }
        Z(qs[Length(qs) - 2]);
    }

    @EntryPoint()
    operation CheckSolution() : Bool {
        let n = 3;
        if not CheckOperationsEquivalenceOnZeroState(Kata.SquareWave, SquareWave_Reference, n) {
            Message($"Incorrect for n = {n}.");
            return false;
        }

        Message("Correct!");
        true
    }
}
