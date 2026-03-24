namespace Test {
    import Std.Math.*;
    import Std.Diagnostics.*;

    @EntryPoint()
    operation Main() : (Bool, Int) {
        // A qubit register that will be used for encoding.
        use encodedRegister = Qubit[3];

        // Initialize the first qubit in the register to a |-〉 state.
        H(encodedRegister[0]);
        Z(encodedRegister[0]);

        // Apply several unitary operations to the encoded qubits
        // performing bit flip detection and correction between each application.
        mutable bitFlipCount = 0;
        within {
            // The 3 qubit register is used as a repetition code.
            Encode(encodedRegister);
        } apply {
            let iterations = 5;
            for _ in 1..iterations {
                // Apply a unitary operatyion to the encoded register that should
                // effectively perform an identity operation but may be noisy
                // on the quantum hardware and introduce errors.
                ApplyRotationalIdentity(encodedRegister);

                // Measure the bit flip error syndrome, revert the bit flip if needed,
                // and increase the count if a bit flip occurred.
                let (parity01, parity12) = MeasureBitFlipSyndrome(encodedRegister);
                let bitFlipReverted = RevertBitFlip(encodedRegister, parity01, parity12);
                if (bitFlipReverted) {
                    set bitFlipCount += 1;
                }
            }
        }

        // Transform the qubit to the |1〉 state and measure it in the computational basis.
        H(encodedRegister[0]);
        let result = MResetZ(encodedRegister[0]) == One;
        // Note that the qubit at index 0 is already reset by MResetZ operation.
        // There's no need to reset it again. Also, MResetZ operation is
        // preferable to the measurement, which is followed by Reset as MResetZ
        // may be directly implemented by the hardware.
        ResetAll(encodedRegister[1...]);

        // The output of the program is a boolean-integer tuple where the boolean
        // represents whether the qubit measurement result was the expected one
        // and the integer represents the number of times bit flips occurred
        // throughout the program.
        return (result, bitFlipCount);
    }

    /// # Summary
    /// Apply four 𝜋/2 rotations about the x-axis to all qubits in the `register`.
    ///
    /// # Description
    /// This operation implements an identity operation using rotations about the x-axis.
    /// The Rx operation has a period of 2𝜋. Using it to apply four 𝜋/2 rotations
    /// about the x-axis, effectively leaves the qubit register in its original state.
    /// However it is likely to be very noisy on a quantum hardware.
    operation ApplyRotationalIdentity(register : Qubit[]) : Unit is Adj {
        let theta = PI() * 0.5;
        for i in 1..4 {
            for qubit in register {
                Rx(theta, qubit);
            }
        }
    }

    /// # Summary
    /// Reverts bit flips in the `register` based on `parity01` and `parity12`.
    operation RevertBitFlip(register : Qubit[], parity01 : Result, parity12 : Result) : Bool {
        mutable result = true;
        if parity01 == One {
            if parity12 == One {
                X(register[1]);
            } else {
                X(register[0]);
            }
        } else {
            if parity12 == One {
                X(register[2]);
            } else {
                set result = false;
            }
        }
        return result;
    }

    operation Encode(register : Qubit[]) : Unit is Adj {
        CNOT(register[0], register[1]);
        CNOT(register[0], register[2]);
    }

    /// # Summary
    /// Measures the bit flip syndrome by checking the parities between
    /// qubits 0 and 1, and between qubits 1 and 2.
    operation MeasureBitFlipSyndrome(encodedRegister : Qubit[]) : (Result, Result) {
        Fact(Length(encodedRegister) == 3, "Encoded register must be of length 3.");
        use auxiliaryRegister = Qubit[2];

        CNOT(encodedRegister[0], auxiliaryRegister[0]);
        CNOT(encodedRegister[1], auxiliaryRegister[0]);
        CNOT(encodedRegister[1], auxiliaryRegister[1]);
        CNOT(encodedRegister[2], auxiliaryRegister[1]);

        let parity01 = MResetZ(auxiliaryRegister[0]);
        let parity12 = MResetZ(auxiliaryRegister[1]);
        return (parity01, parity12);
    }
}
