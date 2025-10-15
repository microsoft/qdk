namespace Test {

    import Std.Intrinsic.*;
    import Std.Math.*;
    import Std.Measurement.*;

    // Verifies the use of the rotation quantum operations from Q#'s Microsoft.Quantum.Intrinsic namespace.
    // Expected simulation output: ([1, 1], [1, 1], [1, 1]).
    @EntryPoint()
    operation Main() : (Result[], Result[], Result[]) {
        // The period for each rotation operator is 4π. This test initializes all qubits in a known state, performs 8
        // π/2 rotations over each axis, and verifies that the qubit is back at the initial known state after these
        // rotations have been performed.
        use rxRegister = Qubit[2];
        use ryRegister = Qubit[2];
        use rzRegister = Qubit[2];
        InitializeRxRegister(rxRegister);   // |11⟩
        InitializeRyRegister(ryRegister);   // (i|1⟩)(i|1⟩)
        InitializeRzRegister(rzRegister);   // |11⟩
        let rotationPeriod = 4.0 * PI();
        let stepsDouble = 8.0;
        let stepsInt = 8;
        let theta = rotationPeriod / stepsDouble;
        for _ in 1..stepsInt {
            // Test both R and its corresponding axis rotation gate (Rx, Ry, Rz) using the same register.
            Rx(theta, rxRegister[0]);
            R(PauliX, theta, rxRegister[1]);
            Ry(theta, ryRegister[0]);
            R(PauliY, theta, ryRegister[1]);
            Rz(theta, rzRegister[0]);
            R(PauliZ, theta, rzRegister[1]);
        }

        let rxResult = MResetZ2Register(rxRegister);
        let ryResult = MResetZ2Register(ryRegister);
        let rzResult = MResetZ2Register(rzRegister);
        return (rxResult, ryResult, rzResult);
    }

    operation InitializeRxRegister(register : Qubit[]) : Unit {
        for qubit in register {
            X(qubit);
        }
    }

    operation InitializeRyRegister(register : Qubit[]) : Unit {
        for qubit in register {
            Y(qubit);
        }
    }

    operation InitializeRzRegister(register : Qubit[]) : Unit {
        for qubit in register {
            H(qubit);
            Z(qubit);
            H(qubit);
        }
    }

    operation MResetZ2Register(register : Qubit[]) : Result[] {
        return [MResetZ(register[0]), MResetZ(register[1])];
    }
}
