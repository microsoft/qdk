// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::{Expect, expect};

fn check(contents: &str, expect: &Expect) {
    let actual = match serde_json::from_str::<Circuit>(contents) {
        Ok(circuit) => build_operation_def("Test", &circuit),
        Err(e) => format!("Error: {e}"),
    };
    expect.assert_eq(&actual);
}

fn check_circuit_group(contents: &str, expect: &Expect) {
    let actual = match circuits_to_qsharp("Test", contents) {
        Ok(circuit) => circuit,
        Err(e) => e,
    };
    expect.assert_eq(&actual);
}

#[test]
fn qsharp_from_circuit() {
    check_circuit_group(
        r#"
{
  "version": 1,
  "circuits": [
    {
      "componentGrid": [
        {
          "components": [
            { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] },
            { "kind": "unitary", "gate": "S", "targets": [{ "qubit": 1 }] }
          ]
        },
        {
          "components": [
            { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] },
            { "kind": "unitary", "gate": "X", "targets": [{ "qubit": 1 }] }
          ]
        }
      ],
      "qubits": [{ "id": 0 }, { "id": 1 }]
    }
  ]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                H(qs[0]);
                S(qs[1]);
                Z(qs[0]);
                X(qs[1]);
            }

        "#]],
    );
}

#[test]
fn circuit_with_controlled_gate() {
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] },
        { "kind": "unitary", "gate": "S", "targets": [{ "qubit": 1 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] }
      ]
    },
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "X",
          "controls": [{ "qubit": 0 }],
          "targets": [{ "qubit": 1 }]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0 }, { "id": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                H(qs[0]);
                S(qs[1]);
                Z(qs[0]);
                Controlled X([qs[0]], qs[1]);
            }

        "#]],
    );
}

#[test]
fn circuit_with_adjoint_gate() {
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] },
        { "kind": "unitary", "gate": "S", "targets": [{ "qubit": 1 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] }
      ]
    },
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "X",
          "isAdjoint": true,
          "targets": [{ "qubit": 1 }]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0 }, { "id": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                H(qs[0]);
                S(qs[1]);
                Z(qs[0]);
                Adjoint X(qs[1]);
            }

        "#]],
    );
}

#[test]
fn circuit_with_controlled_adjoint_gate() {
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] },
        { "kind": "unitary", "gate": "S", "targets": [{ "qubit": 1 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] }
      ]
    },
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "X",
          "isAdjoint": true,
          "controls": [{ "qubit": 0 }],
          "targets": [{ "qubit": 1 }]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0 }, { "id": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                H(qs[0]);
                S(qs[1]);
                Z(qs[0]);
                Controlled Adjoint X([qs[0]], qs[1]);
            }

        "#]],
    );
}

#[test]
fn circuit_with_rz_gate() {
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] },
        { "kind": "unitary", "gate": "Rz", "targets": [{ "qubit": 1 }], "args": ["1.2"] }
      ]
    }
  ],
  "qubits": [{ "id": 0 }, { "id": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                H(qs[0]);
                Z(qs[0]);
                Rz(1.2, qs[1]);
            }

        "#]],
    );
}

#[test]
fn circuit_with_controlled_gate_multiple_args() {
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] }
      ]
    },
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "Rz",
          "controls": [{ "qubit": 0 }],
          "targets": [{ "qubit": 1 }],
          "args": ["1.2"]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0 }, { "id": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                H(qs[0]);
                Z(qs[0]);
                Controlled Rz([qs[0]], (1.2, qs[1]));
            }

        "#]],
    );
}

#[test]
fn circuit_with_pi_arg() {
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] },
        { "kind": "unitary", "gate": "Rz", "targets": [{ "qubit": 1 }], "args": ["π / 2.0"] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Rx", "targets": [{ "qubit": 1 }], "args": ["π / 4.0"] }
      ]
    }
  ],
  "qubits": [{ "id": 0 }, { "id": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                let π = Std.Math.PI();
                H(qs[0]);
                Z(qs[0]);
                Rz(π / 2.0, qs[1]);
                Rx(π / 4.0, qs[1]);
            }

        "#]],
    );
}

