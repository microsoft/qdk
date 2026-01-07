// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Simple implementations of lookup operations using multicontrolled X gates.
// Data shorter or longer than addressable space is allowed:
// Longer data is ignored.
// Little-endian format is used throughout.

import Std.Diagnostics.*;
import Std.Math.*;

// Lookup of a single bit using multicontrolled X gates.
operation BitLookupViaMCX(data : Bool[], address : Qubit[], target : Qubit) : Unit is Adj + Ctl {
    let address_size = Length(address);
    let address_space = 2^address_size;
    let data_length = Length(data);
    for basis_vector in 0..MinI(data_length, address_space)-1 {
        if data[basis_vector] {
            within {
                // Invert address qubits for 0-es in basis_vector
                ApplyXorInPlace(address_space-1-basis_vector, address)
            } apply {
                Controlled X(address, target);
            }
        }
    }
}

// Phase lookup of a single bit using multicontrolled X gates.
operation PhaseLookupViaMCX(data : Bool[], address : Qubit[]) : Unit is Adj + Ctl {
    use aux = Qubit();
    within {
        X(aux);
        H(aux);
    } apply {
        BitLookupViaMCX(data, address, aux);
    }
}

// Lookup of mult-bit register using multicontrolled X gates.
operation LookupViaMCX(data : Bool[][], address : Qubit[], target : Qubit[]) : Unit is Adj + Ctl {
    let address_size = Length(address);
    let address_space = 2^address_size;
    let data_length = Length(data);
    let target_size = Length(target);
    for basis_vector in 0..MinI(data_length, address_space)-1 {
        let data_vector = data[basis_vector];
        Fact(Length(data_vector) == target_size, $"Data vector length {Length(data_vector)} must match target size {target_size}.");
        within {
            // Invert address qubits for 0-es in basis_vector
            ApplyXorInPlace(address_space-1-basis_vector, address)
        } apply {
            Controlled ApplyPauliFromBitString(address, (PauliX, true, data_vector, target));
        }
    }
}

// =============================
// Tests

@Test()
operation CheckLookupViaMCX() : Unit {
    let n = 3;
    let data =
        [[true, false, false],
        [false, true, false],
        [false, false, true],
        [false, false, false],
        [true, true, false],
        [false, true, true],
        [true, false, true],
        [true, true, true]];

    use addr = Qubit[n];
    use target = Qubit[3];

    // Check that data at all indices is looked up correctly.
    for i in 0..Length(data)-1 {
        ApplyXorInPlace(i, addr);
        LookupViaMCX(data, addr, target);

        ApplyPauliFromBitString(PauliX, true, data[i], target);
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {data[i]} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation CheckLookupViaMCXShorterData() : Unit {
    let n = 3;
    let width = 3;
    let data =
        [[true, false, false],
        [false, true, false],
        [false, false, true]];

    use addr = Qubit[n];
    use target = Qubit[width];

    // Check that shorter data at all indices is looked up correctly.
    for i in 0..2^n-1 {
        ApplyXorInPlace(i, addr);
        LookupViaMCX(data, addr, target);

        mutable expected_data = [false, false, false];
        if i < Length(data) {
            ApplyPauliFromBitString(PauliX, true, data[i], target);
            set expected_data = data[i];
        } else {
            // For out-of-bounds indices, target should remain |0...0>
        }
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match { expected_data } at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation CheckLookupViaMCXLongerData() : Unit {
    let n = 2;
    let width = 3;
    let data =
        [[true, false, false],
        [false, true, false],
        [false, false, true],
        [false, false, false],
        [true, true, false],
        [false, true, true],
        [true, true, true]];

    use addr = Qubit[n];
    use target = Qubit[width];

    // Check that longer data at all available indices is looked up correctly.
    for i in 0..2^n-1 {
        ApplyXorInPlace(i, addr);
        LookupViaMCX(data, addr, target);

        ApplyPauliFromBitString(PauliX, true, data[i], target);
        let zero = CheckAllZero(target);
        Fact(zero, $"Target should match {data[i]} at index {i}.");
        ResetAll(addr);
    }
}

@Test()
operation CheckBitLookupViaMCX() : Unit {
    let n = 4;
    let data =
        [true, false, true, false,
        false, false, false, false,
        false, false, false, false,
        false, false, true, true];

    use addr = Qubit[n];
    use target = Qubit();

    // Check that data at all indices is looked up correctly.
    for i in 0..Length(data)-1 {
        ApplyXorInPlace(i, addr);
        BitLookupViaMCX(data, addr, target);

        let value = Std.Convert.ResultAsBool(MResetZ(target));
        ResetAll(addr);

        Fact(value == data[i], $"Target qubit measurement mismatch at index {i}.");
    }
}

@Test()
operation TestPhaseLookupViaMCX() : Unit {
    let n = 4;
    let data =
        [true, false, true, false,
        false, false, false, false,
        false, false, false, false,
        false, false, true, true];
    let coeffs =
        [-0.25, 0.25, -0.25, 0.25,
        0.25, 0.25, 0.25, 0.25,
        0.25, 0.25, 0.25, 0.25,
        0.25, 0.25, -0.25, -0.25];

    use qs = Qubit[n];
    ApplyToEach(H, qs);

    // `Reversed` to match big-endian state preparation coefficients order
    PhaseLookupViaMCX(data, Std.Arrays.Reversed(qs));
    Adjoint Std.StatePreparation.PreparePureStateD(coeffs, qs);

    Fact(CheckAllZero(qs), "All qubits should be back to |0> state.");
}

@Test()
operation TestBitLookupViaMCXMatchesStd(): Unit {
    let n = 4;
    let data =
        [true, false, true, false,
        false, false, false, false,
        true, false, false, true,
        false, false, true, true];
    let select_data = Std.Arrays.Mapped(x -> [x], data);

    // Use adjoint Std.TableLookup.Select because this check takes adjoint of that.
    let equal = CheckOperationsAreEqual(
        n+1,
        qs => BitLookupViaMCX(data, qs[0..n-1], qs[n]),
        qs => Adjoint Std.TableLookup.Select(select_data, qs[0..n-1], qs[n..n])
    );
    Fact(equal, "BitLookupViaMCX should match Std.TableLookup.Select.");
}

@Test()
operation TestLookupViaMCXMatchesStd(): Unit {
    let n = 3;
    let width = 4;
    let data =
        [[true, false, false, false],
        [false, true, false, false],
        [false, false, true, false],
        [false, false, false, false],
        [true, true, false, false],
        [false, true, true, false],
        [true, false, true, true],
        [true, true, true, true]];

    use addr = Qubit[n];
    use target = Qubit[width];

    // Use adjoint Std.TableLookup.Select because this check takes adjoint of that.
    let equal = CheckOperationsAreEqual(
        n + width,
        qs => LookupViaMCX(data, qs[0..n-1], qs[n...]),
        qs => Adjoint Std.TableLookup.Select(data, qs[0..n-1], qs[n...])
    );
    Fact(equal, "LookupViaMCX should match Std.TableLookup.Select.");
}
