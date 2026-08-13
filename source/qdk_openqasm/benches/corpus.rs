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

    /// The byte length of the entry source, excluding any includes.
    #[must_use]
    pub fn source_bytes(&self) -> usize {
        self.source.len()
    }
}

/// A byte budget for the size-indexed corpora.
///
/// These are the sizes the recorded parse and analyze baselines were measured
/// at, so keeping them stable keeps successive runs comparable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExactSize {
    Kib10,
    Kib100,
    Mib1,
    Mib5,
    Mib10,
}

impl ExactSize {
    pub const ALL: [Self; 5] = [
        Self::Kib10,
        Self::Kib100,
        Self::Mib1,
        Self::Mib5,
        Self::Mib10,
    ];

    /// The label used on the command line and in benchmark identifiers.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Kib10 => "10KiB",
            Self::Kib100 => "100KiB",
            Self::Mib1 => "1MiB",
            Self::Mib5 => "5MiB",
            Self::Mib10 => "10MiB",
        }
    }

    /// The exact source length the generated corpus will have.
    #[must_use]
    pub const fn byte_budget(self) -> usize {
        match self {
            Self::Kib10 => 10 * 1024,
            Self::Kib100 => 100 * 1024,
            Self::Mib1 => 1024 * 1024,
            Self::Mib5 => 5 * 1024 * 1024,
            Self::Mib10 => 10 * 1024 * 1024,
        }
    }

    const fn corpus_name(self) -> &'static str {
        match self {
            Self::Kib10 => "exact_size_10KiB",
            Self::Kib100 => "exact_size_100KiB",
            Self::Mib1 => "exact_size_1MiB",
            Self::Mib5 => "exact_size_5MiB",
            Self::Mib10 => "exact_size_10MiB",
        }
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

/// A corpus dominated by pragma and annotation directives.
///
/// Directive lexing emits a command token, value tokens, and an end token per
/// directive, so this corpus is the one that moves when that path changes.
#[must_use]
pub fn directive_heavy(repetitions: usize) -> Corpus {
    let mut source = String::new();
    source.push_str("OPENQASM 3.0;\n");
    source.push_str("include \"stdgates.inc\";\n");
    source.push_str("qubit[2] q;\n");

    for index in 0..repetitions {
        let _ = writeln!(source, "pragma qdk.bench.marker index {index}");
        let _ = writeln!(source, "@qdk.bench.note index {index}");
        source.push_str("h q[0];\n");
        let _ = writeln!(source, "pragma qdk.bench.region depth {index} width 2");
        source.push_str("cx q[0], q[1];\n");
    }

    Corpus {
        name: "directive_heavy",
        source: Arc::from(source),
        path: Arc::from("directive_heavy.qasm"),
        // Three header statements, then per repetition: two pragmas and two
        // gate calls. The annotation attaches to the gate call that follows it
        // rather than forming a statement of its own.
        statement_count: 3 + (4 * repetitions),
        includes: Vec::new(),
    }
}

/// Builds a flat gate corpus whose source is exactly [`ExactSize::byte_budget`]
/// bytes long.
///
/// Whole statements are emitted until the next one would overrun the budget,
/// then the remainder is filled with newlines. Newlines are trivia, so the
/// padding changes the byte count without changing the statement count.
#[must_use]
pub fn exact_size(size: ExactSize) -> Corpus {
    const HEADER: &str = "OPENQASM 3.0;\ninclude \"stdgates.inc\";\nqubit[2] q;\n";
    const HEADER_STATEMENTS: usize = 3;
    const BODY: [&str; 3] = ["h q[0];\n", "cx q[0], q[1];\n", "rz(0.125) q[1];\n"];

    let budget = size.byte_budget();
    assert!(
        budget > HEADER.len(),
        "byte budget must exceed the corpus header"
    );

    let mut source = String::with_capacity(budget);
    source.push_str(HEADER);

    let mut statement_count = HEADER_STATEMENTS;
    for statement in BODY.iter().cycle() {
        if source.len() + statement.len() > budget {
            break;
        }
        source.push_str(statement);
        statement_count += 1;
    }

    // The shortest body statement is longer than one byte, so the loop can stop
    // short of the budget. Newline padding closes the gap exactly.
    while source.len() < budget {
        source.push('\n');
    }

    Corpus {
        name: size.corpus_name(),
        source: Arc::from(source),
        path: Arc::from("exact_size.qasm"),
        statement_count,
        includes: Vec::new(),
    }
}
