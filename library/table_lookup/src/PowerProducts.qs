// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Diagnostics.*;

/// # Summary
/// Constructs power products - AND-ed subsets of qubits from the input register `qs`.
/// `2^Length(qs) - 1` qubits corresponding to non-empty subsets of `qs` are placed into the result array.
///
/// # Description
/// Resulting subsets correspond to an integer index that runs from `1` to `(2^Length(qs))-1`.
/// (Since the empty set (index 0) is not included in the result, actual array indexes should be shifted.)
/// Indexes are treated as bitmasks indicating if a particular qubit is included.
/// Bitmasks `2^i` includes only qubit `qs[i]`, which is placed into the resulting array at index 2^i - 1.
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
    Fact(next_available == Length(aux_qubits), "ConstructPowerProducts: All auxilliary qubits should be used.");
    return power_products;
}

/// # Summary
/// Undo construction of power products done by `ConstructPowerProducts`
/// Pass array returned by `ConstructPowerProducts` to this function
/// to reset auxiliary qubits used to hold power products back to |0> state.
///
/// # Description
/// `products` array has no qubit that corresponds to an empty product (≡1).
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
    Fact((extended_len &&& (extended_len-1)) == 0, "DestructPowerProducts: Length + 1 of a qubit register should be a power of 2");

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
            // Measure and reset the qubit that represents
            // the set (h-1) | k, which is at index h-1+k+1 = h+k
            if MResetX(products[h + k]) == One {
                // If we measure 1, qubit representing set k needs to be included in targets.
                CZ(products[h - 1], products[k]);
            }
        }
        // Done with qubit at index h-1. Go to next original qubit.
        h = h / 2;
    }
}

function GetAuxCountForPP(nQubits : Int) : Int {
    Fact(nQubits >= 0, "Number of qubits for power product construction must be non-negative.");
    // Number of power products is 2^n - 1 (this excludes the empty product).
    // Number of original qubits is n.
    // Aux qubits needed is (2^n - 1) - n = 2^n - n - 1.
    (1 <<< nQubits) - nQubits - 1
}

// =============================
// Tests

internal operation ConstructDestructPowerProducts(qs : Qubit[]) : Unit {
    // For monomials with more than one variable we need auxilliary qubits
    use aux_qubits = Qubit[GetAuxCountForPP(Length(qs))];

    // Construct/destruct should leave qs unchanged.
    let products = ConstructPowerProducts(qs, aux_qubits);
    DestructPowerProducts(products);
}

@Test()
operation TestCreateDestructPowerProducts() : Unit {
    // Check that construction and destruction of power products does not affect the register.
    for i in 0..5 {
        let success = CheckOperationsAreEqual(
            i,
            qs => ConstructDestructPowerProducts(qs),
            qs => {}
        );
        Fact(success, $"Construction/Destruction of power products must be identity for {i} qubits.");
    }
}

internal operation CheckPowerProducts(nQubits : Int, address_value : Int) : Unit {
    // Prepare qubit register.
    Fact(nQubits >= 0, "Number of qubits must be non-negative.");
    use qs = Qubit[nQubits];
    let address_space = 1 <<< nQubits;

    // Prepare random basis state in qs.
    Fact(address_value >= 0 and address_value < address_space, "Value must fit in the number of qubits.");
    let state = Std.Convert.IntAsBoolArray(address_value, nQubits);
    ApplyPauliFromBitString(PauliX, true, state, qs);

    // Construct power products.
    use aux_qubits = Qubit[GetAuxCountForPP(nQubits)];
    let products = ConstructPowerProducts(qs, aux_qubits);
    Fact(Length(products) == address_space - 1, $"Power product length should be {address_space - 1}.");

    // Verify that each product qubit is correct.
    for index in 0..address_space-2 {
        // Shift by 1 since empty product is not included.
        let monomial_index = index + 1;
        mutable expected_value = true;
        for bit_position in 0..nQubits-1 {
            if ((monomial_index &&& (1 <<< bit_position)) != 0) {
                // This qubit is included in the product.
                set expected_value = expected_value and state[bit_position];
            }
        }
        within {
            if (expected_value) {
                // Invert if expected value is 1 - we'll check for |0> state.
                X(products[index]);
            }
        } apply {
            Fact(CheckZero(products[index]), $"Power product at index {index} should match expected value {expected_value}.");
        }
    }

    // Destruct power products to reset aux qubits.
    DestructPowerProducts(products);

    // Reset original qubits.
    ApplyPauliFromBitString(PauliX, true, state, qs);

    // All qubits should be back to |0> state at this point.
}

@Test()
operation TestPowerProductsExhaustive() : Unit {
    // Test power products construction for various numbers of qubits and basis states.
    for nQubits in 0..5 {
        let address_space = 1 <<< nQubits;
        for value in 0..address_space-1 {
            CheckPowerProducts(nQubits, value);
        }
    }
}
