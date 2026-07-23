// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{fmt::Write as _, sync::Arc};

use qdk_openqasm::io::InMemorySourceResolver;

const EXACT_SIZE_HEADER: &str = "OPENQASM 3.0;\nqubit[32] q;\nbit[32] c;\n";

#[derive(Clone, Copy, Debug)]
pub enum ExactSize {
    KiB10,
    KiB100,
    MiB1,
    MiB5,
    MiB10,
}

impl ExactSize {
    pub const ALL: [Self; 5] = [
        Self::KiB10,
        Self::KiB100,
        Self::MiB1,
        Self::MiB5,
        Self::MiB10,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::KiB10 => "10KiB",
            Self::KiB100 => "100KiB",
            Self::MiB1 => "1MiB",
            Self::MiB5 => "5MiB",
            Self::MiB10 => "10MiB",
        }
    }

    #[must_use]
    pub const fn bytes(self) -> usize {
        match self {
            Self::KiB10 => 10 * 1024,
            Self::KiB100 => 100 * 1024,
            Self::MiB1 => 1024 * 1024,
            Self::MiB5 => 5 * 1024 * 1024,
            Self::MiB10 => 10 * 1024 * 1024,
        }
    }

    #[must_use]
    pub const fn statement_count(self) -> usize {
        match self {
            Self::KiB10 => 437,
            Self::KiB100 => 4_358,
            Self::MiB1 => 44_620,
            Self::MiB5 => 223_103,
            Self::MiB10 => 446_203,
        }
    }
}

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

    #[must_use]
    pub fn source_bytes(&self) -> usize {
        self.source.len()
    }
}

#[must_use]
pub fn exact_size(size: ExactSize) -> Corpus {
    let target_bytes = size.bytes();
    let mut source = String::with_capacity(target_bytes);
    source.push_str(EXACT_SIZE_HEADER);
    let mut statement_count = 2;
    let mut cycle_index = 0;

    loop {
        let qubit = cycle_index % 32;
        let other = (qubit + 1) % 32;
        let statements = [
            format!("U(pi / 4, 0, pi) q[{qubit}];\n"),
            format!("ctrl @ U(pi / 2, 0, pi) q[{qubit}], q[{other}];\n"),
            format!("barrier q[{qubit}], q[{other}];\n"),
            format!("c[{qubit}] = measure q[{qubit}];\n"),
            format!("reset q[{qubit}];\n"),
        ];

        for statement in statements {
            if source.len() + statement.len() > target_bytes {
                source.extend(std::iter::repeat_n(' ', target_bytes - source.len()));
                return Corpus {
                    name: size.label(),
                    source: Arc::from(source),
                    path: Arc::from("exact_size.qasm"),
                    statement_count,
                    includes: Vec::new(),
                };
            }
            source.push_str(&statement);
            statement_count += 1;
        }
        cycle_index += 1;
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

#[must_use]
#[allow(dead_code)]
pub fn directive_heavy(repetitions: usize) -> Corpus {
    let mut source = String::new();
    source.push_str("OPENQASM 3.0;\n");

    for index in 0..repetitions {
        let _ = writeln!(source, "pragma vendor.mode{index} opaque/*payload*/ π  ");
        let _ = writeln!(source, "@vendor.note{index} //payload  ");
        let _ = writeln!(source, "bit flag{index};");
    }

    Corpus {
        name: "directive_heavy",
        source: Arc::from(source),
        path: Arc::from("directive_heavy.qasm"),
        statement_count: 1 + (2 * repetitions),
        includes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qdk_openqasm::{analyze_source, parse_source};

    #[test]
    fn exact_size_corpora_have_expected_lengths_and_parse_successfully() {
        for size in ExactSize::ALL {
            let corpus = exact_size(size);
            assert_eq!(
                corpus.source_bytes(),
                size.bytes(),
                "{} length",
                size.label()
            );
            assert_eq!(
                corpus.statement_count,
                size.statement_count(),
                "{} statement count",
                size.label()
            );

            let mut resolver = corpus.resolver();
            let parse_result = parse_source(
                corpus.source.clone(),
                corpus.path.clone(),
                Some(&mut resolver),
            );
            assert!(
                !parse_result.has_errors(),
                "{} syntax parse produced {} errors",
                size.label(),
                parse_result.all_errors().len()
            );
            assert_eq!(
                parse_result
                    .source
                    .program()
                    .expect("successful exact-size parse should retain its program")
                    .statements
                    .len(),
                size.statement_count(),
                "{} parsed statement count",
                size.label()
            );

            let mut resolver = corpus.resolver();
            let analysis_result = analyze_source(
                corpus.source.clone(),
                corpus.path.clone(),
                Some(&mut resolver),
            );
            assert!(
                !analysis_result.has_errors(),
                "{} semantic analysis produced {} errors",
                size.label(),
                analysis_result.all_errors().len()
            );
        }
    }
}
