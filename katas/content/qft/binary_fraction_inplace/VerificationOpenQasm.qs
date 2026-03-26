namespace Kata.Verification {
    import KatasUtils.*;

    operation BinaryFractionQuantumInPlace_Reference(j : Qubit[]) : Unit is Adj + Ctl {
        H(j[0]);
        for ind in 1..Length(j) - 1 {
            Controlled R1Frac([j[ind]], (2, ind + 1, j[0]));
        }
    }

    @EntryPoint()
    operation CheckSolution() : Bool {
        let n = 3;
        if not CheckOperationsAreEqualStrict(n, Kata.BinaryFractionQuantumInPlace, BinaryFractionQuantumInPlace_Reference) {
            Message($"Incorrect for n = {n}.");
            return false;
        }

        Message("Correct!");
        true
    }
}
