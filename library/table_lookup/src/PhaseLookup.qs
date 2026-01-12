// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Diagnostics.*;
import Std.Arrays.*;
import Std.Math.*;
import Std.Convert.*;

import PowerProducts.*;
import Utils.*;

/// # Summary
/// Implements phaseup operation using power products without address split.
operation PhaseLookupViaPP(address : Qubit[], data : Bool[]) : Unit {
    let data_length = Length(data);
    let address_size = Length(address);
    let addressable_space = 1 <<< address_size;
    let data = if (addressable_space < data_length) {
        data[...addressable_space-1]
    } elif (addressable_space > data_length) {
        Padded(-addressable_space, false, data)
    } else {
        data
    };
    use aux_qubits = Qubit[GetAuxCountForPP(address_size)];
    // Transform data from minterm coefficients to polynomial coefficients.
    let corrections = FastMobiusTransform(data);
    let products = ConstructPowerProducts(address, aux_qubits);
    ApplyPhasingViaZ(products, corrections);
    DestructPowerProducts(products);
}

operation ApplyPhasingViaZ(qs : Qubit[], mask : Bool[]) : Unit {
    Fact(Length(mask) > 0, "Mask must be a non-empty array.");
    Fact(Length(mask) == Length(qs) + 1, "Mask row count must match qs length.");

    // Ignore the first element of mask, it affects the global phase.
    ApplyPauliFromBitString(PauliZ, true, Std.Arrays.Rest(mask), qs);
}

/// # Summary
/// Invert phases of `qs` basis states according to the provided boolean array.
/// If `data[i]` is `true`, the phase of |i⟩ gets is inverted (multiplied by -1).
/// Qubit register `qs` is expected to be in little-endian order.
///
/// # Description
/// This operation implements phase lookup using power products and address split.
/// It is a Q# implementation of the "phaseup" operation from the referenced paper.
/// This operation assumes that `Length(data)` matches `2^Length(qs)`.
///
/// # Input
/// ## qs
/// Qubit register whose basis states will have their phases inverted.
///
/// ## data
/// Boolean array indicating which basis states to invert. If `data[i]` is `true`,
/// the phase of |i⟩ gets inverted (multiplied by -1).
///
/// # Reference
/// 1. [arXiv:2505.15917](https://arxiv.org/abs/2505.15917)
///    "How to factor 2048 bit RSA integers with less than a million noisy qubits"
///    by Craig Gidney, May 2025.
operation PhaseLookupViaSplitPP(address : Qubit[], data : Bool[]) : Unit {
    let data_length = Length(data);
    let address_size = Length(address);
    let addressable_space = 1 <<< address_size;
    let data = if (addressable_space < data_length) {
        data[...addressable_space-1]
    } elif (addressable_space > data_length) {
        Padded(-addressable_space, false, data)
    } else {
        data
    };

    Fact(address_size >= 1, "Qubit register must be at least 1.");
    Fact(Length(data) == addressable_space, "Data length must match 2^Length(qs).");
    let n1 = address_size >>> 1; // Number of qubits in the first half
    let n2 = address_size - n1; // Number of qubits in the second half
    let address_low = address[...n1-1]; // Note that address_low will be empty if n == 1.
    let address_high = address[n1...];
    let m1 = 1 <<< n1;
    let m2 = 1 <<< n2;
    Fact(m1 * m2 == addressable_space, "Length of halves must match total length.");

    // Allocate auxilliary qubits
    use aux_qubits1 = Qubit[GetAuxCountForPP(n1)];
    use aux_qubits2 = Qubit[GetAuxCountForPP(n2)];

    // Construct power products for both halves
    let products1 = ConstructPowerProducts(address_low, aux_qubits1);
    let products2 = ConstructPowerProducts(address_high, aux_qubits2);

    // Convert data from minterm to monomial basis using Fast Möbius Transform
    // and chunk it into a matrix
    let mask_as_matrix = Chunks(m1, FastMobiusTransform(data));

    // Apply phasing within each half and between halves
    ApplyPhasingViaZandCZ(products1, products2, mask_as_matrix);

    // Undo power products of both halves
    DestructPowerProducts(products1);
    DestructPowerProducts(products2);
}