#[test]
fn circuit_with_measurement_gate() {
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] },
        { "kind": "unitary", "gate": "S", "targets": [{ "qubit": 1 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] }
      ]
    },
    {
      "components": [
        {
          "kind": "measurement",
          "gate": "Measure",
          "qubits": [{ "qubit": 0 }],
          "results": [{ "qubit": 0, "result": 0 }]
        }
      ]
    }
  ],
  "qubits": [
    { "id": 0, "numResults": 1 },
    { "id": 1 }
  ]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Result {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                H(qs[0]);
                S(qs[1]);
                Z(qs[0]);
                let c0_0 = M(qs[0]);
                return c0_0;
            }

        "#]],
    );
}

#[test]
fn circuit_with_multiple_measurement_gates() {
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] },
        { "kind": "unitary", "gate": "S", "targets": [{ "qubit": 1 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] }
      ]
    },
    {
      "components": [
        {
          "kind": "measurement",
          "gate": "Measure",
          "qubits": [{ "qubit": 0 }],
          "results": [{ "qubit": 0, "result": 0 }]
        },
        {
          "kind": "measurement",
          "gate": "Measure",
          "qubits": [{ "qubit": 1 }],
          "results": [{ "qubit": 1, "result": 0 }]
        }
      ]
    },
    {
      "components": [
        {
          "kind": "measurement",
          "gate": "Measure",
          "qubits": [{ "qubit": 0 }],
          "results": [{ "qubit": 0, "result": 1 }]
        }
      ]
    }
  ],
  "qubits": [
    { "id": 0, "numResults": 2 },
    { "id": 1, "numResults": 1 }
  ]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Result[] {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                H(qs[0]);
                S(qs[1]);
                Z(qs[0]);
                let c0_0 = M(qs[0]);
                let c1_0 = M(qs[1]);
                let c0_1 = M(qs[0]);
                return [c0_0, c0_1, c1_0];
            }

        "#]],
    );
}

#[test]
fn empty_circuit() {
    check(
        r#"
{
  "componentGrid": [],
  "qubits": []
}"#,
        &expect![[r#"
            operation Test() : Unit is Ctl + Adj {
            }

        "#]],
    );
}

#[test]
fn empty_circuit_with_qubits() {
    check(
        r#"
{
  "componentGrid": [],
  "qubits": [{ "id": 0 }, { "id": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
            }

        "#]],
    );
}

#[test]
fn circuit_with_qubit_missing_num_results() {
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
      ]
    }
  ],
  "qubits": [
    { "id": 0 }
  ]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 1 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 1 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 1 qubits.";
                }
                H(qs[0]);
            }

        "#]],
    );
}

#[test]
fn circuit_with_ket_gates() {
    check(
        #[allow(clippy::unicode_not_nfc)]
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "ket", "gate": "0", "targets": [{ "qubit": 0 }] },
        { "kind": "ket", "gate": "1", "targets": [{ "qubit": 1 }] }
      ]
    }
  ],
  "qubits": [
    { "id": 0 },
    { "id": 1 }
  ]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                Reset(qs[0]);
                fail "Unsupported ket operation: |1〉";
            }

        "#]],
    );
}

#[test]
fn circuit_with_int_args() {
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] },
        { "kind": "unitary", "gate": "Rz", "targets": [{ "qubit": 1 }], "args": ["π / 2"] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Rx", "targets": [{ "qubit": 1 }], "args": [".4 + 4. / 2"] }
      ]
    }
  ],
  "qubits": [{ "id": 0 }, { "id": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                let π = Std.Math.PI();
                H(qs[0]);
                Z(qs[0]);
                Rz(π / 2., qs[1]);
                Rx(.4 + 4. / 2., qs[1]);
            }

        "#]],
    );
}

#[test]
fn circuit_with_sqrt_x_gate() {
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] },
        { "kind": "unitary", "gate": "SX", "targets": [{ "qubit": 1 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 1 }] }
      ]
    }
  ],
  "qubits": [{ "id": 0 }, { "id": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                H(qs[0]);
                Z(qs[0]);
                SX(qs[1]);
                Z(qs[1]);
            }

        "#]],
    );
}

#[test]
fn circuit_with_ctrl_adj_sqrt_x_gate() {
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] },
        {
          "kind": "unitary",
          "gate": "SX",
          "isAdjoint": true,
          "controls": [{ "qubit": 1 }],
          "targets": [{ "qubit": 0 }]
        }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 1 }] }
      ]
    }
  ],
  "qubits": [{ "id": 0 }, { "id": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                H(qs[0]);
                Z(qs[0]);
                Controlled Adjoint SX([qs[1]], qs[0]);
                Z(qs[1]);
            }

        "#]],
    );
}

