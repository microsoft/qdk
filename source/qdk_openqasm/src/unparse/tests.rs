// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use num_bigint::BigInt;
use std::io;

use crate::{
    io::InMemorySourceResolver,
    parse_source,
    parser::ast::{Expr, ExprKind, Lit, LiteralKind, Program, Stmt, StmtKind, Version},
    span::Span,
    unparse::{UnparseError, unparse, write},
};

fn parse(source: &str) -> crate::parser::ParseResult {
    let mut resolver = InMemorySourceResolver::from_iter([
        ("defs.inc".into(), "".into()),
        ("qelib1.inc".into(), "".into()),
    ]);
    parse_source(source, "main.qasm", Some(&mut resolver))
}

fn emit(source: &str) -> String {
    let parsed = parse(source);
    assert!(
        parsed.errors().is_empty(),
        "entry source should parse without errors: {:?}",
        parsed.errors()
    );
    unparse(
        parsed
            .source
            .program()
            .expect("syntax parse should retain its program"),
    )
    .expect("valid source should serialize")
}

fn assert_round_trip(source: &str) {
    let emitted = emit(source);
    let reparsed = parse(&emitted);
    assert!(
        reparsed.errors().is_empty(),
        "canonical source should reparse: {:?}\n{emitted}",
        reparsed.errors()
    );
    let second = unparse(
        reparsed
            .source
            .program()
            .expect("syntax parse should retain its program"),
    )
    .expect("reparsed source should serialize");
    assert_eq!(emitted, second, "canonical emission should be stable");
    assert!(emitted.ends_with('\n'));
    assert!(!emitted.ends_with("\n\n"));
    assert!(!emitted.contains('\r'));
}

#[test]
fn versions_and_includes_round_trip_stably() {
    assert_round_trip("OPENQASM 2.0; include \"qelib1.inc\"; qreg q[2]; creg c[2];");
    assert_round_trip("OPENQASM 3.0; include \"stdgates.inc\"; qubit[2] q; bit[2] c;");
    assert_round_trip("OPENQASM 3.1;\r\ninclude \"defs.inc\";");
}

#[test]
fn statements_and_expressions_round_trip_stably() {
    assert_round_trip(
        r#"OPENQASM 3.1;
@vendor.tag payload
qubit[3] q;
bit[3] c;
int a = 1;
int b = 2;
int d = (a + b) * 3;
ctrl @ x q[0], q[1];
inv @ pow(2) @ rz(0.5) q[2];
barrier q[0], q[1];
c[0] = measure q[0];
measure q[1] -> c[1];
reset q[2];
if (a < b) { a += 1; } else { a -= 1; }
for int i in [0:2] { delay[10ns] q[i]; }
while (a < 4) { a += 1; }
switch (a) { case 1, 2 { x q[0]; } default { z q[0]; } }
box { x q[0]; }
pragma vendor.mode exact
"#,
    );
}

#[test]
fn canonical_output_uses_bitwise_operator_precedence() {
    for expression in ["1 | 2 ^ 3", "1 ^ 2 | 3", "1 ^ 2 & 3", "1 & 2 ^ 3"] {
        assert_eq!(
            emit(&format!("OPENQASM 3.0; int value = {expression};")),
            format!("OPENQASM 3.0;\nint value = {expression};\n")
        );
    }
}

#[test]
fn declarations_types_and_callable_forms_round_trip_stably() {
    assert_round_trip(
        r#"OPENQASM 3.0;
const int[32] n = 4;
input float[64] theta;
output bit result;
array[int[32], 3] values = {1, 2, 3};
gate pair(phi) a, b { rx(phi) a; cx a, b; }
extern sample(float[64]) -> bit;
def add(int a, int b) -> int { return a + b; }
qubit[4] q;
let pair_alias = q[0] ++ q[1];
pair(theta) q[0], q[1];
"#,
    );
}

#[test]
fn strings_bitstrings_and_calibration_round_trip_stably() {
    assert_round_trip(concat!(
        "OPENQASM 3.0;\n",
        "defcalgrammar \"open\\\"pulse\";\n",
        "bit[8] bits = \"0010_1010\";\n",
        "cal { pulse frame; }\n",
        "defcal x $0 { play; }\n",
    ));
}

