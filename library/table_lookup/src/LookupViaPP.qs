// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Arrays.*;
import Std.Diagnostics.*;

import Utils.*;
import PowerProducts.*;

/// # Summary
/// Performs table lookup using power products without register split.
/// # Description
/// Table lookup is preformed using power products constructed from the address qubits.
/// Data is processed using Fast Mobius Transform to fit power products structure.
/// Longer data is ignored, shorter data is padded with false values.
/// Little-endian format is used throughout.
/// This version uses O(2^n) auxiliary qubits for n address qubits.
operation LookupViaPP(
    data : Bool[][],
    address : Qubit[],
    target : Qubit[]
) : Unit {
    let data_length = Length(data);
    let address_size = Length(address);
    let addressable_space = 1 <<< address_size;
    let data = if (addressable_space < data_length) {
        data[...addressable_space-1]
    } elif (addressable_space > data_length) {
        Padded(-addressable_space, Repeated(false, Length(target)), data)
    } else {
        data
    };

    // Allocate auxilliary qubits.
    use aux_qubits = Qubit[GetAuxCountForPP(address_size)];

    // Construct power products.
    let products = ConstructPowerProducts(address, aux_qubits);

    ApplyFlips(data, products, [], target);

    // Undo power products.
    DestructPowerProducts(products);

}

/// # Summary
/// Performs table lookup using power products and register split.
///
/// # Description
/// Table lookup is preformed using power products constructed from the address qubits.
/// Data is processed using Fast Mobius Transform to fit power products structure.
/// Longer data is ignored, shorter data is padded with false values.
/// Little-endian format is used throughout. Address register is split into two halves.
/// This version uses O(2^(n/2)) auxiliary qubits for n address qubits.
operation LookupViaSplitPP(
    data : Bool[][],
    address : Qubit[],
    target : Qubit[]
) : Unit {
    let data_length = Length(data);
    let address_size = Length(address);
    let addressable_space = 1 <<< address_size;
    let data = if (addressable_space < data_length) {
        data[...addressable_space-1]
    } elif (addressable_space > data_length) {
        Padded(-addressable_space, Repeated(false, Length(target)), data)
    } else {
        data
    };

    let m = 2^address_size;
    Fact(address_size >= 1, "Qubit register must be at least 1.");
    Fact(Length(data) == m, "Data length must match 2^Length(qs).");
    let n1 = address_size >>> 1; // Number of qubits in the first half
    let n2 = address_size - n1; // Number of qubits in the second half
    let h1 = address[...n1-1]; // Note that h1 will be empty if address_size == 1
    let h2 = address[n1...];
    let m1 = 1 <<< n1;
    let m2 = 1 <<< n2;
    Fact(m1 * m2 == m, "Length of halves must match total length.");

    // Allocate auxilliary qubits.
    use aux_qubits1 = Qubit[2^n1 - n1 - 1];
    use aux_qubits2 = Qubit[2^n2 - n2 - 1];

    // Construct power products for both halves.
    let products1 = ConstructPowerProducts(h1, aux_qubits1);
    let products2 = ConstructPowerProducts(h2, aux_qubits2);

    ApplyFlips(data, products1, products2, target);

    // Undo power products of both halves.
    DestructPowerProducts(products1);
    DestructPowerProducts(products2);
}

/// # Summary
/// Applies flips to the target register based on the data and power products.
operation ApplyFlips(
    data : Bool[][],
    products1 : Qubit[],
    products2 : Qubit[],
    target : Qubit[]
) : Unit {
    let m1 = Length(products1) + 1;
    let m2 = Length(products2) + 1;

    for bit_index in IndexRange(target) {
        let sourceData = Mapped(a -> a[bit_index], data);
        let flipData = FastMobiusTransform(sourceData);
        let mask_as_matrix = Chunks(m1, flipData);

        // Apply X to target[bit_index] if the empty product (index 0) is set.
        if mask_as_matrix[0][0] {
            X(target[bit_index]);
        }

        for row in 0..m2-2 {
            if (mask_as_matrix[row + 1][0]) {
                CX(products2[row], target[bit_index]);
            }
        }

        for col in 0..m1-2 {
            if (mask_as_matrix[0][col + 1]) {
                CX(products1[col], target[bit_index]);
            }
        }

        for row in 0..m2-2 {
            for col in 0..m1-2 {
                if mask_as_matrix[row + 1][col + 1] {
                    CCNOT(products2[row], products1[col], target[bit_index]);
                }
            }
        }

    }
}

// =============================
// Tests