// ---------------------------------------------------------------------------
// Recursive emission of structural groups (loops, conditionals, scopes,
// loop-iteration wrappers). These groups are produced by the circuit tracer
// and don't correspond to real callable operations — emitting them as a call
// to e.g. `loop: 0..3(qs)` would be nonsense. Instead the emitter recurses
// into their children and surfaces the structure as Q# comments.
//
// Custom-gate groups (a Unitary whose name *does* refer to a real operation
// in the user's project) are deliberately preserved as a call so the emitted
// preview keeps the user's abstraction.
// ---------------------------------------------------------------------------

#[test]
fn custom_gate_with_children_emits_call_not_inline() {
    // A custom gate `Foo` carries its body in `children` for visualization
    // purposes. We must keep emitting `Foo(...)` — inlining would duplicate
    // code that the user has already defined elsewhere in the project.
    //
    // The test uses two top-level components so the entry-point-wrapper
    // unwrap heuristic doesn't apply (which is reserved for the case where
    // the trace wraps the entire body in a single non-existent operation).
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "Foo",
          "targets": [{ "qubit": 0 }, { "qubit": 1 }],
          "children": [
            {
              "components": [
                { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
              ]
            },
            {
              "components": [
                { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 1 }] }
              ]
            }
          ]
        }
      ]
    },
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "Foo",
          "targets": [{ "qubit": 0 }, { "qubit": 1 }],
          "children": [
            {
              "components": [
                { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
              ]
            }
          ]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0 }, { "id": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                Foo(qs[0], qs[1]);
                Foo(qs[0], qs[1]);
            }

        "#]],
    );
}

#[test]
fn loop_group_inlines_with_comment_header() {
    // The `loop: 0..3` outer group becomes a `// loop:` … `// end loop`
    // pair; each `(N)` iteration wrapper becomes a single-line
    // `// iteration (N)` marker so the reader can see iteration
    // boundaries even when iteration bodies differ structurally.
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "loop: 0..3",
          "targets": [{ "qubit": 0 }, { "qubit": 1 }],
          "children": [
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "(0)",
                  "targets": [{ "qubit": 0 }],
                  "children": [
                    {
                      "components": [
                        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
                      ]
                    }
                  ]
                }
              ]
            },
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "(1)",
                  "targets": [{ "qubit": 1 }],
                  "children": [
                    {
                      "components": [
                        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 1 }] }
                      ]
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0 }, { "id": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                // loop: 0..3
                // iteration (0)
                H(qs[0]);
                // iteration (1)
                H(qs[1]);
                // end loop
            }

        "#]],
    );
}

#[test]
fn conditional_group_inlines_with_comment_header() {
    // The `if:` group's header becomes a comment; its body inlines.
    // The inner H carries no classical controls itself (those live on the
    // outer group), so it emits as a plain `H(qs[0])` — exactly what we
    // want when the outer conditional is rendered as a comment.
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "measurement", "gate": "M", "qubits": [{ "qubit": 0 }], "results": [{ "qubit": 0, "result": 0 }] }
      ]
    },
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "if: c_0 == One",
          "targets": [{ "qubit": 0 }, { "qubit": 0, "result": 0 }],
          "controls": [{ "qubit": 0, "result": 0 }],
          "isConditional": true,
          "children": [
            {
              "components": [
                { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
              ]
            }
          ]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0, "numResults": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 1 qubits.
            operation Test(qs : Qubit[]) : Result {
                if Length(qs) < 1 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 1 qubits.";
                }
                let c0_0 = M(qs[0]);
                // if: c_0 == One
                H(qs[0]);
                // end if
                return c0_0;
            }

        "#]],
    );
}

#[test]
fn anonymous_scope_inlines_with_comment_header() {
    // `<lambda>` and `<scope>` are compiler-synthesized labels for groupings
    // that have no callable name. They emit as anonymous scope comments.
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "<lambda>",
          "targets": [{ "qubit": 0 }],
          "children": [
            {
              "components": [
                { "kind": "unitary", "gate": "X", "targets": [{ "qubit": 0 }] }
              ]
            }
          ]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 1 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 1 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 1 qubits.";
                }
                // <lambda>
                X(qs[0]);
                // end scope
            }

        "#]],
    );
}

