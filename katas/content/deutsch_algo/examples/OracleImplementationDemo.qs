namespace Kata {
    import Std.Diagnostics.*;
    import Std.Math.*;

    operation PhaseOracle_Zero(x : Qubit) : Unit {
        // Do nothing...
    }

    operation PhaseOracle_One(x : Qubit) : Unit {
        // Apply a global phase of -1
        R(PauliI, 2.0 * PI(), x);
    }

    operation PhaseOracle_X(x : Qubit) : Unit {
        Z(x);
    }

    @EntryPoint()
    operation OracleImplementationDemo() : Unit {
        use q = Qubit();
        Ry(2.0 * ArcCos(0.6), q);
        Message("The qubit state before oracle application is 0.6|0⟩ + 0.8|1⟩:");
        DumpMachine();

        // Apply the oracle.
        // Experiment with using other oracles to see their behavior!
        // (Note that the -1 global phase might not show up in simulation)
        PhaseOracle_X(q);

        Message("The qubit state after oracle application:");
        DumpMachine();

        Reset(q);
    }
}
