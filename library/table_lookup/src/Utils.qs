// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Diagnostics.*;
import Std.Math.*;
import Std.Arrays.*;
import Std.Convert.*;

import Lookup.*;

struct AddressAndData {
    // Lower part of the address needed to index into data.
    fitAddress : Qubit[],
    // Data padded or trimmed to fit needed address space.
    fitData : Bool[][],
}

function PrepareAddressAndData(
    options : LookupOptions,
    address : Qubit[],
    data : Bool[][]
) : AddressAndData {
    let address_size = Length(address);
    let address_space = 1 <<< address_size;
    let data_length = Length(data);

    if (data_length == address_space) {
        // Data length match address space, nothing to adjust.
        return new AddressAndData {
            fitAddress = address,
            fitData = data,
        };
    }

    if (data_length > address_space) {
        // Truncate longer data if needed.
        if options.failOnLongData {
            fail $"Data length {data_length} exceeds address space {address_space}.";
        }
        return new AddressAndData {
            fitAddress = address,
            fitData = data[...address_space-1]
        };
    }

    // Data is shorter than addressable space. Truncate excessive address if needed.

    if (not options.failOnShortData) {
        fail $"Data length {data_length} is shorter than address space {address_space}.";
    }

    if (options.respectExcessiveAddress) {
        return new AddressAndData {
            fitAddress = address,
            fitData = data,
        };
    }

    if (data_length <= 1) {
        // No address qubits are needed for data length 0.
        // Case data_length == 1 is for compatibility with earlier behavior.
        return new AddressAndData {
            fitAddress = [],
            fitData = data,
        };
    }

    let address_size_needed = BitSizeI(data_length - 1);
    Fact(address_size_needed <= address_size, "Internal error: address_size_needed should be at most address_size.");

    let address_space_needed = 1 <<< address_size_needed;
    Fact(address_space_needed >= data_length, "Internal error: address_space_needed should be at least data_length.");

    return new AddressAndData {
        // Trim address qubits to needed size.
        fitAddress = address[...address_size_needed - 1],
        // Shorter data in this case will be handled later.
        fitData = data,
    };
}

/// # Summary
/// Computes the Fast Möbius Transform of a boolean array over GF(2).
/// Also known as the Walsh-Hadamard Transform or subset sum transform.
///
/// # Description
/// This transform converts minterm coefficients to monomial coefficients.
/// For each position i in the result, it computes the XOR (sum over GF(2)) of all
/// input elements at positions that are subsets of i (when i is interpreted as a bitmask).
///
/// This is equivalent to multiplying the input vector by a triangular matrix
/// where entry (i,j) is 1 if j is a subset of i (as bitmasks), and 0 otherwise.
///
/// # Input
/// ## qs
/// Boolean array of minterm coefficients of length 2^n for some integer n ≥ 0.
///
/// # Output
/// Boolean array of the same length as input containing monomial coefficients.
///
/// # Remarks
/// This function is the classical preprocessing step for quantum phase lookup operations,
/// converting phase data from standard basis coefficients to power product coefficients.
/// The transformation is its own inverse when applied twice.
function FastMobiusTransform(qs : Bool[]) : Bool[] {
    let len = Length(qs);
    Fact((len &&& (len-1)) == 0, "Length of a qubit register should be a power of 2");
    let n = BitSizeI(len)-1;

    mutable result = qs;
    // For each bit position (from least to most significant)
    for i in 0..n-1 {
        let step = 2^i;
        // For each pair of positions that differ only in that bit
        for j in 0..(step * 2)..len-1 {
            for k in 0..step-1 {
                // XOR the "upper" position with the "lower" position
                result[j + k + step] = Std.Logical.Xor(result[j + k + step], result[j + k]);
            }
        }
    }
    return result;
}

/// # Summary
/// Measures all qubits in the given register in the X basis. And resets them to |0>.
operation MeasureAndComputePhaseData(target : Qubit[], data : Bool[][], size : Int) : Bool[] {
    // Measure target register in X basis
    let measurements = ResultArrayAsBoolArray(ForEach(MResetX, target));
    // Get phasing data via parity checks
    let phaseData = Mapped(BinaryInnerProduct(measurements, _), data);
    // Pad phase data at the end to cover the entire address space
    Padded(-2^size, false, phaseData)
}

