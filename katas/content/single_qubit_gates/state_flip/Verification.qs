namespace Kata.Verification {
    open Microsoft.Quantum.Katas;
    open Microsoft.Quantum.Math;

    operation StateFlip(q : Qubit) : Unit is Adj + Ctl {
        X(q);
    }

    operation CheckSolution() : Bool {
        let solution = register => Kata.StateFlip(register[0]);
        let reference = register => StateFlip(register[0]);
        let isCorrect = CheckOperationsEquivalenceStrict(solution, reference, 1);

        // Output different feedback to the user depending on whether the solution was correct.
        if isCorrect {
            Message("Correct!");
        } else {
            Message("Incorrect.");
            Message("Hint: examine the effect your solution has on the state 0.6|0〉 + 0.8|1〉 and compare it with the effect it " +
                "is expected to have.");
            use initial = Qubit(); // |0〉
            Ry(ArcTan2(0.8, 0.6) * 2.0, initial);
            ShowQuantumStateComparison([initial], solution, reference);
            Reset(initial);
        }
        isCorrect
    }
}