#[test]
fn measurement_nested_in_loop_disqualifies_ctl_adj() {
    // The grid_is_all_unitary check must descend into structural groups —
    // a measurement inside a loop body is just as much a non-unitary as
    // one at the top level, and the emitted operation must NOT declare
    // `is Ctl + Adj`.
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "loop: 0..1",
          "targets": [{ "qubit": 0 }, { "qubit": 0, "result": 0 }],
          "children": [
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "(0)",
                  "targets": [{ "qubit": 0 }],
                  "children": [
                    {
                      "components": [
                        { "kind": "measurement", "gate": "M", "qubits": [{ "qubit": 0 }], "results": [{ "qubit": 0, "result": 0 }] }
                      ]
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0, "numResults": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 1 qubits.
            operation Test(qs : Qubit[]) : Result {
                if Length(qs) < 1 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 1 qubits.";
                }
                // loop: 0..1
                // iteration (0)
                let c0_0 = M(qs[0]);
                // end loop
                return c0_0;
            }

        "#]],
    );
}

#[test]
fn nested_structural_groups_compose() {
    // A loop with two iterations, the second of which contains an `if:`
    // group. Verifies that nested structural groups compose cleanly and
    // that the inner conditional's children are reachable.
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "loop: 0..1",
          "targets": [{ "qubit": 0 }],
          "children": [
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "(0)",
                  "targets": [{ "qubit": 0 }],
                  "children": [
                    {
                      "components": [
                        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
                      ]
                    }
                  ]
                }
              ]
            },
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "(1)",
                  "targets": [{ "qubit": 0 }],
                  "children": [
                    {
                      "components": [
                        {
                          "kind": "unitary",
                          "gate": "if: c_0 == One",
                          "targets": [{ "qubit": 0 }],
                          "controls": [{ "qubit": 0, "result": 0 }],
                          "isConditional": true,
                          "children": [
                            {
                              "components": [
                                { "kind": "unitary", "gate": "X", "targets": [{ "qubit": 0 }] }
                              ]
                            }
                          ]
                        }
                      ]
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0 }]
}"#,
        &expect![[r#"
            // NOTE: This Q# preview was reconstructed from a circuit trace and is approximate.
            // The original Q# source is the authoritative version.
            //   - loop has structurally divergent iterations: loop: 0..1
            /// Expects a qubit register of at least 1 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 1 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 1 qubits.";
                }
                // loop: 0..1
                // iteration (0)
                H(qs[0]);
                // iteration (1)
                // if: c_0 == One
                X(qs[0]);
                // end if
                // end loop
            }

        "#]],
    );
}

#[test]
fn bare_iteration_marker_emits_visible_header() {
    // A bare `(0)` wrapper is rare — the tracer normally produces them
    // only inside a `loop:` group — but if one shows up at the top level
    // we still want it visible. The header is a single-line marker (no
    // closing comment), since iteration boundaries are implicitly closed
    // by the next iteration or the enclosing loop's `// end loop`.
    //
    // (Two top-level components keep the entry-point-wrapper unwrap from
    // hiding the marker.)
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "(0)",
          "targets": [{ "qubit": 0 }],
          "children": [
            {
              "components": [
                { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
              ]
            }
          ]
        }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "X", "targets": [{ "qubit": 0 }] }
      ]
    }
  ],
  "qubits": [{ "id": 0 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 1 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 1 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 1 qubits.";
                }
                // iteration (0)
                H(qs[0]);
                X(qs[0]);
            }

        "#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)] // long because the inline JSON mirrors a real trace
