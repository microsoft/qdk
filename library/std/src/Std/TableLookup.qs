// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Arrays.*;
import Std.Convert.IntAsDouble;
import Std.Convert.ResultArrayAsBoolArray;
import Std.Diagnostics.Fact;
import Std.Math.*;
import Std.Logical.Xor;
import Std.ResourceEstimation.BeginEstimateCaching;
import Std.ResourceEstimation.EndEstimateCaching;

/// # Summary
/// Performs table lookup using power products and register split. Sizes must match.
operation LookupViaPPAndSplit(
    data : Bool[][],
    address : Qubit[],
    target : Qubit[]
) : Unit is Adj {
    // TODO: Implement padding logic if necessary
    body (...) {
        let data_length = Length(data);
        Fact(data_length > 0, "Data cannot be empty");

        let address_size = Length(address);
        let addressable_space = 1 <<< address_size;
        Fact(addressable_space == data_length, "Address space must be equal to data length.");

        if data_length == 1 {
            // Simple case: just write the single data entry to the target
            ApplyPauliFromBitString(PauliX, true, Head(data), target);
        } else {
            let n = Length(address);
            let m = 2^n;
            Fact(n >= 1, "Qubit register must be at least 1.");
            Fact(Length(data) == m, "Data length must match 2^Length(qs).");
            let n1 = n >>> 1; // Number of qubits in the first half
            let n2 = n - n1; // Number of qubits in the second half
            let h1 = address[...n1-1]; // Note that h1 will be empty if n == 1.
            let h2 = address[n1...];
            let m1 = 1 <<< n1;
            let m2 = 1 <<< n2;
            Fact(m1 * m2 == m, "Length of halves must match total length.");

            // Allocate auxilliary qubits
            use aux_qubits1 = Qubit[2^n1 - n1 - 1];
            use aux_qubits2 = Qubit[2^n2 - n2 - 1];

            // Construct power products for both halves
            let products1 = ConstructPowerProducts(h1, aux_qubits1);
            let products2 = ConstructPowerProducts(h2, aux_qubits2);

            ApplyFlips(data, products1, products2, target);

            // Undo power products of both halves
            DestructPowerProducts(products1);
            DestructPowerProducts(products2);

            Fact(Std.Diagnostics.CheckAllZero(aux_qubits1), "Auxiliary1 qubits should be reset to zero after SelectViaPowerProducts");
            Fact(Std.Diagnostics.CheckAllZero(aux_qubits2), "Auxiliary2 qubits should be reset to zero after SelectViaPowerProducts");
        }
    }

    // TODO: Make this non-adjointable and deal with this outside
    adjoint self;
}

