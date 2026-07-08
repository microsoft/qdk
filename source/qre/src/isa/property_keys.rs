// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// NOTE: To add a new property key:
// 1. Add the name to the `define_properties!` macro below (values are auto-assigned)
// 2. Add it to `add_property_keys` in qre.rs
// 3. Add it to property_keys.pyi
//
// The `property_name_to_key` function is auto-generated from the entries.

/// Property keys for instruction properties. These are used to query properties of instructions in the ISA.
macro_rules! define_properties {
    // Internal rule: accumulator-based counting to auto-assign incrementing u64 values.
    (@step $counter:expr, $name:ident, $($rest:ident),* $(,)?) => {
        pub const $name: u64 = $counter;
        define_properties!(@step $counter + 1, $($rest),*);
    };
    (@step $counter:expr, $name:ident $(,)?) => {
        pub const $name: u64 = $counter;
    };
    // Entry point
    ( $($name:ident),* $(,)? ) => {
        define_properties!(@step 0, $($name),*);

        /// Property name → integer key mapping
        pub fn property_name_to_key(name: &str) -> Option<u64> {
            match name {
                $(
                    stringify!($name) => Some($name),
                )*
                _ => None
            }
        }

        /// Integer key → property name mapping
        #[must_use]
        pub fn property_name(id: u64) -> Option<&'static str> {
            match id {
                $(
                    $name => Some(stringify!($name)),
                )*
                _ => None,
            }
        }
    };
}

define_properties! {
    DISTANCE,
    SURFACE_CODE_ONE_QUBIT_TIME_FACTOR,
    SURFACE_CODE_TWO_QUBIT_TIME_FACTOR,
    ACCELERATION,
    NUM_TS_PER_ROTATION,
    RUNTIME_SINGLE_SHOT,
    EXPECTED_SHOTS,
    EVALUATION_TIME,
    PHYSICAL_COMPUTE_QUBITS,
    PHYSICAL_FACTORY_QUBITS,
    PHYSICAL_MEMORY_QUBITS,
    MOLECULE,
    LOGICAL_COMPUTE_QUBITS,
    LOGICAL_MEMORY_QUBITS,
    ALGORITHM_COMPUTE_QUBITS,
    ALGORITHM_MEMORY_QUBITS,
    NAME,
    LOSS,
    LOGICAL_CYCLE_TIME,
    CODE_CYCLE_TIME,
    ATOM_SPACING,
    DATA_QUBIT_SPACING,
    VELOCITY,
    ASSUMPTIONS,
    FEASIBILITY,
    TARGET_YEAR,
    BLOCK_SIZE,
    BASE_SYSTEM_COST,
    SHOT_COST,
    COST_PER_QUBIT,
    COST_PER_HOUR,
    COST_PER_QUBIT_PER_HOUR,
}

#[cfg(test)]
mod tests {
    use super::{
        BASE_SYSTEM_COST, COST_PER_HOUR, COST_PER_QUBIT, COST_PER_QUBIT_PER_HOUR, DISTANCE,
        SHOT_COST, property_name, property_name_to_key,
    };

    #[test]
    fn cost_property_keys_round_trip() {
        let keys = [
            ("BASE_SYSTEM_COST", BASE_SYSTEM_COST),
            ("SHOT_COST", SHOT_COST),
            ("COST_PER_QUBIT", COST_PER_QUBIT),
            ("COST_PER_HOUR", COST_PER_HOUR),
            ("COST_PER_QUBIT_PER_HOUR", COST_PER_QUBIT_PER_HOUR),
        ];

        for (name, key) in keys {
            assert_eq!(property_name(key), Some(name));
            assert_eq!(property_name_to_key(name), Some(key));
        }

        assert_ne!(BASE_SYSTEM_COST, DISTANCE);
        assert_ne!(SHOT_COST, DISTANCE);
        assert_ne!(COST_PER_QUBIT, DISTANCE);
        assert_ne!(COST_PER_HOUR, DISTANCE);
        assert_ne!(COST_PER_QUBIT_PER_HOUR, DISTANCE);
    }
}