fn group_splitting_test_shape_emits_loop_with_asymmetric_iterations() {
    // Trims a real trace produced by samples/circuit_integration/GroupSplittingTest.qs
    // (entry-point wrapper, an outer `loop: 0..3` with structurally
    // different iterations, including a conditional that only appears in
    // later iterations and a custom-gate call to `Foo`). The point of
    // this test is to lock down what the user sees when they open such a
    // trace as a `.qsc` file: a flat sequence of gates inside `// loop`
    // and `// if` markers, with custom gates preserved as calls.
    //
    // This test intentionally lives alongside the unit tests rather than
    // in an integration harness so it stays in sync with the emitter
    // output as we extend the structural-group handling. If it breaks
    // because we taught the emitter real `for` / `if` syntax, the new
    // expectation is the contract going forward.
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "Main",
          "targets": [{ "qubit": 0 }, { "qubit": 1 }, { "qubit": 2 }, { "qubit": 3 }],
          "children": [
            {
              "components": [
                { "kind": "unitary", "gate": "X", "targets": [{ "qubit": 1 }] },
                { "kind": "unitary", "gate": "X", "targets": [{ "qubit": 2 }] }
              ]
            },
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "loop: 0..3",
                  "targets": [{ "qubit": 0 }, { "qubit": 2 }, { "qubit": 3 }],
                  "children": [
                    {
                      "components": [
                        {
                          "kind": "unitary",
                          "gate": "(1)",
                          "targets": [{ "qubit": 0 }, { "qubit": 2 }, { "qubit": 3 }],
                          "children": [
                            {
                              "components": [
                                { "kind": "unitary", "gate": "X", "targets": [{ "qubit": 3 }] },
                                { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
                              ]
                            },
                            {
                              "components": [
                                { "kind": "measurement", "gate": "M", "qubits": [{ "qubit": 0 }], "results": [{ "qubit": 0, "result": 0 }] }
                              ]
                            },
                            {
                              "components": [
                                { "kind": "ket", "gate": "0", "targets": [{ "qubit": 0 }] }
                              ]
                            },
                            {
                              "components": [
                                {
                                  "kind": "unitary",
                                  "gate": "Foo",
                                  "targets": [{ "qubit": 0 }, { "qubit": 2 }],
                                  "children": [
                                    {
                                      "components": [
                                        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] },
                                        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 2 }] }
                                      ]
                                    }
                                  ]
                                }
                              ]
                            }
                          ]
                        }
                      ]
                    },
                    {
                      "components": [
                        {
                          "kind": "unitary",
                          "gate": "(2)",
                          "targets": [{ "qubit": 0 }, { "qubit": 2 }, { "qubit": 3 }],
                          "children": [
                            {
                              "components": [
                                { "kind": "unitary", "gate": "X", "targets": [{ "qubit": 3 }] },
                                { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
                              ]
                            },
                            {
                              "components": [
                                {
                                  "kind": "unitary",
                                  "gate": "if: (f(c_0)) > (2)",
                                  "targets": [{ "qubit": 0 }, { "qubit": 0, "result": 0 }],
                                  "controls": [{ "qubit": 0, "result": 0 }],
                                  "isConditional": true,
                                  "children": [
                                    {
                                      "components": [
                                        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
                                      ]
                                    }
                                  ]
                                }
                              ]
                            },
                            {
                              "components": [
                                { "kind": "measurement", "gate": "M", "qubits": [{ "qubit": 0 }], "results": [{ "qubit": 0, "result": 1 }] }
                              ]
                            },
                            {
                              "components": [
                                { "kind": "ket", "gate": "0", "targets": [{ "qubit": 0 }] }
                              ]
                            },
                            {
                              "components": [
                                {
                                  "kind": "unitary",
                                  "gate": "Foo",
                                  "targets": [{ "qubit": 0 }, { "qubit": 2 }]
                                }
                              ]
                            }
                          ]
                        }
                      ]
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }
  ],
  "qubits": [
    { "id": 0, "numResults": 2 },
    { "id": 1 },
    { "id": 2 },
    { "id": 3 }
  ]
}"#,
        &expect![[r#"
            // NOTE: This Q# preview was reconstructed from a circuit trace and is approximate.
            // The original Q# source is the authoritative version.
            //   - loop has structurally divergent iterations: loop: 0..3
            //   - conditional uses an opaque expression: if: (f(c_0)) > (2)
            /// Expects a qubit register of at least 4 qubits.
            operation Test(qs : Qubit[]) : Result[] {
                if Length(qs) < 4 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 4 qubits.";
                }
                X(qs[1]);
                X(qs[2]);
                // loop: 0..3
                // iteration (1)
                X(qs[3]);
                H(qs[0]);
                let c0_0 = M(qs[0]);
                Reset(qs[0]);
                Foo(qs[0], qs[2]);
                // iteration (2)
                X(qs[3]);
                H(qs[0]);
                // if: (f(c_0)) > (2)
                H(qs[0]);
                // end if
                let c0_1 = M(qs[0]);
                Reset(qs[0]);
                Foo(qs[0], qs[2]);
                // end loop
                return [c0_0, c0_1];
            }

        "#]],
    );
}

