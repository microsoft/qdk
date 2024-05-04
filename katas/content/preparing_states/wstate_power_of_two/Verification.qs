namespace Kata.Verification {
    open Microsoft.Quantum.Convert;
    open Microsoft.Quantum.Katas;
    open Microsoft.Quantum.Math;

    operation WState_PowerOfTwo_Reference (qs : Qubit[]) : Unit is Adj + Ctl {
        let N = Length(qs);
        Ry(2.0 * ArcSin(Sqrt(1.0/IntAsDouble(N))), qs[0]);
        for i in 1 .. N - 1 {
            ApplyControlledOnInt(0, Ry(2.0 * ArcSin(Sqrt(1.0/IntAsDouble(N - i))), _), qs[0 .. i-1], qs[i]);
        }
    }

    @EntryPoint()
    operation CheckSolution() : Bool {
        for n in [1, 2, 4, 8, 16] {
            Message($"Testing for N = {n}...");
            if not CheckOperationsEquivalenceOnZeroStateWithFeedback(
                Kata.WState_PowerOfTwo,
                WState_PowerOfTwo_Reference,
                n) {
                return false;
            }
        }

        return true;
    }
}
