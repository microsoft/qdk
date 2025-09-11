use crate::tableau::StabilizerTableau;

use super::{Simulator, operation::*};
use expect_test::{Expect, expect};

impl Simulator {
    fn check_tableau(&self, expect: &Expect) {
        expect.assert_eq(&self.inv_state.to_string());
    }

    fn check_measurements(&self, expect: &Expect) {
        expect.assert_eq(&self.measurements_str());
    }
}

fn check_tableau(num_qubits: usize, gates: &[Operation], expect: &Expect) {
    let mut simulator = Simulator::new(num_qubits, Default::default());
    simulator.apply_gates(gates);
    expect.assert_eq(&simulator.inv_state.to_string());
}

#[test]
fn initial_state_1_qubits() {
    check_tableau(
        1,
        &[],
        &expect![[r#"
              | X0 Z0
            --+------
            ± | +  + 
            --+------
            0 | X  Z "#]],
    );
}

#[test]
fn initial_state_2_qubits() {
    check_tableau(
        2,
        &[],
        &expect![[r#"
          | X0 Z0 | X1 Z1
        --+-------+------
        ± | +  +  | +  + 
        --+-------+------
        0 | X  Z  | I  I 
        --+-------+------
        1 | I  I  | X  Z "#]],
    );
}

#[test]
fn identity_gate() {
    check_tableau(
        1,
        &[id(0)],
        &expect![[r#"
          | X0 Z0
        --+------
        ± | +  + 
        --+------
        0 | X  Z "#]],
    );
}

#[test]
fn x_gate() {
    check_tableau(
        1,
        &[x(0)],
        &expect![[r#"
          | X0 Z0
        --+------
        ± | +  - 
        --+------
        0 | X  Z "#]],
    );
}

#[test]
fn y_gate() {
    check_tableau(
        1,
        &[y(0)],
        &expect![[r#"
          | X0 Z0
        --+------
        ± | -  - 
        --+------
        0 | X  Z "#]],
    );
}

#[test]
fn z_gate() {
    check_tableau(
        1,
        &[z(0)],
        &expect![[r#"
          | X0 Z0
        --+------
        ± | -  + 
        --+------
        0 | X  Z "#]],
    );
}

#[test]
fn h_gate() {
    check_tableau(
        1,
        &[h(0)],
        &expect![[r#"
          | X0 Z0
        --+------
        ± | +  + 
        --+------
        0 | Z  X "#]],
    );
}

#[test]
fn s_gate() {
    check_tableau(
        1,
        &[s(0)],
        &expect![[r#"
          | X0 Z0
        --+------
        ± | +  + 
        --+------
        0 | Y  Z "#]],
    );
}

#[test]
fn cz_gate() {
    check_tableau(
        2,
        &[cz(0, 1)],
        &expect![[r#"
          | X0 Z0 | X1 Z1
        --+-------+------
        ± | +  +  | +  + 
        --+-------+------
        0 | X  Z  | Z  I 
        --+-------+------
        1 | Z  I  | X  Z "#]],
    );
}

#[test]
fn append_x_gate() {
    let mut tableau = StabilizerTableau::identity(1);
    expect![[r#"
          | X0 Z0
        --+------
        ± | +  + 
        --+------
        0 | X  Z "#]]
    .assert_eq(&tableau.to_string());

    let mut transposed_tableau = tableau.transpose();
    expect![[r#"
          | X0 Z0
        --+------
        ± | +  + 
        --+------
        0 | X  Z "#]]
    .assert_eq(&transposed_tableau.to_string());

    transposed_tableau.append_x(0);
    expect![[r#"
          | X0 Z0
        --+------
        ± | +  - 
        --+------
        0 | X  Z "#]]
    .assert_eq(&transposed_tableau.to_string());

    drop(transposed_tableau);
    expect![[r#"
          | X0 Z0
        --+------
        ± | +  - 
        --+------
        0 | X  Z "#]]
    .assert_eq(&tableau.to_string());
}

#[test]
fn deterministic_0_measurement() {
    // Initialize a simulator with a single qubit and no noise.
    let mut simulator = Simulator::new(1, Default::default());

    // Check initial tableau.
    simulator.check_tableau(&expect![[r#"
          | X0 Z0
        --+------
        ± | +  + 
        --+------
        0 | X  Z "#]]);

    // Measure qubit.
    simulator.apply_gate(&mz(0));

    // Check tableau.
    simulator.check_tableau(&expect![[r#"
          | X0 Z0
        --+------
        ± | +  + 
        --+------
        0 | X  Z "#]]);

    // Check measurements.
    simulator.check_measurements(&expect!["0"]);
}

#[test]
fn deterministic_1_measurement() {
    // Initialize a simulator with a single qubit and no noise.
    let mut simulator = Simulator::new(1, Default::default());

    // Apply X gate.
    simulator.apply_gate(&x(0));
    simulator.check_tableau(&expect![[r#"
          | X0 Z0
        --+------
        ± | +  - 
        --+------
        0 | X  Z "#]]);

    // Measure qubit.
    simulator.apply_gate(&mz(0));

    // Check tableau.
    simulator.check_tableau(&expect![[r#"
          | X0 Z0
        --+------
        ± | +  - 
        --+------
        0 | X  Z "#]]);

    // Check measurements.
    simulator.check_measurements(&expect!["1"]);
}

#[test]
fn transposed_tableau() {
    let mut tableau = StabilizerTableau::identity(2);
    tableau.prepend_h(0);
    tableau.prepend_s(1);
    tableau.prepend_cz(0, 1);
    expect![[r#"
          | X0 Z0 | X1 Z1
        --+-------+------
        ± | +  +  | +  + 
        --+-------+------
        0 | Z  X  | X  I 
        --+-------+------
        1 | Z  I  | Y  Z "#]]
    .assert_eq(&tableau.to_string());

    let transposed_tableau = tableau.transpose();
    expect![[r#"
          | X0 Z0 | X1 Z1
        --+-------+------
        ± | +  +  | +  + 
        --+-------+------
        0 | Z  X  | X  I 
        --+-------+------
        1 | Z  I  | Y  Z "#]]
    .assert_eq(&transposed_tableau.to_string());
}

#[test]
fn random_measurement() {
    // Initialize a simulator with a single qubit and no noise.
    let mut simulator = Simulator::new(2, Default::default());

    // Apply X gate.
    simulator.apply_gates(&[h(1), cz(1, 0), s(0), h(0)]);
    simulator.check_tableau(&expect![[r#"
          | X0 Z0 | X1 Z1
        --+-------+------
        ± | +  +  | +  + 
        --+-------+------
        0 | Z  Y  | Z  I 
        --+-------+------
        1 | I  X  | Z  X "#]]);

    // Measure qubit.
    simulator.apply_gate(&mz(0));

    // Check tableau.
    simulator.check_tableau(&expect![[r#"
          | X0 Z0 | X1 Z1
        --+-------+------
        ± | +  -  | -  - 
        --+-------+------
        0 | I  Z  | Z  X 
        --+-------+------
        1 | X  Y  | Z  I "#]]);

    // Check measurements.
    simulator.check_measurements(&expect!["1"]);
}