// ---------------------------------------------------------------------------
// Trace-divergence detection
//
// These tests exercise the banner that appears above the operation when the
// circuit contains shapes the emitter can't faithfully recreate as Q#.
// Editor-authored circuits should never trigger the banner; trace-derived
// circuits with non-uniform loops or opaque conditionals should.
// ---------------------------------------------------------------------------

#[test]
fn uniform_loop_emits_no_banner() {
    // Two iterations whose bodies share the same shape (only the qubit
    // index differs) — the canonical `for i in 0..1 { H(qs[i]); }` pattern.
    // The detector must treat this as uniform and *not* emit a banner.
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "loop: 0..1",
          "targets": [{ "qubit": 0 }, { "qubit": 1 }],
          "children": [
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "(0)",
                  "targets": [{ "qubit": 0 }],
                  "children": [
                    {
                      "components": [
                        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
                      ]
                    }
                  ]
                }
              ]
            },
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "(1)",
                  "targets": [{ "qubit": 1 }],
                  "children": [
                    {
                      "components": [
                        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 1 }] }
                      ]
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0 }, { "id": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 2 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 2 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 2 qubits.";
                }
                // loop: 0..1
                // iteration (0)
                H(qs[0]);
                // iteration (1)
                H(qs[1]);
                // end loop
            }

        "#]],
    );
}

#[test]
fn divergent_loop_emits_banner_with_label() {
    // Iteration (0) is just `H`; iteration (1) is `H` followed by `X`.
    // The shapes don't match — divergence finding expected.
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "loop: 0..1",
          "targets": [{ "qubit": 0 }],
          "children": [
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "(0)",
                  "targets": [{ "qubit": 0 }],
                  "children": [
                    {
                      "components": [
                        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
                      ]
                    }
                  ]
                }
              ]
            },
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "(1)",
                  "targets": [{ "qubit": 0 }],
                  "children": [
                    {
                      "components": [
                        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] },
                        { "kind": "unitary", "gate": "X", "targets": [{ "qubit": 0 }] }
                      ]
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0 }]
}"#,
        &expect![[r#"
            // NOTE: This Q# preview was reconstructed from a circuit trace and is approximate.
            // The original Q# source is the authoritative version.
            //   - loop has structurally divergent iterations: loop: 0..1
            /// Expects a qubit register of at least 1 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 1 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 1 qubits.";
                }
                // loop: 0..1
                // iteration (0)
                H(qs[0]);
                // iteration (1)
                H(qs[0]);
                X(qs[0]);
                // end loop
            }

        "#]],
    );
}

#[test]
fn simple_conditional_emits_no_banner() {
    // `if: c_0 == One` is a label the emitter could reproduce literally as
    // `if c_0 == One { ... }`, so no opaque-conditional finding is expected.
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "measurement", "gate": "M", "qubits": [{ "qubit": 0 }], "results": [{ "qubit": 0, "result": 0 }] }
      ]
    },
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "if: c_0 == One",
          "targets": [{ "qubit": 0 }, { "qubit": 0, "result": 0 }],
          "controls": [{ "qubit": 0, "result": 0 }],
          "isConditional": true,
          "children": [
            {
              "components": [
                { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
              ]
            }
          ]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0, "numResults": 1 }]
}"#,
        &expect![[r#"
            /// Expects a qubit register of at least 1 qubits.
            operation Test(qs : Qubit[]) : Result {
                if Length(qs) < 1 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 1 qubits.";
                }
                let c0_0 = M(qs[0]);
                // if: c_0 == One
                H(qs[0]);
                // end if
                return c0_0;
            }

        "#]],
    );
}

#[test]
fn opaque_conditional_emits_banner_with_label() {
    // Function-call / inequality labels can't be reproduced literally
    // because the trace lost the original Q# expression. Expect a banner.
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        { "kind": "measurement", "gate": "M", "qubits": [{ "qubit": 0 }], "results": [{ "qubit": 0, "result": 0 }] }
      ]
    },
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "if: (f(c_0)) > (2)",
          "targets": [{ "qubit": 0 }, { "qubit": 0, "result": 0 }],
          "controls": [{ "qubit": 0, "result": 0 }],
          "isConditional": true,
          "children": [
            {
              "components": [
                { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
              ]
            }
          ]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0, "numResults": 1 }]
}"#,
        &expect![[r#"
            // NOTE: This Q# preview was reconstructed from a circuit trace and is approximate.
            // The original Q# source is the authoritative version.
            //   - conditional uses an opaque expression: if: (f(c_0)) > (2)
            /// Expects a qubit register of at least 1 qubits.
            operation Test(qs : Qubit[]) : Result {
                if Length(qs) < 1 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 1 qubits.";
                }
                let c0_0 = M(qs[0]);
                // if: (f(c_0)) > (2)
                H(qs[0]);
                // end if
                return c0_0;
            }

        "#]],
    );
}