operation ApplyFlips(
    data : Bool[][],
    products1 : Qubit[],
    products2 : Qubit[],
    target : Qubit[]
) : Unit {
    let m1 = Length(products1) + 1;
    let m2 = Length(products2) + 1;
    // TODO: If multi-target CNOTs are not available, we can optimize this
    // by moving this loop to be the innermost loop.
    for bit_index in 0..Length(target)-1 {
        let sourceData = Mapped(a -> a[bit_index], data);
        let flipData = FastMobiusTransform(sourceData);
        let mask_as_matrix = Chunks(m1, flipData);

        // Apply X to target[bit_index] if the empty product (index 0) is set
        if mask_as_matrix[0][0] {
            X(target[bit_index]);
        }

        for row in 0..m2-2 {
            if (mask_as_matrix[row+1][0]) {
                CNOT(products2[row], target[bit_index]);
            }
        }

        for col in 0..m1-2 {
            if (mask_as_matrix[0][col+1]) {
                CNOT(products1[col], target[bit_index]);
            }
        }

        for row in 0..m2-2 {
            for col in 0..m1-2 {
                if mask_as_matrix[row+1][col+1] {
                    CCNOT(products2[row], products1[col], target[bit_index]);
                }
            }
        }

    }
}

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
/// respect to the state in `address`. The length of data must be less than
/// 2ⁿ, where 𝑛 is the length of `address`. Each entry in data must have
/// the same length that must be equal to the length of `target`.
/// ## address
/// Address register
/// ## target
/// Zero-initialized target register
///
/// # Remarks
/// The implementation of the SELECT network is based on unary encoding as
/// presented in [1].  The recursive implementation of that algorithm is
/// presented in [3].  The adjoint variant is optimized using a
/// measurement-based unlookup operation [4]. The controlled adjoint variant
/// is not optimized using this technique.
///
/// # References
/// 1. [arXiv:1805.03662](https://arxiv.org/abs/1805.03662)
///    "Encoding Electronic Spectra in Quantum Circuits with Linear T
///    Complexity"
/// 2. [arXiv:1905.07682](https://arxiv.org/abs/1905.07682)
///    "Windowed arithmetic"
/// 3. [arXiv:2211.01133](https://arxiv.org/abs/2211.01133)
///    "Space-time optimized table lookup"
/// 4. [arXiv:2505.15917](https://arxiv.org/abs/2505.15917)
///    "How to factor 2048 bit RSA integers with less than a million noisy qubits"
///    by Craig Gidney, May 2025.
operation Select(
    data : Bool[][],
    address : Qubit[],
    target : Qubit[]
) : Unit is Adj + Ctl {
    body (...) {
        let (N, n) = DimensionsForSelect(data, address);

        if N == 1 {
            // base case
            WriteMemoryContents(Head(data), target);
        } else {
            let (most, tail) = MostAndTail(address[...n - 1]);
            let parts = Partitioned([2^(n - 1)], data);

            within {
                X(tail);
            } apply {
                SinglyControlledSelect(tail, parts[0], most, target);
            }

            SinglyControlledSelect(tail, parts[1], most, target);
        }
    }
    adjoint (...) {
        Unlookup(data, address, target);
    }

    controlled (ctls, ...) {
        let numCtls = Length(ctls);

        if numCtls == 0 {
            Select(data, address, target);
        } elif numCtls == 1 {
            SinglyControlledSelect(ctls[0], data, address, target);
        } else {
            use andChainTarget = Qubit();
            let andChain = MakeAndChain(ctls, andChainTarget);
            use helper = Qubit[andChain::NGarbageQubits];

            within {
                andChain::Apply(helper);
            } apply {
                SinglyControlledSelect(andChainTarget, data, address, target);
            }
        }
    }

    controlled adjoint (ctls, ...) {
        Controlled Select(ctls, (data, address, target));
    }
}

operation SinglyControlledSelect(
    ctl : Qubit,
    data : Bool[][],
    address : Qubit[],
    target : Qubit[]
) : Unit {
    let (N, n) = DimensionsForSelect(data, address);

    if BeginEstimateCaching("Std.TableLookup.SinglyControlledSelect", N) {
        if N == 1 {
            // base case
            Controlled WriteMemoryContents([ctl], (Head(data), target));
        } else {
            use helper = Qubit();

            let (most, tail) = MostAndTail(address[...n - 1]);
            let parts = Partitioned([2^(n - 1)], data);

            within {
                X(tail);
            } apply {
                AND(ctl, tail, helper);
            }

            SinglyControlledSelect(helper, parts[0], most, target);

            CNOT(ctl, helper);

            SinglyControlledSelect(helper, parts[1], most, target);

            Adjoint AND(ctl, tail, helper);
        }

        EndEstimateCaching();
    }
}

function DimensionsForSelect(
    data : Bool[][],
    address : Qubit[]
) : (Int, Int) {
    let N = Length(data);
    Fact(N > 0, "data cannot be empty");

    let n = Ceiling(Lg(IntAsDouble(N)));
    Fact(
        Length(address) >= n,
        $"address register is too small, requires at least {n} qubits"
    );

    return (N, n);
}

operation WriteMemoryContents(
    value : Bool[],
    target : Qubit[]
) : Unit is Adj + Ctl {
    Fact(
        Length(value) == Length(target),
        "number of data bits must equal number of target qubits"
    );

    ApplyPauliFromBitString(PauliX, true, value, target);
}

newtype AndChain = (
    NGarbageQubits : Int,
    Apply : Qubit[] => Unit is Adj
);

function MakeAndChain(ctls : Qubit[], target : Qubit) : AndChain {
    AndChain(
        MaxI(Length(ctls) - 2, 0),
        helper => AndChainOperation(ctls, helper, target)
    )
}