@Test()
operation CheckLookupViaPP() : Unit {
    let n = 3;
    let data = [[true, false, false], [false, true, false], [false, false, true], [false, false, false], [true, true, false], [false, true, true], [true, false, true], [true, true, true]];

    use addr = Qubit[n];
    use target = Qubit[3];

    // Check that data at all indices is looked up correctly.
    for i in IndexRange(data) {
        ApplyXorInPlace(i, addr);
        LookupViaPP(data, addr, target);

        ApplyPauliFromBitString(PauliX, true, data[i], target);
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {data[i]} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation CheckLookupViaPPShorterData() : Unit {
    let n = 3;
    let width = 3;
    let data = [[true, false, false], [false, true, false], [false, false, true]];

    use addr = Qubit[n];
    use target = Qubit[width];

    // Check that shorter data at all indices is looked up correctly.
    for i in 0..2^n-1 {
        ApplyXorInPlace(i, addr);
        LookupViaPP(data, addr, target);

        mutable expected_data = [false, false, false];
        if i < Length(data) {
            ApplyPauliFromBitString(PauliX, true, data[i], target);
            set expected_data = data[i];
        } else {
            // For out-of-bounds indices, target should remain |0...0⟩.
        }
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {expected_data} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation CheckLookupViaPPLongerData() : Unit {
    let n = 2;
    let width = 3;
    let data = [[true, false, false], [false, true, false], [false, false, true], [false, false, false], [true, true, false], [false, true, true], [true, true, true]];

    use addr = Qubit[n];
    use target = Qubit[width];

    // Check that longer data at all available indices is looked up correctly.
    for i in 0..2^n-1 {
        ApplyXorInPlace(i, addr);
        LookupViaPP(data, addr, target);

        ApplyPauliFromBitString(PauliX, true, data[i], target);
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {data[i]} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation TestLookupViaPPMatchesStd() : Unit {
    let n = 3;
    let width = 4;
    let data = [[true, false, false, false], [false, true, false, false], [false, false, true, false], [false, false, false, false], [true, true, false, false], [false, true, true, false], [true, false, true, true], [true, true, true, true]];

    // Use adjoint Std.TableLookup.Select because this check takes adjoint of that.
    let equal = CheckOperationsAreEqual(
        n + width,
        qs => LookupViaPP(data, qs[0..n-1], qs[n...]),
        qs => Adjoint Std.TableLookup.Select(data, qs[0..n-1], qs[n...])
    );
    Fact(equal, "LookupViaPP should match Std.TableLookup.Select.");
}

@Test()
operation CheckLookupViaSplitPP() : Unit {
    let n = 3;
    let data = [[true, false, false], [false, true, false], [false, false, true], [false, false, false], [true, true, false], [false, true, true], [true, false, true], [true, true, true]];

    use addr = Qubit[n];
    use target = Qubit[3];

    // Check that data at all indices is looked up correctly.
    for i in IndexRange(data) {
        ApplyXorInPlace(i, addr);
        LookupViaSplitPP(data, addr, target);

        ApplyPauliFromBitString(PauliX, true, data[i], target);
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {data[i]} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation CheckLookupViaSplitPPShorterData() : Unit {
    let n = 3;
    let width = 3;
    let data = [[true, false, false], [false, true, false], [false, false, true]];

    use addr = Qubit[n];
    use target = Qubit[width];

    // Check that shorter data at all indices is looked up correctly.
    for i in 0..2^n-1 {
        ApplyXorInPlace(i, addr);
        LookupViaSplitPP(data, addr, target);

        mutable expected_data = [false, false, false];
        if i < Length(data) {
            ApplyPauliFromBitString(PauliX, true, data[i], target);
            set expected_data = data[i];
        } else {
            // For out-of-bounds indices, target should remain |0...0⟩.
        }
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {expected_data} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation CheckLookupViaSplitPPLongerData() : Unit {
    let n = 2;
    let width = 3;
    let data = [[true, false, false], [false, true, false], [false, false, true], [false, false, false], [true, true, false], [false, true, true], [true, true, true]];

    use addr = Qubit[n];
    use target = Qubit[width];

    // Check that longer data at all available indices is looked up correctly.
    for i in 0..2^n-1 {
        ApplyXorInPlace(i, addr);
        LookupViaSplitPP(data, addr, target);

        ApplyPauliFromBitString(PauliX, true, data[i], target);
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {data[i]} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation TestLookupViaSplitPPMatchesStd() : Unit {
    let n = 3;
    let width = 4;
    let data = [[true, false, false, false], [false, true, false, false], [false, false, true, false], [false, false, false, false], [true, true, false, false], [false, true, true, false], [true, false, true, true], [true, true, true, true]];

    // Use adjoint Std.TableLookup.Select because this check takes adjoint of that.
    let equal = CheckOperationsAreEqual(
        n + width,
        qs => LookupViaSplitPP(data, qs[0..n-1], qs[n...]),
        qs => Adjoint Std.TableLookup.Select(data, qs[0..n-1], qs[n...])
    );
    Fact(equal, "LookupViaSplitPP should match Std.TableLookup.Select.");
}
