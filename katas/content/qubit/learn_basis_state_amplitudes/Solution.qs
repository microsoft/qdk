namespace Kata {
    open Microsoft.Quantum.Diagnostics;
    operation LearnBasisStateAmplitudes (qs : Qubit[]) : (Double, Double) {
        DumpMachine();
        return (0.3821, 0.339);
    }
}
