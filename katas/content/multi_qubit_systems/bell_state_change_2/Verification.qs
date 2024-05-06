namespace Kata.Verification {
    open Microsoft.Quantum.Diagnostics;
    open Microsoft.Quantum.Katas;

    operation PrepareBellState(qs : Qubit[]) : Unit is Adj + Ctl {
        H(qs[0]);
        CNOT(qs[0], qs[1]);
    }


    operation BellStateChange2_Reference(qs : Qubit[]) : Unit is Adj + Ctl {
        X(qs[0]);
    }


    @EntryPoint()
    operation CheckSolution() : Bool {
        let isCorrect = CheckOperationsEquivalenceOnInitialStateStrict(
            PrepareBellState,
            Kata.BellStateChange2, 
            BellStateChange2_Reference, 
            2);

        if isCorrect {
            Message("Correct!");
        } else {
            Message("Incorrect");
            ShowQuantumStateComparison(2, PrepareBellState, Kata.BellStateChange2, BellStateChange2_Reference);
        }

        return isCorrect;
    }
}