#[test]
fn remaining_statement_expression_and_type_families_round_trip_stably() {
    assert_round_trip(
        r#"OPENQASM 3.1;
input complex[float[32]] input_value;
output duration elapsed;
stretch slack;
complex[float[32]] value = 1.5im;
int[64] huge = 9223372036854775808;
bool flag = !false;
array[int[8], 3] values = {1, 2, 3};
qubit[2] q;
gate phase(theta) target { gphase(theta) target; }
extern inspect(readonly array[int[8], 2, 3]) -> bit;
def use(mutable array[int[8], #dim = 2] data, qubit target) {
  duration elapsed = durationof({ delay[10ns] target; });
  int item = int[32](data[0][0]);
  item = item + 1;
  phase(pi / 2) target;
  box[20ns] { barrier target; }
  while (flag) { break; continue; }
  return;
}
{ end; }
"#,
    );
}

#[test]
fn canonical_output_uses_two_space_indentation() {
    assert_eq!(
        emit("OPENQASM 3.0; if (true) { if (false) { end; } }"),
        "OPENQASM 3.0;\nif (true) {\n  if (false) {\n    end;\n  }\n}\n"
    );
}

#[test]
fn i64_minimum_magnitude_and_negative_form_round_trip_stably() {
    assert_eq!(
        emit("OPENQASM 3.1; int[64] value = 9223372036854775808;"),
        "OPENQASM 3.1;\nint[64] value = 9223372036854775808;\n"
    );
    assert_eq!(
        emit("OPENQASM 3.1; int[64] value = -9223372036854775808;"),
        "OPENQASM 3.1;\nint[64] value = -9223372036854775808;\n"
    );
}

#[test]
fn string_values_that_look_like_bitstrings_remain_strings() {
    let span = Span { lo: 1, hi: 4 };
    let program = expression_program(span, LiteralKind::String("101".into()));
    let emitted = unparse(&program).expect("string should serialize");
    assert_eq!(emitted, "'101';\n");
    assert_round_trip(&emitted);
}

#[test]
fn recovered_syntax_is_rejected() {
    let parsed = parse("OPENQASM 3.0; int value = ;");
    let error = unparse(
        parsed
            .source
            .program()
            .expect("syntax parse should retain its program"),
    )
    .expect_err("recovered syntax should not serialize");
    assert_eq!(error.code(), "recovered-syntax");
}

#[test]
fn non_finite_floats_are_rejected() {
    let span = Span { lo: 4, hi: 7 };
    let mut program = expression_program(span, LiteralKind::Float(f64::INFINITY));
    program.version = Some(Version {
        major: 3,
        minor: Some(0),
        span: Span::default(),
    });
    assert_eq!(
        unparse(&program),
        Err(UnparseError::NonFiniteFloat { span })
    );
}

#[test]
fn invalid_strings_are_rejected() {
    let span = Span { lo: 1, hi: 2 };
    let program = expression_program(span, LiteralKind::String("bad\u{0000}".into()));
    let error = unparse(&program).expect_err("invalid string should not serialize");
    assert_eq!(error.code(), "invalid-string");
    assert_eq!(error.span(), span);
}

#[test]
fn unsupported_bitstring_values_are_rejected() {
    let span = Span { lo: 2, hi: 3 };
    let program = expression_program(span, LiteralKind::Bitstring(BigInt::from(-1), 1));
    let error = unparse(&program).expect_err("invalid bitstring should not serialize");
    assert_eq!(error.code(), "unsupported-syntax");
}

#[test]
fn write_streams_canonical_output_to_sink() {
    let parsed = parse("OPENQASM 3.0; qubit q;");
    let program = parsed
        .source
        .program()
        .expect("syntax parse should retain its program");
    let mut output = Vec::new();

    write(&mut output, program).expect("valid source should serialize");

    assert_eq!(output, b"OPENQASM 3.0;\nqubit q;\n");
}

#[test]
fn write_reports_sink_failures() {
    let program = expression_program(Span::default(), LiteralKind::Int(1));
    let error = write(FailingWriter, &program).expect_err("sink failure should propagate");

    assert_eq!(error.code(), "write");
}

struct FailingWriter;

impl io::Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("expected sink failure"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn expression_program(span: Span, literal: LiteralKind) -> Program {
    Program {
        version: None,
        statements: vec![Stmt {
            span,
            annotations: Vec::new().into_boxed_slice(),
            kind: Box::new(StmtKind::ExprStmt(crate::parser::ast::ExprStmt {
                span,
                expr: Expr {
                    span,
                    kind: Box::new(ExprKind::Lit(Lit {
                        span,
                        kind: literal,
                    })),
                },
            })),
        }]
        .into_boxed_slice(),
        span,
    }
}