operation AndChainOperation(ctls : Qubit[], helper : Qubit[], target : Qubit) : Unit is Adj {
    let n = Length(ctls);

    Fact(Length(helper) == MaxI(n - 2, 0), "Invalid number of helper qubits");

    if n == 0 {
        X(target);
    } elif n == 1 {
        CNOT(ctls[0], target);
    } else {
        let ctls1 = ctls[0..0] + helper;
        let ctls2 = ctls[1...];
        let tgts = helper + [target];

        for idx in IndexRange(tgts) {
            AND(ctls1[idx], ctls2[idx], tgts[idx]);
        }
    }
}

/// # Summary
/// Performs measurement-based adjoint Select to reset and disentangle target
/// qubits from address qubits. This operation undoes a quantum lookup
/// operation by measuring the target and applying phase corrections
/// to the address register.
///
/// # Description
/// This operation implements the "unlookup" step (adjoint Select), which
/// uncomputes ancilla qubits after a quantum lookup operation. Target qubits are
/// measured and results of measurements are used to correct phases of the
/// address register.
///
/// This operation is typically used after a `Select` operation to clean up the target
/// register while preserving the superposition state of the address register. The
/// measurement-based approach allows efficient uncomputation without requiring the
/// inverse of the original lookup circuit.
///
/// The phase corrections are computed using parity checks between the measurement
/// outcomes and the original data, then applied via the `PhaseLookup` operation.
///
/// # Input
/// ## data
/// 2D boolean array where `data[i]` contains the data that was stored at address `i`.
/// Each `data[i]` should be a boolean array of the same length as the target register.
///
/// ## address
/// Quantum register that was used to address the data during the Select operation.
/// This register may be entangled with the target and needs to be disentangled.
///
/// ## target
/// Quantum register that received the looked-up data and needs to be uncomputed.
/// Will be measured and reset during the uncomputation process.
operation Unlookup(
    data : Bool[][],
    address : Qubit[],
    target : Qubit[]
) : Unit {
    if Length(data) == 1 {
        // Just invert appropriate target qubits.
        // No need for measurement-based uncomputation.
        WriteMemoryContents(data[0], target);
    } else {
        // Check that address size is enough to address all data entries
        let addressBitsNeeded = BitSizeI(Length(data) - 1);
        Fact(Length(address) >= addressBitsNeeded, $"Address size {Length(address)} must be at least {addressBitsNeeded}.");

        // Measure target register in X basis
        let measurements = ResultArrayAsBoolArray(ForEach(MResetX, target));
        // Get phasing data via parity checks
        let phaseData = Mapped(MustBeFixed(measurements, _), data);
        // Pad phase data at the end to cover the entire address space
        let phaseData = Padded(-2^addressBitsNeeded, false, phaseData);

        // Apply phase lookup to correct phases in the address register
        PhaseLookup(address, phaseData);
    }
}

