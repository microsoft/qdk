// # Sample
// Quantum Dynamics
//
// # Description
// This example demonstrates quantum dynamics in a style tailored for
// resource estimation. The sample is specifically the simulation
// of an Ising model Hamiltonian on an N1xN2 2D lattice using a
// fourth-order Trotter Suzuki product formula, assuming
// a 2D qubit architecture with nearest-neighbor connectivity.
// The is an example of a program that is not amenable to simulating
// classically, but can be run through resource estimation to determine
// what size of quantum system would be needed to solve the problem.

import Std.Math.*;
import Std.Arrays.*;

operation Main() : Unit {
    // n : Int, m : Int, t: Double, u : Double, tstep : Double

    let n = 10;
    let m = 10;

    let J = 1.0;
    let g = 1.0;

    let totTime = 30.0;
    let dt = 0.9;

    IsingModel2DSim(n, m, J, g, totTime, dt);
}

/// # Summary
/// The function below creates a sequence containing the rotation angles that will be applied with the two operators used in the expansion of the Trotter-Suzuki formula.
/// # Input
/// ## p (Double) : Constant used for fourth-order formulas
///
/// ## dt (Double) : Time-step used to compute rotation angles
///
/// ## J (Double) : coefficient for 2-qubit interactions
///
/// ## g (Double) : coefficient for transverse field
///
/// # Output
/// ## values (Double[]) : The list of rotation angles to be applies in sequence with the corresponding operators
///
function SetAngleSequence(p : Double, dt : Double, J : Double, g : Double) : Double[] {

    let len1 = 3;
    let len2 = 3;
    let valLength = 2 * len1 + len2 + 1;
    mutable values = [0.0, size = valLength];

    let val1 = J * p * dt;
    let val2 = -g * p * dt;
    let val3 = J * (1.0 - 3.0 * p) * dt / 2.0;
    let val4 = g * (1.0 - 4.0 * p) * dt / 2.0;

    for i in 0..len1 {

        if (i % 2 == 0) {
            set values w/= i <- val1;
        } else {
            set values w/= i <- val2;
        }

    }

    for i in len1 + 1..len1 + len2 {
        if (i % 2 == 0) {
            set values w/= i <- val3;
        } else {
            set values w/= i <- val4;
        }
    }

    for i in len1 + len2 + 1..valLength - 1 {
        if (i % 2 == 0) {
            set values w/= i <- val1;
        } else {
            set values w/= i <- val2;
        }
    }
    return values;
}

/// # Summary
/// Applies e^-iX(theta) on all qubits in the 2D lattice as part of simulating the transverse field in the Ising model
/// # Input
/// ## n (Int) : Lattice size for an n x n lattice
///
/// ## qArr (Qubit[][]) : Array of qubits representing the lattice
///
/// ## theta (Double) : The angle/time-step for which the unitary simulation is done.
///
operation ApplyAllX(n : Int, qArr : Qubit[][], theta : Double) : Unit {
    // This applies `Rx` with an angle of `2.0 * theta` to all qubits in `qs`
    // using partial application
    for row in 0..n - 1 {
        ApplyToEach(Rx(2.0 * theta, _), qArr[row]);
    }
}

/// # Summary
/// Applies e^-iP(theta) where P = Z o Z as part of the repulsion terms.
/// # Input
/// ## n, m (Int, Int) : Lattice sizes for an n x m lattice
///
/// ## qArr (Qubit[]) : Array of qubits representing the lattice
///
/// ## theta (Double) : The angle/time-step for which unitary simulation is done.
///
/// ## dir (Bool) : Direction is true for vertical direction.
///
/// ## grp (Bool) : Group is true for odd starting indices
///
operation ApplyDoubleZ(n : Int, m : Int, qArr : Qubit[][], theta : Double, dir : Bool, grp : Bool) : Unit {
    let start = grp ? 1 | 0;    // Choose either odd or even indices based on group number
    let P_op = [PauliZ, PauliZ];
    let c_end = dir ? m - 1 | m - 2;
    let r_end = dir ? m - 2 | m - 1;

    for row in 0..r_end {
        for col in start..2..c_end {
            // Iterate through even or odd columns based on `grp`

            let row2 = dir ? row + 1 | row;
            let col2 = dir ? col | col + 1;

            Exp(P_op, theta, [qArr[row][col], qArr[row2][col2]]);
        }
    }
}

/// # Summary
/// The main function that takes in various parameters and calls the operations needed to simulate fourth order Trotterizatiuon of the Ising Hamiltonian for a given time-step
/// # Input
/// ## N1, N2 (Int, Int) : Lattice sizes for an N1 x N2 lattice
///
/// ## J (Double) : coefficient for 2-qubit interactions
///
/// ## g (Double) : coefficient for transverse field
///
/// ## totTime (Double) : The total time-step for which unitary simulation is done.
///
/// ## dt (Double) : The time the simulation is done for each timestep
///
operation IsingModel2DSim(N1 : Int, N2 : Int, J : Double, g : Double, totTime : Double, dt : Double) : Unit {

    use qs = Qubit[N1 * N2];
    let qubitArray = Chunks(N2, qs); // qubits are re-arranged to be in an N1 x N2 array

    let p = 1.0 / (4.0 - 4.0^(1.0 / 3.0));
    let t = Ceiling(totTime / dt);

    let seqLen = 10 * t + 1;

    let angSeq = SetAngleSequence(p, dt, J, g);

    for i in 0..seqLen - 1 {
        let theta = (i == 0 or i == seqLen - 1) ? J * p * dt / 2.0 | angSeq[i % 10];

        // for even indexes
        if i % 2 == 0 {
            ApplyAllX(N1, qubitArray, theta);
        } else {
            // iterate through all possible combinations for `dir` and `grp`.
            for (dir, grp) in [(true, true), (true, false), (false, true), (false, false)] {
                ApplyDoubleZ(N1, N2, qubitArray, theta, dir, grp);
            }
        }
    }
}