/// # Summary
/// Applies phase corrections using Z and CZ gates based on power product coefficients.
/// This is the core quantum operation in the address-split phase lookup algorithm.
///
/// # Description
/// This operation applies conditional phase flips based on a 2D mask that represents
/// power product coefficients after Fast Möbius Transform. The algorithm treats the
/// input qubits as split into two halves, with separate power products for each half.
///
/// The phase correction is applied as follows:
/// 1. Apply Z gates to products2 based on products1[0] (for products from first half only)
/// 2. Apply Z gates to products1 based on products2[0] (for products from second half only)
/// 3. Apply CZ gates between corresponding products from both halves
///
/// # Input
/// ## products1
/// Power product qubits from the first half of the address register.
///
/// ## products2
/// Power product qubits from the second half of the address register.
///
/// ## mask
/// 2D boolean array containing power product coefficients.
/// - `mask[i][j]` indicates whether to apply phase correction for the product
///   of subset i from second half and subset j from first half
///
/// # Remarks
/// The mask is obtained by applying Fast Möbius Transform to phase data
/// and reshaping into a 2D matrix. This allows efficient quantum evaluation of
/// the phase function using O(2^(n/2)) quantum resources instead of O(2^n).
operation ApplyPhasingViaZandCZ(products1 : Qubit[], products2 : Qubit[], mask : Bool[][]) : Unit {
    Fact(Length(mask) > 0, "Mask must be a non-empty array.");
    Fact(Length(mask) == Length(products2) + 1, "Mask row count must match products2 length.");
    Fact(Length(mask[0]) == Length(products1) + 1, "Mask column count must match products1 length.");

    // ColumnAt(0, mask) doesn't correspond to any qubits from the first half,
    // so we can apply Z (rather than CZ) based on mask values.
    ApplyPauliFromBitString(PauliZ, true, Rest(ColumnAt(0, mask)), products2);

    // mask[0] row doesn't correspond to any qubits from the second half,
    // so we can apply Z (rather than CZ) based on mask values.
    ApplyPauliFromBitString(PauliZ, true, Rest(mask[0]), products1);

    // From the second row on, take control from the first half and apply
    // masked multi-target CZ gates via Controlled ApplyPauliFromBitString.
    for row in IndexRange(products1) {
        Controlled ApplyPauliFromBitString(
            [products1[row]],
            (PauliZ, true, Rest(ColumnAt(row + 1, mask)), products2)
        );
    }
}

// =============================
// Tests

@Test()
operation TestPhaseLookupViaPPandZ() : Unit {
    let address_size = 3;
    let data_length = 2^address_size;
    let data_value_count = 2^data_length;

    for i in 0..data_value_count-1 {
        let data = Std.Convert.IntAsBoolArray(i, data_length);
        let same = CheckOperationsAreEqual(
            address_size,
            PhaseLookupViaPP(_, data),
            Multicontrolled.PhaseLookupViaMCX(data, _)
        );
        Fact(same, $"PhaseLookupViaPPandZ must be the same as PhaseLookupViaMCX for {data}.");
    }
}

@Test()
operation TestPhaseLookupViaPPandCZ() : Unit {
    let address_size = 3;
    let data_length = 2^address_size;
    let data_value_count = 2^data_length;

    for i in 0..data_value_count-1 {
        let data = Std.Convert.IntAsBoolArray(i, data_length);
        let same = CheckOperationsAreEqual(
            address_size,
            PhaseLookupViaSplitPP(_, data),
            Multicontrolled.PhaseLookupViaMCX(data, _)
        );
        Fact(same, $"PhaseLookupViaPPandCZ must be the same as PhaseLookupViaMCX for {data}.");
    }
}