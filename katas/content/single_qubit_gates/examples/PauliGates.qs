namespace Demo {
    // To use a namespace, you need to use the `open` keyword to access it
    open Microsoft.Quantum.Diagnostics;
    open Microsoft.Quantum.Intrinsic;

    @EntryPoint()
    operation PauliGatesUsage () : Unit {
        // This allocates a qubit for us to work with
        use q = Qubit();

        // This will put the qubit into an uneven superposition |𝜓❭,
        // where the amplitudes of |0⟩ and |1⟩ have different moduli
        Ry(1.0, q);

        Message("Qubit in state |𝜓❭:");
        DumpMachine();

        // Let's apply the X gate; notice how it swaps the amplitudes of the |0❭ and |1❭ basis states
        X(q);
        Message("Qubit in state X|𝜓❭:");
        DumpMachine();

        // Applying the Z gate adds -1 relative phase to the |1❭ basis states
        Z(q);
        Message("Qubit in state ZX|𝜓❭:");
        DumpMachine();

        // Finally, applying the Y gate returns the qubit to its original state |𝜓❭, with an extra global phase of i
        Y(q);
        Message("Qubit in state YZX|𝜓❭:");
        DumpMachine();

        // This returns the qubit into state |0❭
        Reset(q);
    }
}
