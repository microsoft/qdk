// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.


import Std.Core.*;
import Std.Convert.*;
import Std.Diagnostics.*;
import Std.Intrinsic.*;
import QIR.Intrinsic.*;
import QIR.Runtime.*;

/// # Summary
/// Jointly measures a register of qubits in the Pauli Z basis.
///
/// # Description
/// Measures a register of qubits in the `Z ⊗ Z ⊗ ••• ⊗ Z`
/// basis, representing the parity of the entire register.
/// This operation does not reset the measured qubits to the |0⟩ state,
/// leaving them in the state that corresponds to the measurement result.
///
/// # Input
/// ## register
/// The register to be jointly measured.
///
/// # Output
/// The result of measuring in the `Z ⊗ Z ⊗ ••• ⊗ Z` basis.
///
/// # See also
/// - [Std.Measurement.MeasureEachZ](xref:Qdk.Std.Measurement.MeasureEachZ)
operation MeasureAllZ(register : Qubit[]) : Result {
    Measure(Repeated(PauliZ, Length(register)), register)
}

/// # Summary
/// Measures each qubit in a given array in the Pauli Z basis.
///
/// # Description
/// Measures each qubit in a register in the `Z` basis
/// and returns the result of each measurement.
/// This operation does not reset the measured qubits to the |0⟩ state,
/// leaving them in the state that corresponds to the measurement results.
///
/// # Input
/// ## targets
/// An array of qubits to be measured.
/// # Output
/// An array of measurement results.
///
/// # Remarks
/// Please note the following differences:
/// - Operation `MeasureEachZ` performs one measurement for each qubit and returns
///   an array of results. The operation does not reset the qubits.
/// - Operation `MResetEachZ` performs one measurement for each qubit and returns
///   an array of results. The operation resets all qubits to |0⟩ state.
/// - Operation `MeasureAllZ` performs a joint measurement on all qubits
///   and returns one result. The operation does not reset the qubits.
///
/// # See also
/// - [Std.Measurement.MeasureAllZ](xref:Qdk.Std.Measurement.MeasureAllZ)
/// - [Std.Measurement.MResetEachZ](xref:Qdk.Std.Measurement.MResetEachZ)
operation MeasureEachZ(register : Qubit[]) : Result[] {
    mutable results = [];
    for qubit in register {
        set results += [M(qubit)];
    }
    results
}

/// # Summary
/// Measures each qubit in a given array in the Pauli Z basis
/// and resets them to |0⟩ state.
///
/// # Input
/// ## targets
/// An array of qubits to be measured.
///
/// # Output
/// An array of measurement results.
///
/// # See also
/// - [Std.Measurement.MeasureEachZ](xref:Qdk.Std.Measurement.MeasureEachZ)
operation MResetEachZ(register : Qubit[]) : Result[] {
    mutable results = [];
    for qubit in register {
        set results += [MResetZ(qubit)];
    }
    results
}

/// # Summary
/// Performs a single-qubit measurement in the Pauli X basis,
/// and resets `target` to the |0⟩ state after the measurement.
///
/// # Input
/// ## target
/// A single qubit to be measured.
///
/// # Output
/// The result of measuring `target` in the Pauli X basis.
operation MResetX(target : Qubit) : Result {
    // Map the qubit's state from the Z-basis to the X-basis.
    // Then measure and reset the qubit.
    H(target);
    MResetZ(target)
}

/// # Summary
/// Performs a single-qubit measurement in the Pauli Y basis,
/// and resets `target` to the |0⟩ state after the measurement.
///
/// # Input
/// ## target
/// A single qubit to be measured.
///
/// # Output
/// The result of measuring `target` in the Pauli Y basis.
operation MResetY(target : Qubit) : Result {
    // Map the qubit's state from the Z-basis to the Y-basis.
    // Then measure and reset the qubit.
    // Note: this use HSadj instead of HSH since that is sufficient for measurement.
    Adjoint S(target);
    H(target);
    MResetZ(target)
}

/// # Summary
/// Performs a single-qubit measurement in the Pauli Z basis,
/// and resets `target` to the |0⟩ state after the measurement.
///
/// # Input
/// ## target
/// A single qubit to be measured.
///
/// # Output
/// The result of measuring `target` in the Pauli Z basis.
operation MResetZ(target : Qubit) : Result {
    __quantum__qis__mresetz__body(target)
}

