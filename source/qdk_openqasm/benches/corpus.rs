// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{fmt::Write as _, sync::Arc};

use qdk_openqasm::io::InMemorySourceResolver;

#[derive(Clone, Debug)]
pub struct Corpus {
    pub name: &'static str,
    pub source: Arc<str>,
    pub path: Arc<str>,
    pub statement_count: usize,
    includes: Vec<(Arc<str>, Arc<str>)>,
}

impl Corpus {
    #[must_use]
    pub fn resolver(&self) -> InMemorySourceResolver {
        self.includes.iter().cloned().collect()
    }
}

#[must_use]
pub fn flat_gate(repetitions: usize) -> Corpus {
    let mut source = String::new();
    source.push_str("OPENQASM 3.0;\n");
    source.push_str("include \"stdgates.inc\";\n");
    source.push_str("qubit q0;\n");
    source.push_str("qubit q1;\n");
    source.push_str("bit c0;\n");

    for index in 0..repetitions {
        source.push_str("h q0;\n");
        source.push_str("cx q0, q1;\n");
        source.push_str("rz(0.125) q1;\n");
        if index.is_multiple_of(8) {
            source.push_str("c0 = measure q1;\n");
            source.push_str("reset q1;\n");
        }
    }

    let measured_cycles = repetitions.div_ceil(8);
    Corpus {
        name: "flat_gate",
        source: Arc::from(source),
        path: Arc::from("flat_gate.qasm"),
        statement_count: 5 + (3 * repetitions) + (2 * measured_cycles),
        includes: Vec::new(),
    }
}

#[must_use]
pub fn broadcast_gate(repetitions: usize, register_width: usize) -> Corpus {
    let mut source = String::new();
    source.push_str("OPENQASM 3.0;\n");
    source.push_str("include \"stdgates.inc\";\n");
    let _ = writeln!(source, "qubit[{register_width}] left;");
    let _ = writeln!(source, "qubit[{register_width}] right;");

    for _ in 0..repetitions {
        source.push_str("h left;\n");
        source.push_str("cx left, right;\n");
        source.push_str("rz(0.25) right;\n");
    }

    Corpus {
        name: "broadcast_gate",
        source: Arc::from(source),
        path: Arc::from("broadcast_gate.qasm"),
        statement_count: 4 + (3 * repetitions),
        includes: Vec::new(),
    }
}

#[must_use]
pub fn include_heavy(include_count: usize, statements_per_include: usize) -> Corpus {
    let mut source = String::new();
    source.push_str("OPENQASM 3.0;\n");
    source.push_str("include \"stdgates.inc\";\n");
    source.push_str("qubit q;\n");

    let mut includes = Vec::with_capacity(include_count);
    for include_index in 0..include_count {
        let path = format!("bench/include_{include_index}.inc");
        let _ = writeln!(source, "include \"{path}\";");
        let _ = writeln!(source, "g{include_index} q;");

        let mut include_source = String::new();
        let _ = writeln!(include_source, "gate g{include_index} target {{");
        for statement_index in 0..statements_per_include {
            if statement_index.is_multiple_of(3) {
                include_source.push_str("    h target;\n");
            } else if statement_index.is_multiple_of(3_usize.saturating_sub(1)) {
                include_source.push_str("    rz(0.0625) target;\n");
            } else {
                include_source.push_str("    x target;\n");
            }
        }
        include_source.push_str("}\n");
        includes.push((Arc::from(path), Arc::from(include_source)));
    }

    Corpus {
        name: "include_heavy",
        source: Arc::from(source),
        path: Arc::from("include_heavy.qasm"),
        statement_count: 3 + (2 * include_count) + (include_count * statements_per_include),
        includes,
    }
}