// Checks whether specific bit string `data` must be fixed for a given
// measurement result `result`.
//
// Returns true if the number of indices for which both result and data are
// `true` is odd.
function MustBeFixed(result : Bool[], data : Bool[]) : Bool {
    mutable state = false;
    for i in IndexRange(result) {
        set state = state != (result[i] and data[i]);
    }
    state
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
operation PhaseLookup(qs : Qubit[], data : Bool[]) : Unit {
    let n = Length(qs);
    let m = 2^n;
    Fact(n >= 1, "Qubit register must be at least 1.");
    Fact(Length(data) == m, "Data length must match 2^Length(qs).");
    let n1 = n >>> 1; // Number of qubits in the first half
    let n2 = n - n1; // Number of qubits in the second half
    let h1 = qs[...n1-1]; // Note that h1 will be empty if n == 1.
    let h2 = qs[n1...];
    let m1 = 1 <<< n1;
    let m2 = 1 <<< n2;
    Fact(m1 * m2 == m, "Length of halves must match total length.");

    // Allocate auxilliary qubits
    use aux_qubits1 = Qubit[2^n1 - n1 - 1];
    use aux_qubits2 = Qubit[2^n2 - n2 - 1];

    // Construct power products for both halves
    let products1 = ConstructPowerProducts(h1, aux_qubits1);
    let products2 = ConstructPowerProducts(h2, aux_qubits2);

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
/// Constructs power products - AND-ed subsets of qubits from the input register `qs`.
/// `2^Length(qs) - 1` qubits corresponding to non-empty subsets of `qs` are placed into the result array.
///
/// # Description
/// Resulting subsets correspond to an integer index that runs from `1` to `(2^Length(qs))-1`.
/// (Since the empty set (index 0) is not included in the result, actual array indexes should be shifted.)
/// Indexes are treated as bitmasks indicating if a particular qubit is included.
/// Bitmasks `2^i` includes only qubit `qs[i]`, which is placed into the resulting array at that index minus 1.
/// Bitmasks with more than one bit set correspond to subsets with multiple qubits from `qs`.
/// Qubits for these masks are taken from aux_qubits register and their value is set using AND gates.
/// Note:
///     1. Empty set is not included in the result.
///     2. For sets that only contain one qubit, the input qubits are reused.
///
/// # Alt summary
/// Takes a register of qubits and returns "power products" - qubits corresponding to all non-empty subsets
/// of the qubits from the input register: each power product qubit state is a result of AND operation
/// for the qubits in corresponding subset.
operation ConstructPowerProducts(qubits : Qubit[], aux_qubits : Qubit[]) : Qubit[] {
    // Start with empty array - no dummy qubit for empty set
    mutable power_products = [];
    // Index to take next free qubit from aux_qubits array.
    mutable next_available = 0;
    // Consider every index in the input qubit register
    for qubit_index in 0..Length(qubits)-1 {
        // First, add the set that consists of only one qubit at index qubit_index.
        power_products += qubits[qubit_index..qubit_index];
        // Then, construct and add sets that include this new qubit as the last one.
        for existing_set_index in 0..Length(power_products)-2 {
            // Take the next qubit for the new set
            let next_power_product = aux_qubits[next_available];
            next_available += 1;
            // Create appropriate set and add it to the result
            AND(power_products[existing_set_index], qubits[qubit_index], next_power_product);
            power_products += [next_power_product];
        }
    }
    Fact(next_available == Length(aux_qubits), "All auxilliary qubits should be used.");
    return power_products;
}

/// # Summary
/// Undo construction of power products done by `ConstructPowerProducts`
/// Pass array returned by `ConstructPowerProducts` to this function
/// to reset auxiliary qubits used to hold power products back to |0> state.
///
/// # Description
/// `products` array has no qubit that corresponds to an empty product (=1).
/// All entries at indexes `2^i - 1` contain original qubits.
/// Qubits from `2^i - 1` to `2^(i+1) - 2` represent power products that
/// end in original qubit at `2^i - 1`.
/// To undo power products this function goes over original qubits backwards.
/// Then measures out qubits from `2^i - 1` to `2^(i+1) - 2` in X basis,
/// targeting corresponding qubits from 0 to `2^i - 2` in CZ gates if necessary.
operation DestructPowerProducts(products : Qubit[]) : Unit {
    let len = Length(products);
    if len <= 1 {
        // Nothing to undo - this was one of the source qubits.
        return ();
    }
    // For no-dummy version, length is 2^n - 1, so we need to work with 2^n
    let extended_len = len + 1;
    Fact((extended_len &&& (extended_len-1)) == 0, "Length + 1 of a qubit register should be a power of 2");

    // At index h-1 a source qubit is located (shifted by 1 compared to original version).
    // To the right are all power products ending in it.
    // We are going backwards over all original qubits.
    mutable h = extended_len / 2;
    // If h is 1 we have nothing else to undo.
    while h > 1 {
        // Go over all sets that end in original qubit currently at index h-1.
        // NOTE: k starts from 0 since there's no dummy qubit.
        // NOTE: The order of targets here doesn't matter.
        for k in 0..h-2 {
            // Measure and reset the qubit that represents the set (h-1) | k.
            // In the no-dummy version, this is at index h-1+k+1 = h+k
            if MResetX(products[h + k]) == One {
                // If we measure 1, qubit representing set k needs to be included in targets.
                CZ(products[h - 1], products[k]);
            }
        }
        // Done with qubit at index h-1. Go to next original qubit.
        h = h / 2;
    }
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
        for j in 0..step * 2..len-1 {
            for k in 0..step-1 {
                // XOR the "upper" position with the "lower" position
                result[j + k + step] = Xor(result[j + k + step], result[j + k]);
            }
        }
    }
    return result;
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
    for row in 0..Length(products1)-1 {
        Controlled ApplyPauliFromBitString(
            [products1[row]],
            (PauliZ, true, Rest(ColumnAt(row + 1, mask)), products2)
        );
    }
}

export Select;
export LookupViaPPAndSplit;

