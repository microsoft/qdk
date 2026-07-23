// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Arithmetic.RippleCarryCGIncByLE;
import Std.Arrays.Mapped;
import Std.Convert.BigIntAsBoolArray;
import Std.Diagnostics.Fact;
import Std.ResourceEstimation.IsResourceEstimating;
import Std.ResourceEstimation.RepeatEstimates;
import Std.TableLookup.Select;

import ClassicalMath.SafeMod;
import Modular.ModAdd.ModAdd;

// References:
// - Thomas Haener, Vadym Kliuchnikov, Martin Roetteler, Mathias Soeken,
//   "Space-time optimized table lookup", 2022.
//   https://arxiv.org/abs/2211.01133
// - Dominic W. Berry, Craig Gidney, Mario Motta, Jarrod R. McClean, Ryan Babbush,
//   "Qubitization of Arbitrary Basis Quantum Chemistry Leveraging Sparsity and
//   Low Rank Factorization" (Appendix C), 2019.
//   https://arxiv.org/abs/1902.02134

function _CombineTables(tables : BigInt[][], num_bits : Int, modulus : BigInt) : BigInt[] {
    mutable combined = Mapped(x -> SafeMod(x, modulus), tables[0]);
    for i in 1..Length(tables) - 1 {
        let shift = i * num_bits;
        for j in 0..Length(combined) - 1 {
            let shifted_value = SafeMod(tables[i][j], modulus) <<< shift;
            set combined w/= j <- combined[j] ||| shifted_value;
        }
    }
    return combined;
}

function DataRowsAsBits(data : BigInt[], num_bits : Int) : Bool[][] {
    let wrap = 1L <<< num_bits;
    return Mapped(x -> BigIntAsBoolArray(SafeMod(x, wrap), num_bits), data);
}

// Requires 0 <= data[i] < 2^Length(address).
operation _Lookup(data : BigInt[], address : Qubit[], target : Qubit[]) : Unit is Adj {
    let address_size = Length(address);
    let can_use_formula = (address_size >= 3) and (Length(data) == 2^address_size);

    if (can_use_formula and IsResourceEstimating()) {
        // TODO: make Lookup a primitive instruction in resource estimator.
        let num_ancilla = address_size - 1;
        use q_anc = Qubit[num_ancilla];
        within {
            RepeatEstimates(2 ^^^ address_size - 2);
        } apply {
            AND(address[0], address[1], q_anc[0]);
            Adjoint AND(address[0], address[1], q_anc[0]);
        }
    } else {
        let data_bits = DataRowsAsBits(data, Length(target));
        Select(data_bits, address, target);
    }
}

/// # Summary
/// Computes `q_result := (q_result + data[q_address]) % 2^n`.
/// Requires `0 <= data[i] < 2^n`.
///
/// # Input
/// ## q_address
/// Register encoding the table index `q_address`.
/// ## q_result
/// Target register updated in place.
/// ## data
/// Lookup table of values to add.
operation AddLookup(
    q_address : Qubit[],
    q_result : Qubit[],
    data : BigInt[]
) : Unit {
    use q_select_output = Qubit[Length(q_result)];

    within {
        _Lookup(data, q_address, q_select_output);
    } apply {
        RippleCarryCGIncByLE(q_select_output, q_result);
    }
}

/// # Summary
/// Computes `q_result := (q_result + data[q_address]) % modulus`.
///
/// # Input
/// ## q_address
/// Register encoding the table index `q_address`.
/// ## q_result
/// Target register updated in place.
/// ## data
/// Lookup table of values to add.
/// ## modulus
/// Modulus used for modular addition.
operation ModAddLookup(
    q_address : Qubit[],
    q_result : Qubit[],
    data : BigInt[],
    modulus : BigInt
) : Unit is Adj {
    use q_select_output = Qubit[Length(q_result)];
    let data_modded = Mapped(x -> SafeMod(x, modulus), data);
    within {
        _Lookup(data_modded, q_address, q_select_output);
    } apply {
        ModAdd(q_select_output, q_result, modulus);
    }
}

/// # Summary
/// Computes `q_result[i] := (q_result[i] + tables[i][q_address]) % modulus` for each `i`.
///
/// # Input
/// ## q_address
/// Register encoding the table index `q_address`.
/// ## q_result
/// Array of target registers, each updated in place.
/// ## tables
/// Array of lookup tables matched one-to-one with `q_result`.
/// ## modulus
/// Modulus used for each modular addition.
operation ParallelModAddLookup(
    q_address : Qubit[],
    q_result : Qubit[][],
    tables : BigInt[][],
    modulus : BigInt
) : Unit {
    let num_tables = Length(tables);
    Fact(num_tables == Length(q_result), "Size mismatch.");
    Fact(num_tables > 0, "Must provide at least one table.");
    let result_size = Length(q_result[0]);
    let table_length = Length(tables[0]);
    for i in 0..num_tables - 1 {
        Fact(Length(q_result[i]) == result_size, "All target registers must have equal size.");
        Fact(Length(tables[i]) == table_length, "All tables must have the same length.");
    }

    if (Std.Core.ConfigValue("minimize_qubits", true)) {
        for i in 0..num_tables - 1 {
            ModAddLookup(q_address, q_result[i], tables[i], modulus);
        }
    } else {
        use q_select_output = Qubit[num_tables * result_size];
        let tables_combined = _CombineTables(tables, result_size, modulus);
        within {
            _Lookup(tables_combined, q_address, q_select_output);
        } apply {
            for i in 0..num_tables - 1 {
                let q_temp = q_select_output[i * result_size..(i + 1) * result_size - 1];
                ModAdd(q_temp, q_result[i], modulus);
            }
        }
    }
}

export AddLookup, ModAddLookup, ParallelModAddLookup;
