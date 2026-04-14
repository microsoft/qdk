// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::model::{StructType, Type};

/// Represents the writer-facing compatibility contract for emitted QIR bitcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QirEmitTarget {
    /// Typed-pointer QIR v1 output for legacy LLVM 14 and 15 style consumers.
    QirV1Typed,
    /// Opaque-pointer QIR v2 output for LLVM 16 and newer style consumers.
    QirV2Opaque,
}

impl QirEmitTarget {
    /// The QIR major version number for this emit target.
    #[must_use]
    pub const fn major_version(self) -> i64 {
        match self {
            Self::QirV1Typed => 1,
            Self::QirV2Opaque => 2,
        }
    }

    /// The QIR minor version number for this emit target.
    #[must_use]
    pub const fn minor_version(self) -> i64 {
        0
    }

    /// Whether this target emits typed pointers (`%Qubit*`) instead of opaque pointers (`ptr`).
    #[must_use]
    pub const fn uses_typed_pointers(self) -> bool {
        matches!(self, Self::QirV1Typed)
    }

    /// The module layout version currently emitted for this target.
    #[must_use]
    pub const fn module_bitcode_version(self) -> u64 {
        match self {
            Self::QirV1Typed => 1,
            Self::QirV2Opaque => 2,
        }
    }

    /// Opaque struct type declarations needed for typed-pointer emit targets.
    #[must_use]
    pub fn struct_types(self) -> Vec<StructType> {
        if self.uses_typed_pointers() {
            vec![
                StructType {
                    name: RESULT_TYPE_NAME.into(),
                    is_opaque: true,
                },
                StructType {
                    name: QUBIT_TYPE_NAME.into(),
                    is_opaque: true,
                },
            ]
        } else {
            vec![]
        }
    }

    /// Returns the pointer type the writer should synthesize for the given pointee.
    #[must_use]
    pub fn pointer_type_for_pointee(self, pointee: &Type) -> Type {
        match self {
            Self::QirV1Typed => match pointee {
                Type::Named(name) | Type::NamedPtr(name) => Type::NamedPtr(name.clone()),
                Type::TypedPtr(inner) => Type::TypedPtr(inner.clone()),
                other => Type::TypedPtr(Box::new(other.clone())),
            },
            Self::QirV2Opaque => Type::Ptr,
        }
    }

    /// Returns the default pointer type to use when the model carries an untyped null pointer.
    #[must_use]
    pub fn default_pointer_type(self) -> Type {
        self.pointer_type_for_pointee(&Type::Integer(8))
    }
}

/// Represents a concrete QIR spec profile and version combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QirProfile {
    /// QIR Base Profile v1: typed pointers, no dynamic features.
    BaseV1,
    /// QIR Adaptive Profile v1: typed pointers, forward branching and qubit reuse.
    AdaptiveV1,
    /// QIR Adaptive Profile v2: opaque pointers, full capabilities.
    AdaptiveV2,
}

impl QirProfile {
    /// The writer-facing emit target for this profile.
    #[must_use]
    pub const fn emit_target(self) -> QirEmitTarget {
        match self {
            Self::BaseV1 | Self::AdaptiveV1 => QirEmitTarget::QirV1Typed,
            Self::AdaptiveV2 => QirEmitTarget::QirV2Opaque,
        }
    }

    /// The `qir_profiles` attribute value for this profile.
    #[must_use]
    pub const fn profile_name(self) -> &'static str {
        match self {
            Self::BaseV1 => BASE_PROFILE,
            Self::AdaptiveV1 | Self::AdaptiveV2 => ADAPTIVE_PROFILE,
        }
    }

    /// The QIR major version number for module flags metadata.
    #[must_use]
    pub const fn major_version(self) -> i64 {
        self.emit_target().major_version()
    }

    /// The QIR minor version number for module flags metadata.
    #[must_use]
    pub const fn minor_version(self) -> i64 {
        self.emit_target().minor_version()
    }

    /// Whether this profile uses typed pointers (`%Qubit*`) or opaque pointers (`ptr`).
    #[must_use]
    pub const fn uses_typed_pointers(self) -> bool {
        self.emit_target().uses_typed_pointers()
    }

    /// Opaque struct type declarations needed for typed-pointer profiles.
    #[must_use]
    pub fn struct_types(self) -> Vec<StructType> {
        self.emit_target().struct_types()
    }
}

