// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub mod codegen;
pub mod compile;
pub mod error;
pub mod incremental;
pub mod interpret;
pub mod location;
pub mod packages;

pub use qsc_formatter::formatter;

pub use qsc_frontend::compile::{CompileUnit, PackageStore};

pub mod resolve {
    pub use qsc_frontend::resolve::{
        GlobalScope, Local, Locals, NameKind, Res, iter_valid_items, path_as_field_accessor,
    };
}

pub mod fir {
    pub use qsc_fir::{fir::*, *};
}

pub mod hir {
    pub use qsc_hir::{hir::*, *};
}

pub mod ast {
    pub use qsc_ast::{ast::*, *};
}

pub mod project {
    pub use qsc_project::{
        DirEntry, EntryType, Error, FileSystem, Manifest, ManifestDescriptor, PackageCache,
        PackageGraphSources, ProjectType,
    };
}

pub use qsc_data_structures::{
    functors::FunctorApp, language_features::LanguageFeatures, namespaces::*, source::*,
    span::Span, target::TargetCapabilityFlags,
};

pub use qsc_passes::{PackageType, PassContext, lower_hir_to_fir};

pub mod line_column {
    pub use qsc_data_structures::line_column::{Encoding, Position, Range};
}

pub use qsc_eval::{
    backend::{Backend, SparseSim},
    noise::PauliNoise,
    state::{
        fmt_basis_state_label, fmt_complex, format_state_id, get_matrix_latex, get_phase,
        get_state_latex,
    },
    val::Result as BackendResult,
};

pub mod linter {
    pub use qsc_linter::{
        GroupConfig, LintConfig, LintKind, LintLevel, LintOrGroupConfig, run_lints,
    };
}

pub use qsc_doc_gen::{display, generate_docs};

pub mod circuit {
    pub use qsc_circuit::{
        CURRENT_VERSION, Circuit, CircuitGroup, TracerConfig,
        circuit_to_qsharp::circuits_to_qsharp, json_to_circuit::json_to_circuits, operations::*,
    };
}

pub mod parse {
    pub use qsc_parse::{completion, top_level_nodes};
}

pub mod partial_eval {
    pub use qsc_partial_eval::Error;
}

pub mod target {
    pub use qsc_data_structures::target::Profile;
}

pub mod openqasm;