/// # Summary
/// Computes dot (inner) product of two vectors over GF(2) field.
/// This isn't a proper inner product as it is not positive-definite.
///
/// It is used to see if a phase correction is needed for a bit string `data`
/// after obtaining a measurement result `measurements`.
function BinaryInnerProduct(data : Bool[], measurements : Bool[]) : Bool {
    mutable sum = false;
    for i in IndexRange(measurements) {
        set sum = Std.Logical.Xor(sum, (data[i] and measurements[i]));
    }
    sum
}

/// # Summary
/// Combines multiple control qubits into a single control qubit using auxiliary qubits.
/// Logarithmic depth and linear number of auxiliary qubits are used.
operation CombineControls(controls : Qubit[], aux : Qubit[]) : Unit is Adj {
    Fact(Length(controls) >= 1, "CombineControls: controls must not be empty.");
    Fact(Length(controls) == Length(aux) + 1, "CombineControls: control and aux length mismatch.");
    let combined = controls + aux;
    let aux_offset = Length(controls);
    for i in 0..aux_offset-2 {
        AND(combined[i * 2], combined[i * 2 + 1], combined[aux_offset + i]);
    }
}

/// # Summary
/// Retrieves the combined control qubit after CombineControls operation.
function GetCombinedControl(controls : Qubit[], aux : Qubit[]) : Qubit {
    Fact(Length(controls) >= 1, "GetCombinedControl: controls must not be empty.");
    Fact(Length(controls) == Length(aux) + 1, "GetCombinedControl: control and aux length mismatch.");
    if Length(controls) == 1 {
        return Head(controls);
    } else {
        return Tail(aux);
    }
}

// =============================
// Tests

@Test()
function TestFastMobiusTransform() : Unit {
    // Test cases for FastMobiusTransform
    let testCases = [
        ([], []),
        ([false], [false]),
        ([true], [true]),
        ([false, false], [false, false]),
        ([false, true], [false, true]),
        ([true, false], [true, true]),
        ([true, true], [true, false]),
        ([false, false, false, false], [false, false, false, false]),
        ([false, false, false, true], [false, false, false, true]),
        ([false, false, true, false], [false, false, true, true]),
        ([false, false, true, true], [false, false, true, false]),
        ([true, true, true, true], [true, false, false, false]),
        ([true, false, false, false], [true, true, true, true]),
    ];
    for (input, expected) in testCases {
        let output = FastMobiusTransform(input);
        Fact(output == expected, $"FastMobiusTransform({input}) should be {expected}, got {output}");
        // Test that applying the transform twice returns the original input
        let roundTrip = FastMobiusTransform(output);
        Fact(roundTrip == input, $"FastMobiusTransform(FastMobiusTransform({input})) should be {input}, got {roundTrip}");
    }
}

internal operation TestCombineControlsForN(n : Int) : Unit {
    let all_ones = 2^n - 1;
    use controls = Qubit[n];
    use aux = Qubit[n - 1];

    // Test all combinations of control qubits
    for i in 0..all_ones {
        ApplyXorInPlace(i, controls);

        // Combine controls
        within {
            CombineControls(controls, aux);
        } apply {
            let combined = GetCombinedControl(controls, aux);
            // Check that combined control is |1> iff all controls are |1>
            if i == all_ones {
                within {
                    // Ensure combined control is |1>
                    X(combined);
                } apply {
                    Fact(CheckZero(combined), $"Combined control should be |1> when all {n} controls are |1>.");
                }
            } else {
                Fact(CheckZero(combined), $"Combined control should be |0> when some of {n} controls are |0>.");
            }

        }
        ApplyXorInPlace(i, controls);
        // Check that all qubits are reset to |0>
        Fact(CheckAllZero(controls + aux), "All qubits should be reset to |0> after CombineControls adjoint.");
    }
}

@Test()
operation TestCombineControls() : Unit {
    TestCombineControlsForN(1);
    TestCombineControlsForN(4);
}