/// Returns the argument index that must carry a string output label for the given runtime call.
#[must_use]
pub fn output_label_arg_index(callee: &str) -> Option<usize> {
    match callee {
        rt::TUPLE_RECORD_OUTPUT
        | rt::ARRAY_RECORD_OUTPUT
        | rt::RESULT_RECORD_OUTPUT
        | rt::BOOL_RECORD_OUTPUT
        | rt::INT_RECORD_OUTPUT
        | rt::DOUBLE_RECORD_OUTPUT => Some(1),
        rt::RESULT_ARRAY_RECORD_OUTPUT => Some(2),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qir_profiles_map_to_explicit_emit_targets() {
        assert_eq!(QirProfile::BaseV1.emit_target(), QirEmitTarget::QirV1Typed);
        assert_eq!(
            QirProfile::AdaptiveV1.emit_target(),
            QirEmitTarget::QirV1Typed
        );
        assert_eq!(
            QirProfile::AdaptiveV2.emit_target(),
            QirEmitTarget::QirV2Opaque
        );
    }

    #[test]
    fn qir_emit_targets_choose_expected_pointer_shapes() {
        assert_eq!(
            QirEmitTarget::QirV1Typed.pointer_type_for_pointee(&Type::Named("Qubit".into())),
            Type::NamedPtr("Qubit".into())
        );
        assert_eq!(
            QirEmitTarget::QirV1Typed.pointer_type_for_pointee(&Type::Integer(8)),
            Type::TypedPtr(Box::new(Type::Integer(8)))
        );
        assert_eq!(
            QirEmitTarget::QirV2Opaque.pointer_type_for_pointee(&Type::Named("Qubit".into())),
            Type::Ptr
        );
    }

    #[test]
    fn qir_emit_targets_choose_expected_module_bitcode_versions() {
        assert_eq!(QirEmitTarget::QirV1Typed.module_bitcode_version(), 1);
        assert_eq!(QirEmitTarget::QirV2Opaque.module_bitcode_version(), 2);
    }

    #[test]
    fn output_recording_calls_map_to_expected_label_argument_indexes() {
        for callee in [
            rt::TUPLE_RECORD_OUTPUT,
            rt::ARRAY_RECORD_OUTPUT,
            rt::RESULT_RECORD_OUTPUT,
            rt::BOOL_RECORD_OUTPUT,
            rt::INT_RECORD_OUTPUT,
            rt::DOUBLE_RECORD_OUTPUT,
        ] {
            assert_eq!(output_label_arg_index(callee), Some(1));
        }

        assert_eq!(
            output_label_arg_index(rt::RESULT_ARRAY_RECORD_OUTPUT),
            Some(2)
        );
        assert_eq!(output_label_arg_index(rt::INITIALIZE), None);
    }
}

pub const BASE_PROFILE: &str = "base_profile";
pub const ADAPTIVE_PROFILE: &str = "adaptive_profile";

pub const ENTRYPOINT_NAME: &str = "ENTRYPOINT__main";

pub const ENTRY_POINT_ATTR: &str = "entry_point";
pub const OUTPUT_LABELING_SCHEMA_ATTR: &str = "output_labeling_schema";
pub const QIR_PROFILES_ATTR: &str = "qir_profiles";
pub const REQUIRED_NUM_QUBITS_ATTR: &str = "required_num_qubits";
pub const REQUIRED_NUM_RESULTS_ATTR: &str = "required_num_results";
pub const IRREVERSIBLE_ATTR: &str = "irreversible";
pub const QDK_NOISE_ATTR: &str = "qdk_noise";

pub const QIR_MAJOR_VERSION_KEY: &str = "qir_major_version";
pub const QIR_MINOR_VERSION_KEY: &str = "qir_minor_version";
pub const DYNAMIC_QUBIT_MGMT_KEY: &str = "dynamic_qubit_management";
pub const DYNAMIC_RESULT_MGMT_KEY: &str = "dynamic_result_management";
pub const INT_COMPUTATIONS_KEY: &str = "int_computations";
pub const FLOAT_COMPUTATIONS_KEY: &str = "float_computations";
pub const BACKWARDS_BRANCHING_KEY: &str = "backwards_branching";
pub const ARRAYS_KEY: &str = "arrays";
pub const IR_FUNCTIONS_KEY: &str = "ir_functions";
pub const MULTIPLE_TARGET_BRANCHING_KEY: &str = "multiple_target_branching";
pub const MULTIPLE_RETURN_POINTS_KEY: &str = "multiple_return_points";
pub const MODULE_FLAGS_NAME: &str = "llvm.module.flags";

pub const QUBIT_TYPE_NAME: &str = "Qubit";
pub const RESULT_TYPE_NAME: &str = "Result";

pub const ENTRY_POINT_ATTR_GROUP_ID: u32 = 0;
pub const IRREVERSIBLE_ATTR_GROUP_ID: u32 = 1;
pub const QDK_NOISE_ATTR_GROUP_ID: u32 = 2;

pub const FLAG_BEHAVIOR_ERROR: i64 = 1;
pub const FLAG_BEHAVIOR_APPEND: i64 = 5;
pub const FLAG_BEHAVIOR_MAX: i64 = 7;

pub mod rt {
    pub const INITIALIZE: &str = "__quantum__rt__initialize";
    pub const READ_RESULT: &str = "__quantum__rt__read_result";
    pub const READ_LOSS: &str = "__quantum__rt__read_loss";
    pub const RESULT_RECORD_OUTPUT: &str = "__quantum__rt__result_record_output";
    pub const TUPLE_RECORD_OUTPUT: &str = "__quantum__rt__tuple_record_output";
    pub const ARRAY_RECORD_OUTPUT: &str = "__quantum__rt__array_record_output";
    pub const BOOL_RECORD_OUTPUT: &str = "__quantum__rt__bool_record_output";
    pub const INT_RECORD_OUTPUT: &str = "__quantum__rt__int_record_output";
    pub const DOUBLE_RECORD_OUTPUT: &str = "__quantum__rt__double_record_output";
    pub const QUBIT_ALLOCATE: &str = "__quantum__rt__qubit_allocate";
    pub const QUBIT_BORROW: &str = "__quantum__rt__qubit_borrow";
    pub const QUBIT_RELEASE: &str = "__quantum__rt__qubit_release";
    pub const BEGIN_PARALLEL: &str = "__quantum__rt__begin_parallel";
    pub const END_PARALLEL: &str = "__quantum__rt__end_parallel";
    pub const READ_ATOM_RESULT: &str = "__quantum__rt__read_atom_result";
    pub const RESULT_ALLOCATE: &str = "__quantum__rt__result_allocate";
    pub const RESULT_RELEASE: &str = "__quantum__rt__result_release";
    pub const QUBIT_ARRAY_ALLOCATE: &str = "__quantum__rt__qubit_array_allocate";
    pub const QUBIT_ARRAY_RELEASE: &str = "__quantum__rt__qubit_array_release";
    pub const RESULT_ARRAY_ALLOCATE: &str = "__quantum__rt__result_array_allocate";
    pub const RESULT_ARRAY_RELEASE: &str = "__quantum__rt__result_array_release";
    pub const RESULT_ARRAY_RECORD_OUTPUT: &str = "__quantum__rt__result_array_record_output";
}

pub mod qis {
    pub const X: &str = "__quantum__qis__x__body";
    pub const Y: &str = "__quantum__qis__y__body";
    pub const Z: &str = "__quantum__qis__z__body";
    pub const H: &str = "__quantum__qis__h__body";
    pub const S: &str = "__quantum__qis__s__body";
    pub const S_ADJ: &str = "__quantum__qis__s__adj";
    pub const SX: &str = "__quantum__qis__sx__body";
    pub const T: &str = "__quantum__qis__t__body";
    pub const T_ADJ: &str = "__quantum__qis__t__adj";

    pub const RX: &str = "__quantum__qis__rx__body";
    pub const RY: &str = "__quantum__qis__ry__body";
    pub const RZ: &str = "__quantum__qis__rz__body";

    pub const CX: &str = "__quantum__qis__cx__body";
    pub const CY: &str = "__quantum__qis__cy__body";
    pub const CZ: &str = "__quantum__qis__cz__body";
    pub const SWAP: &str = "__quantum__qis__swap__body";

    pub const RXX: &str = "__quantum__qis__rxx__body";
    pub const RYY: &str = "__quantum__qis__ryy__body";
    pub const RZZ: &str = "__quantum__qis__rzz__body";

    pub const CCX: &str = "__quantum__qis__ccx__body";

    pub const M: &str = "__quantum__qis__m__body";
    pub const MZ: &str = "__quantum__qis__mz__body";
    pub const MRESETZ: &str = "__quantum__qis__mresetz__body";
    pub const RESET: &str = "__quantum__qis__reset__body";

    pub const BARRIER: &str = "__quantum__qis__barrier__body";
    pub const MOVE: &str = "__quantum__qis__move__body";
    pub const READ_RESULT: &str = "__quantum__qis__read_result__body";
}
