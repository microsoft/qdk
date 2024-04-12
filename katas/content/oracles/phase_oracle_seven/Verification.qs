namespace Kata.Verification {
    open Microsoft.Quantum.Arrays;
    open Microsoft.Quantum.Katas;
    open Microsoft.Quantum.Math;

    operation IsSeven_PhaseOracle_Reference(x : Qubit[]) : Unit is Adj + Ctl {
        Controlled Z(Most(x), Tail(x));
    }

    @EntryPoint()
    operation CheckSolution() : Bool {
        let N = 3;
        let isCorrect = CheckOperationsEquivalenceStrict(
            Kata.IsSeven_PhaseOracle,
            IsSeven_PhaseOracle_Reference,
            3);
        if isCorrect {
            Message("Correct!");
        } else {
            Message("Incorrect.");
            Message("Hint: examine how your solution transforms the given state and compare it with the expected " +
                "transformation");
            ShowQuantumStateComparison(3, PrepRandomState, Kata.IsSeven_PhaseOracle, IsSeven_PhaseOracle_Reference);
        }
        isCorrect
    }
}
