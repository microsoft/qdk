import Std.Diagnostics.DumpMachine;
import Std.Math.ArcCos;
import Std.Math.PI;
import Std.Convert.IntAsDouble;
import Std.Arrays.Subarray;
import Std.Arithmetic.AddLE;
import Std.StatePreparation.PreparePureStateD;

@EntryPoint(Adaptive_RIF)
operation Main() : Unit {
    Foo()
}

operation Foo() : Unit {
    use ancilla = Qubit();
    use system = Qubit[2];
    CtlExp(ancilla, system);
}

operation CtlExp(control : Qubit, system : Qubit[]) : Unit {
    Controlled Exp([control], ([PauliX, PauliX], PI() / -2.0, system));
}