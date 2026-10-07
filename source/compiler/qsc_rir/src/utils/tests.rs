// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use rustc_hash::FxHashMap;

use super::{build_deep_mapping_index, update_variable_mapping};
use crate::rir::{Literal, Operand, OperandMapping, Variable, VariableId};

/// Reference version of `update_variable_mapping` that finds the deep mappings to downgrade by
/// scanning the whole variable map. Returns the number of mappings it downgraded.
fn update_variable_mapping_by_scan(
    var_map: &mut FxHashMap<VariableId, OperandMapping>,
    operand: &Operand,
    var: &Variable,
) -> usize {
    var_map.insert(var.variable_id, operand.last_mapping(var_map));
    let mut downgraded = 0;
    for mapping in var_map.values_mut() {
        if let OperandMapping::Deep(Operand::Variable(existing)) = mapping
            && existing == var
        {
            *mapping = OperandMapping::Shallow(Operand::Variable(*existing));
            downgraded += 1;
        }
    }
    downgraded
}

#[test]
fn indexed_downgrade_of_deep_mappings_matches_full_scan() {
    const NUM_VARS: u32 = 10;
    const ROUNDS: usize = 200;
    const STORES_PER_ROUND: usize = 30;

    // A fixed linear congruential generator keeps the store sequences deterministic.
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = |bound: u32| -> u32 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        u32::try_from((state >> 33) % u64::from(bound)).expect("value should fit in u32")
    };
    let var = |id: u32| Variable::new_integer(VariableId(id));

    // Start each round from a map with a parameter-style self mapping, a deep mapping to a variable
    // with no mapping of its own, and a shallow mapping.
    let initial: FxHashMap<VariableId, OperandMapping> = [
        (
            VariableId(0),
            OperandMapping::Deep(Operand::Variable(var(0))),
        ),
        (
            VariableId(1),
            OperandMapping::Deep(Operand::Variable(var(5))),
        ),
        (
            VariableId(2),
            OperandMapping::Shallow(Operand::Variable(var(6))),
        ),
    ]
    .into_iter()
    .collect();

    let mut total_downgraded = 0;
    for _ in 0..ROUNDS {
        let mut expected = initial.clone();
        let mut actual = initial.clone();
        let mut index = build_deep_mapping_index(&actual);
        for _ in 0..STORES_PER_ROUND {
            let target = var(next(NUM_VARS));
            let operand = if next(4) == 0 {
                Operand::Literal(Literal::Integer(next(100).into()))
            } else {
                Operand::Variable(var(next(NUM_VARS)))
            };

            total_downgraded += update_variable_mapping_by_scan(&mut expected, &operand, &target);
            update_variable_mapping(&mut actual, &mut index, &operand, &target);
            assert_eq!(
                expected, actual,
                "variable maps diverged after storing {operand} to {target}"
            );
        }
    }

    assert!(
        total_downgraded > 0,
        "store sequences should exercise downgrading deep mappings"
    );
}
