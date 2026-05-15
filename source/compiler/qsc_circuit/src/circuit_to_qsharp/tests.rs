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
