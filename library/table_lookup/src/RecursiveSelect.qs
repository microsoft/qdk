// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Arrays.*;
import Std.Diagnostics.*;
import Std.Math.*;
import Std.Convert.*;


/// # Summary
/// Performs table lookup using a SELECT network
///
/// # Description
/// Assuming a zero-initialized `target` register, this operation will
/// initialize it with the bitstrings in `data` at indices according to the
/// computational values of the `address` register.
///
/// # Input
/// ## data
/// The classical table lookup data which is prepared in `target` with
/// respect to the state in `address`. Each entry in data must have
/// the same length that must be equal to the length of `target`.
/// ## address
/// Address register
/// ## target
/// Zero-initialized target register
///
/// # Remarks
/// The implementation of the SELECT network is based on unary encoding as
/// presented in [1]. The recursive implementation of that algorithm is
/// presented in [2].
///
/// # References
/// 1. [arXiv:1805.03662](https://arxiv.org/abs/1805.03662)
///    "Encoding Electronic Spectra in Quantum Circuits with Linear T
///    Complexity"
/// 2. [arXiv:2211.01133](https://arxiv.org/abs/2211.01133)
///    "Space-time optimized table lookup"
operation RecursiveLookup(
    data : Bool[][],
    address : Qubit[],
    target : Qubit[]
) : Unit {
    let data_length = Length(data);
    if data_length == 0 {
        return ();
    }
    let address_size = Length(address);
    if address_size == 0 {
        ApplyPauliFromBitString(PauliX, true, data[0], target);
        return ();
    }
    let addressable_space = 1 <<< address_size;
    let data_length = MinI(data_length, addressable_space);
    let data = data[...data_length - 1];
    let highest_address_qubit = Tail(address);
    let lower_address_qubits = Most(address);
    let parts = Partitioned([MinI(addressable_space / 2, data_length)], data);

    within {
        X(highest_address_qubit);
    } apply {
        ControlledRecursiveSelect(highest_address_qubit, parts[0], lower_address_qubits, target);
    }
    ControlledRecursiveSelect(highest_address_qubit, parts[1], lower_address_qubits, target);
}

operation RecursiveLookupOpt(
    data : Bool[][],
    address : Qubit[],
    target : Qubit[]
) : Unit {
    let data_length = Length(data);
    if data_length == 0 {
        return ();
    }
    if data_length == 1 {
        ApplyPauliFromBitString(PauliX, true, data[0], target);
        return ();
    }
    let addressable_space = 1 <<< Length(address);
    if addressable_space == 1 {
        return ();
    }

    let data_length = MinI(data_length, addressable_space);
    let data = data[...data_length - 1];
    let address_size_needed = BitSizeI(data_length - 1);
    let (lower_address_qubits, highest_address_qubit) = MostAndTail(address[...address_size_needed - 1]);
    let parts = Partitioned([2^(address_size_needed - 1)], data);

    within {
        X(highest_address_qubit);
    } apply {
        ControlledRecursiveSelectOpt(highest_address_qubit, parts[0], lower_address_qubits, target);
    }
    ControlledRecursiveSelectOpt(highest_address_qubit, parts[1], lower_address_qubits, target);
}

// Complete version of recursive select network that ignores address values
// beyond data length. This is equivalent to padding data with false values
// to cover entire addressable space.
// If data length is 1, single data value is used only if address is zero.
operation ControlledRecursiveSelect(
    control : Qubit,
    data : Bool[][],
    address : Qubit[],
    target : Qubit[]
) : Unit {
    let data_length = Length(data);
    if (data_length == 0) {
        // If there's no data, there's nothing to do.
        return ();
    }

    let address_size = Length(address);
    if address_size == 0 {
        // Base case. Use CX on qubits where data is true.
        Fact(data_length == 1, "Data length must be 1 when address size is 0.");
        Controlled ApplyPauliFromBitString([control], (PauliX, true, data[0], target));
        return ();
    }

    let address_space = 1 <<< address_size;
    Fact(data_length <= address_space, "Data length must not exceed addressable space.");

    let highest_address_qubit = Tail(address);
    let lower_address_qubits = Most(address);
    let data_parts = Partitioned([address_space / 2], data);

    use aux = Qubit();
    within {
        X(highest_address_qubit);
    } apply {
        AND(control, highest_address_qubit, aux);
    }
    ControlledRecursiveSelect(aux, data_parts[0], lower_address_qubits, target);
    CNOT(control, aux);
    ControlledRecursiveSelect(aux, data_parts[1], lower_address_qubits, target);
    Adjoint AND(control, highest_address_qubit, aux);
}

// Optimized version of recursive select network that expects all address values
// to be within data length. If address value exceeds data length, behavior is undefined.
// If data length is 1, single data value is always used.
operation ControlledRecursiveSelectOpt(
    control : Qubit,
    data : Bool[][],
    address : Qubit[],
    target : Qubit[]
) : Unit {
    let data_length = Length(data);
    Fact(data_length > 0, "ControlledRecursiveSelectOpt: Data cannot be empty.");

    let address_size_needed = BitSizeI(data_length - 1);
    Fact(Length(address) >= address_size_needed, "ControlledRecursiveSelectOpt: Address register is too short.");

    if data_length == 1 {
        // Base case: always apply data value if data length is 1.
        Controlled ApplyPauliFromBitString([control], (PauliX, true, data[0], target));
    } else {
        use helper = Qubit();

        // Get just enough address qubits to address all data and split data.
        let (lower_address_qubits, highest_address_qubit) = MostAndTail(address[...address_size_needed - 1]);
        let parts = Partitioned([1 <<< (address_size_needed - 1)], data);

        within {
            X(highest_address_qubit);
        } apply {
            AND(control, highest_address_qubit, helper);
        }
        ControlledRecursiveSelectOpt(helper, parts[0], lower_address_qubits, target);
        CNOT(control, helper);
        ControlledRecursiveSelectOpt(helper, parts[1], lower_address_qubits, target);
        Adjoint AND(control, highest_address_qubit, helper);
    }

}


