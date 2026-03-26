namespace Kata.Verification {
    import KatasUtils.*;

    operation BinaryFractionQuantum_Reference(q : Qubit, j : Qubit[]) : Unit is Adj + Ctl {
        for ind in 0..Length(j) - 1 {
            Controlled R1Frac([j[ind]], (2, ind + 1, q));
        }
    }

    @EntryPoint()
    operation CheckSolution() : Bool {
        let n = 2;
        let solution = qs => Kata.BinaryFractionQuantum(qs[0], qs[1...]);
        let reference = qs => BinaryFractionQuantum_Reference(qs[0], qs[1...]);
        if not CheckOperationsAreEqualStrict(n + 1, solution, reference) {
            Message($"Incorrect for n = {n}.");
            return false;
        }

        Message("Correct!");
        true
    }
}