/// # Summary
/// Measures the content of a quantum register and converts it to an integer.
/// The measurement is performed with respect to the standard computational basis,
/// i.e., the eigenbasis of `PauliZ`. Input register is reset to the |00...0⟩ state,
/// which is suitable for releasing the register back to a target machine.
///
/// # Input
/// ## target
/// A quantum register in the little-endian encoding.
///
/// # Output
/// An unsigned integer that contains the measured value of `target`.
operation MeasureInteger(target : Qubit[]) : Int {
    let nBits = Length(target);
    Fact(nBits < 64, $"`Length(target)` must be less than 64, but was {nBits}.");

    mutable number = 0;
    for i in 0..nBits - 1 {
        if (MResetZ(target[i]) == One) {
            set number |||= 1 <<< i;
        }
    }

    number
}

/// # Summary
/// Measures the content of a quantum register and converts it to an unsigned
/// little-endian `BigInt`. The measurement is performed in the computational
/// (`PauliZ`) basis, `target[0]` is treated as the least significant bit, and the
/// input register is reset to the |00...0⟩ state.
///
/// # Input
/// ## target
/// A quantum register in the little-endian encoding.
///
/// # Output
/// An unsigned integer that contains the measured value of `target`.
operation MeasureBigInt(target : Qubit[]) : BigInt {
    BoolArrayAsBigInt(ResultArrayAsBoolArray(MResetEachZ(target)))
}

/// # Summary
/// Performs a single-qubit measurement in the Pauli Z basis, resetting the `target` to the |0⟩ state after the measurement.
/// Additionally, it checks if the measurement result indicates a loss and returns `true` when a loss is detected. If the qubit
/// is lost, the result value will not be `Zero` or `One` and any use of that result in a comparison will cause a runtime failure.
/// This operation is not supported on all hardware targets.
///
/// # Input
/// ## target
/// A single qubit to be measured.
///
/// # Output
/// A tuple containing the measurement result and a Boolean `true` if the result indicates a loss, otherwise `false`.
///
/// # Remarks
/// This operation is useful for detecting qubit loss during execution. During simulation, qubit loss probability can be
/// configured via the `ConfigureQubitLoss` operation. When compiled to QIR, this uses the `__quantum__rt__read_loss` intrinsic,
/// which may not be supported on all hardware targets and could result in compilation errors when submitting to those targets.
///
/// # See also
/// - [Std.Measurement.IsLossResult](xref:Qdk.Std.Measurement.IsLossResult)
/// - [Std.Diagnostics.ConfigureQubitLoss](xref:Qdk.Std.Diagnostics.ConfigureQubitLoss)
operation MResetZChecked(target : Qubit) : (Result, Bool) {
    let res = MResetZ(target);
    (res, IsLossResult(res))
}

/// # Summary
/// Checks if the measurement result indicates qubit loss. Such measurement results are not `Zero` or `One`
/// and using such a result in comparisons causes a runtime failure.
/// This operation is not supported on all hardware targets.
///
/// # Input
/// ## res
/// The measurement result to check.
///
/// # Output
/// A Boolean value indicating whether the result indicates a loss (`true`) or not (`false`).
///
/// # Remarks
/// This operation is useful for detecting qubit loss during execution. After measurement result from qubit loss
/// cannot be used in a comparison, and any attempt to do so will result in a runtime failure. During simulation, qubit loss
/// probability can be configured via the `ConfigureQubitLoss` operation. When compiled to QIR, this uses the
/// `__quantum__rt__read_loss` intrinsic, which may not be supported on all hardware targets and could result in compilation errors when submitting to those targets.
///
/// # See also
/// - [Std.Measurement.MResetZChecked](xref:Qdk.Std.Measurement.MResetZChecked)
/// - [Std.Diagnostics.ConfigureQubitLoss](xref:Qdk.Std.Diagnostics.ConfigureQubitLoss)
operation IsLossResult(res : Result) : Bool {
    __quantum__rt__read_loss(res)
}

export MeasureAllZ, MeasureEachZ, MResetEachZ, MResetX, MResetY, MResetZ, MeasureInteger, MeasureBigInt, MResetZChecked, IsLossResult;