// =============================
// Tests

@Test()
operation CheckRecursiveLookup() : Unit {
    let n = 3;
    let data = [[true, false, false], [false, true, false], [false, false, true], [false, false, false], [true, true, false], [false, true, true], [true, false, true], [true, true, true]];

    use addr = Qubit[n];
    use target = Qubit[3];

    // Check that data at all indices is looked up correctly.
    for i in 0..Length(data)-1 {
        ApplyXorInPlace(i, addr);
        RecursiveLookup(data, addr, target);

        ApplyPauliFromBitString(PauliX, true, data[i], target);
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {data[i]} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation CheckRecursiveLookupOpt() : Unit {
    let n = 3;
    let data = [[true, false, false], [false, true, false], [false, false, true], [false, false, false], [true, true, false], [false, true, true], [true, false, true], [true, true, true]];

    use addr = Qubit[n];
    use target = Qubit[3];

    // Check that data at all indices is looked up correctly.
    for i in 0..Length(data)-1 {
        ApplyXorInPlace(i, addr);
        RecursiveLookupOpt(data, addr, target);

        ApplyPauliFromBitString(PauliX, true, data[i], target);
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {data[i]} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation CheckRecursiveLookupShorterData() : Unit {
    let n = 3;
    let width = 3;
    let data = [[true, false, false], [false, true, false], [false, false, true]];

    use addr = Qubit[n];
    use target = Qubit[width];

    // Check that shorter data at all indices is looked up correctly.
    // This works for all addresses even beyond data length.
    for i in 0..2^n-1 {
        ApplyXorInPlace(i, addr);
        RecursiveLookup(data, addr, target);

        mutable expected_data = [false, false, false];
        if i < Length(data) {
            ApplyPauliFromBitString(PauliX, true, data[i], target);
            set expected_data = data[i];
        } else {
            // For out-of-bounds indices, target should remain |0...0>
        }
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {expected_data} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation CheckRecursiveLookupShorterDataOpt() : Unit {
    let n = 3;
    let width = 3;
    let data = [[true, false, false], [false, true, false], [false, false, true]];

    use addr = Qubit[n];
    use target = Qubit[width];

    // Check that shorter data at all indices is looked up correctly.
    // This only works up to data length.
    for i in 0..Length(data)-1 {
        ApplyXorInPlace(i, addr);
        RecursiveLookupOpt(data, addr, target);

        ApplyPauliFromBitString(PauliX, true, data[i], target);
        let expected_data = data[i];
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {expected_data} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation CheckRecursiveLookupLongerData() : Unit {
    let n = 2;
    let width = 3;
    let data = [[true, false, false], [false, true, false], [false, false, true], [false, false, false], [true, true, false], [false, true, true], [true, true, true]];

    use addr = Qubit[n];
    use target = Qubit[width];

    // Check that longer data at all available indices is looked up correctly.
    for i in 0..2^n-1 {
        ApplyXorInPlace(i, addr);
        RecursiveLookup(data, addr, target);

        ApplyPauliFromBitString(PauliX, true, data[i], target);
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {data[i]} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation CheckRecursiveLookupLongerDataOpt() : Unit {
    let n = 2;
    let width = 3;
    let data = [[true, false, false], [false, true, false], [false, false, true], [false, false, false], [true, true, false], [false, true, true], [true, true, true]];

    use addr = Qubit[n];
    use target = Qubit[width];

    // Check that longer data at all available indices is looked up correctly.
    for i in 0..2^n-1 {
        ApplyXorInPlace(i, addr);
        RecursiveLookupOpt(data, addr, target);

        ApplyPauliFromBitString(PauliX, true, data[i], target);
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {data[i]} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation TestRecursiveLookupMatchesStd() : Unit {
    let n = 3;
    let width = 4;
    let data = [[true, false, false, false], [false, true, false, false], [false, false, true, false], [false, false, false, false], [true, true, false, false], [false, true, true, false], [true, false, true, true], [true, true, true, true]];

    use addr = Qubit[n];
    use target = Qubit[width];

    // Use adjoint Std.TableLookup.Select because this check takes adjoint of that.
    let equal = CheckOperationsAreEqual(
        n + width,
        qs => RecursiveLookup(data, qs[0..n-1], qs[n...]),
        qs => Adjoint Std.TableLookup.Select(data, qs[0..n-1], qs[n...])
    );
    Fact(equal, "RecursiveLookup should match Std.TableLookup.Select.");
}

@Test()
operation TestRecursiveLookupMatchesStdOpt() : Unit {
    let n = 3;
    let width = 4;
    let data = [[true, false, false, false], [false, true, false, false], [false, false, true, false], [false, false, false, false], [true, true, false, false], [false, true, true, false], [true, false, true, true], [true, true, true, true]];

    use addr = Qubit[n];
    use target = Qubit[width];

    // Use adjoint Std.TableLookup.Select because this check takes adjoint of that.
    let equal = CheckOperationsAreEqual(
        n + width,
        qs => RecursiveLookupOpt(data, qs[0..n-1], qs[n...]),
        qs => Adjoint Std.TableLookup.Select(data, qs[0..n-1], qs[n...])
    );
    Fact(equal, "RecursiveLookupOpt should match Std.TableLookup.Select.");
}