#[test]
fn divergence_banner_includes_source_line_when_available() {
    // When the structural group carries a `scopeLocation` in its metadata,
    // the banner surfaces the (1-indexed) line number so the reader can
    // jump to the construct in the original `.qs` source. The trace stores
    // line numbers 0-indexed, so a `"line": 5` should display as `line 6`.
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "if: (a + b) > 0",
          "targets": [{ "qubit": 0 }],
          "metadata": {
            "scopeLocation": { "file": "test.qs", "line": 5, "column": 4 },
            "controlResultIds": []
          },
          "children": [
            {
              "components": [
                { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
              ]
            }
          ]
        }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "X", "targets": [{ "qubit": 0 }] }
      ]
    }
  ],
  "qubits": [{ "id": 0 }]
}"#,
        &expect![[r#"
            // NOTE: This Q# preview was reconstructed from a circuit trace and is approximate.
            // The original Q# source is the authoritative version.
            //   - conditional uses an opaque expression (line 6): if: (a + b) > 0
            /// Expects a qubit register of at least 1 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 1 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 1 qubits.";
                }
                // if: (a + b) > 0
                H(qs[0]);
                // end if
                X(qs[0]);
            }

        "#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)] // long because the inline JSON describes two distinct loops
fn divergence_banner_is_per_finding_not_global() {
    // Two divergent loops at different points produce two findings, each
    // named individually. This confirms the banner doesn't collapse to a
    // single generic "something is wrong" line.
    check(
        r#"
{
  "componentGrid": [
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "loop: 0..1",
          "targets": [{ "qubit": 0 }],
          "children": [
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "(0)",
                  "targets": [{ "qubit": 0 }],
                  "children": [
                    {
                      "components": [
                        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] }
                      ]
                    }
                  ]
                }
              ]
            },
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "(1)",
                  "targets": [{ "qubit": 0 }],
                  "children": [
                    {
                      "components": [
                        { "kind": "unitary", "gate": "X", "targets": [{ "qubit": 0 }] }
                      ]
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    },
    {
      "components": [
        {
          "kind": "unitary",
          "gate": "loop: 0..1",
          "targets": [{ "qubit": 0 }],
          "children": [
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "(0)",
                  "targets": [{ "qubit": 0 }],
                  "children": [
                    {
                      "components": [
                        { "kind": "unitary", "gate": "Y", "targets": [{ "qubit": 0 }] }
                      ]
                    }
                  ]
                }
              ]
            },
            {
              "components": [
                {
                  "kind": "unitary",
                  "gate": "(1)",
                  "targets": [{ "qubit": 0 }],
                  "children": [
                    {
                      "components": [
                        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] },
                        { "kind": "unitary", "gate": "S", "targets": [{ "qubit": 0 }] }
                      ]
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }
  ],
  "qubits": [{ "id": 0 }]
}"#,
        &expect![[r#"
            // NOTE: This Q# preview was reconstructed from a circuit trace and is approximate.
            // The original Q# source is the authoritative version.
            //   - loop has structurally divergent iterations: loop: 0..1
            //   - loop has structurally divergent iterations: loop: 0..1
            /// Expects a qubit register of at least 1 qubits.
            operation Test(qs : Qubit[]) : Unit is Ctl + Adj {
                if Length(qs) < 1 {
                    fail "Invalid number of qubits. Operation Test expects a qubit register of at least 1 qubits.";
                }
                // loop: 0..1
                // iteration (0)
                H(qs[0]);
                // iteration (1)
                X(qs[0]);
                // end loop
                // loop: 0..1
                // iteration (0)
                Y(qs[0]);
                // iteration (1)
                Z(qs[0]);
                S(qs[0]);
                // end loop
            }

        "#]],
    );
}
