namespace Test {
    import Std.Intrinsic.*;
    import Std.Arrays.*;
    import Std.Measurement.*;

    @EntryPoint()
    operation Main() : Result {
        use bellPair = Qubit[2] {
            H(bellPair[0]);
            CNOT(bellPair[0], bellPair[1]);
            use qubit = Qubit() {
                EncodeValue(true, PauliX, qubit);
                CNOT(qubit, bellPair[0]);
                H(qubit);
                if (M(bellPair[0]) == One) {
                    X(bellPair[1]);
                }
                if (MResetZ(qubit) == One) {
                    Z(bellPair[1]);
                }
                let mres = Measure([PauliX], [bellPair[1]]);
                ResetAll(bellPair);
                return mres;
            }
        }
    }

    operation EncodeValue(value : Bool, basis : Pauli, qubit : Qubit) : Unit {
        if (value) {
            X(qubit);
        }
        PreparePauliEigenstate(basis, qubit);
    }

    operation PreparePauliEigenstate(basis : Pauli, qubit : Qubit) : Unit {
        if (basis == PauliX) {
            H(qubit);
        } elif (basis == PauliY) {
            H(qubit);
            S(qubit);
        }
    }
}
