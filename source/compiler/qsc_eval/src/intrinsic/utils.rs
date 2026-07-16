// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::hash_map::Entry;

use num_bigint::BigUint;
use num_complex::{Complex, Complex64};
use num_traits::Zero;
use qdk_simulators::NearlyZero;
use rustc_hash::{FxHashMap, FxHashSet};

/// Given a state and a set of qubits, split the state into two parts: the qubits to dump and the remaining qubits.
/// This function will return an error if the state is not separable using the provided qubit identifiers.
pub fn split_state(
    qubits: &[usize],
    state: &[(BigUint, Complex64)],
    qubit_count: usize,
) -> Result<Vec<(BigUint, Complex64)>, ()> {
    // For an empty state, return an empty state.
    // This handles cases where the underlying simulator doesn't track any quantum state.
    if state.is_empty() {
        return Ok(vec![]);
    }

    let mut dump_state = FxHashMap::default();

    // Compute the mask for the qubits to dump and the mask for the other qubits.
    let (dump_mask, other_mask) = compute_mask(qubit_count, qubits);

    // Try to split out the state for the given qubits from the whole state, detecting any entanglement
    // and returning an error if the qubits are not separable.
    let dump_norm = collect_split_state(state, &dump_mask, &other_mask, &mut dump_state)?;

    let dump_norm = 1.0 / dump_norm.sqrt();
    let mut dump_state = dump_state
        .into_iter()
        .filter_map(|(label, val)| {
            normalize_and_reorder(val, dump_norm, qubits, &label, qubit_count)
        })
        .collect::<Vec<_>>();
    dump_state.sort_by(|(a, _), (b, _)| a.cmp(b));
    Ok(dump_state)
}

/// From the qubit identifiers provided, compute the bit masks for the qubits to dump and the remaining qubits.
/// These masks can be applied to the state labels to separate the label into the two parts needed.
fn compute_mask(qubit_count: usize, qubits: &[usize]) -> (BigUint, BigUint) {
    let mut dump_mask = BigUint::zero();
    let mut other_mask = BigUint::zero();
    for q in 0..qubit_count {
        // Note that the qubit order is reversed to match the order of the qubits in the state.
        if qubits.contains(&q) {
            dump_mask.set_bit((qubit_count - q - 1) as u64, true);
        } else {
            other_mask.set_bit((qubit_count - q - 1) as u64, true);
        }
    }
    (dump_mask, other_mask)
}

/// Iterates through the given state and for each entry uses the mask to calculate what the separated labels would be
/// and finds the amplitude for each separated state. If the state is not separable, returns an error.
/// On success, the `dump_state` and `other_state` maps will be populated with the separated states, and the
/// function returns the accumulated norm of the dump state.
fn collect_split_state(
    state: &[(BigUint, Complex64)],
    dump_mask: &BigUint,
    other_mask: &BigUint,
    dump_state: &mut FxHashMap<BigUint, Complex64>,
) -> Result<f64, ()> {
    // To ensure consistent ordering, we iterate over the vector directly (returned from the simulator in a deterministic order),
    // and not the map used for arbitrary lookup.
    let mut state_iter = state.iter();
    let state_map = state.iter().cloned().collect::<FxHashMap<_, _>>();
    let (base_label, base_val) = state_iter.next().expect("state should never be empty");
    let dump_base_label = base_label & dump_mask;
    let other_base_label = base_label & other_mask;
    let mut dump_norm = base_val.norm().powi(2);
    let mut other_state = FxHashSet::default();

    dump_state.insert(dump_base_label.clone(), *base_val);
    other_state.insert(other_base_label.clone());

    for (curr_label, curr_val) in state_iter {
        let dump_label = curr_label & dump_mask;
        let other_label = curr_label & other_mask;

        // If either the state identified by the dump mask or the state identified by the other mask
        // is None, that means it has zero amplitude and we can conclude the state is not separable.
        let Some(dump_val) = state_map.get(&(&dump_label | &other_base_label)) else {
            return Err(());
        };
        let Some(other_val) = state_map.get(&(&dump_base_label | &other_label)) else {
            return Err(());
        };

        if !(dump_val * other_val - base_val * curr_val)
            .norm()
            .is_nearly_zero()
        {
            // Coefficients are not equal, so the state is not separable.
            return Err(());
        }

        if let Entry::Vacant(entry) = dump_state.entry(dump_label) {
            let amplitude = *curr_val;
            let norm = amplitude.norm().powi(2);
            if !norm.is_nearly_zero() {
                entry.insert(amplitude);
                dump_norm += norm;
            }
        }
        if !(curr_val / dump_val).norm().powi(2).is_nearly_zero() {
            other_state.insert(other_label);
        }
    }

    // If the product of the collected states is not equal to the total number of input states, then that
    // implies some states are zero amplitude that would have to be non-zero for the state to be separable.
    if state.len() != dump_state.len() * other_state.len() {
        return Err(());
    }
    Ok(dump_norm)
}

/// Given a dump state amplitude, the normalization factor, the qubits to dump, the label, and the qubit count,
/// normalize the amplitude and reorder the label to match the provided qubit order.
/// Specifically, qubits in the requested array may not be in the same allocation order that is used in the state
/// labels, so the bits in the label must be reordered to match the qubit order.
fn normalize_and_reorder(
    val: Complex64,
    dump_norm: f64,
    qubits: &[usize],
    label: &BigUint,
    qubit_count: usize,
) -> Option<(BigUint, Complex64)> {
    // Normalize the dump state by the collected factor.
    let new_val = val * dump_norm;
    // Drop any zero amplitude states.
    if new_val.is_nearly_zero() {
        None
    } else {
        // Reorder the bits in the label to match the provided qubit order.
        let mut new_label = BigUint::zero();
        for (i, q) in qubits.iter().enumerate() {
            // Note that the qubit order is reversed to match the order of the qubits in the state.
            if label.bit((qubit_count - *q - 1) as u64) {
                new_label.set_bit((qubits.len() - i - 1) as u64, true);
            }
        }

        Some((new_label, new_val))
    }
}

pub(crate) fn state_to_matrix(
    state: Vec<(BigUint, Complex64)>,
    qubit_count: usize,
) -> Vec<Vec<Complex64>> {
    let state: FxHashMap<BigUint, Complex<f64>> = state.into_iter().collect();
    let mut matrix = Vec::new();
    let num_entries: usize = 1 << qubit_count;
    #[allow(clippy::cast_precision_loss)]
    let factor = (num_entries as f64).sqrt();
    for i in 0..num_entries {
        let mut row = Vec::new();
        for j in 0..num_entries {
            let key = BigUint::from(i * num_entries + j);
            let val = match state.get(&key) {
                Some(val) => val * factor,
                None => Complex::zero(),
            };
            row.push(val);
        }
        matrix.push(row);
    }

    matrix
}
