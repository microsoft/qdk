namespace Kata.Verification {
    open Microsoft.Quantum.Arrays;
    open Microsoft.Quantum.Convert;
    open Microsoft.Quantum.Katas;
    open Microsoft.Quantum.Math;

    operation BinaryFractionQuantumInPlace_Reference(j : Qubit[]) : Unit is Adj + Ctl {
        H(j[0]);
        for ind in 1 .. Length(j) - 1 {
            Controlled R1Frac([j[ind]], (2, ind + 1, j[0]));
        }
    }

    @EntryPoint()
    operation CheckSolution() : Bool {
        for n in 1 .. 5 {
            if not CheckOperationsAreEqualStrict(n, Kata.BinaryFractionQuantumInPlace, BinaryFractionQuantumInPlace_Reference) {
                Message($"Incorrect for n = {n}.");
                return false;
            }
        }

        Message("Correct!");
        true
    }
}
