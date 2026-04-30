namespace Test {

    import Std.Intrinsic.*;
    import Std.Measurement.*;

    // Verifies the use of the Measure operation from Q#'s Microsoft.Quantum.Intrinsic namespace.
    // Expected simulation output: ([0, 0], [1, 1], [0, 1, 0]).
    @EntryPoint()
    operation Main() : (Result[], Result[], Result[]) {
        use register = Qubit[3];
        Encode(register);

        // Verify parity between qubits.
        let firstParity01 = Measure([PauliX, PauliX, PauliI], register);
        let firtsParity12 = Measure([PauliI, PauliX, PauliX], register);

        // Verify parity between qubits after flipping one of them.
        Z(register[1]);
        let secondParity01 = Measure([PauliX, PauliX, PauliI], register);
        let secondParity12 = Measure([PauliI, PauliX, PauliX], register);

        // Decode.
        Adjoint Encode(register);
        return (
            [firstParity01, firtsParity12],
            [secondParity01, secondParity12],
            [MResetZ(register[0]), MResetZ(register[1]), MResetZ(register[2])]
        );
    }

    operation Encode(register : Qubit[]) : Unit is Adj {
        CNOT(register[0], register[1]);
        CNOT(register[0], register[2]);
        H(register[0]);
        H(register[1]);
        H(register[2]);
    }
}
