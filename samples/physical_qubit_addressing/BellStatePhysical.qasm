// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Physical qubit addressing in OpenQASM 3.
//
// This program treats QIR qubit IDs as fixed physical addresses on a target machine. The machine
// size is specific to the hardware you target; this example assumes 256 qubits, so size the
// register to the qubit count of the machine you target. The pattern relies on three facts:
//   1. The source declares the entire machine-sized qubit register exactly once, so its indices
//      map directly onto QIR qubit IDs.
//   2. Selecting register elements by explicit physical address fixes their intended QIR qubit IDs.
//   3. A provider target must contractually preserve those IDs. Source indices and QIR IDs alone
//      do not force a hardware layout; confirm the target's contract before relying on it.
//
// See README.md for the full physical-addressing guidance and the constraints below.

OPENQASM 3.0;
include "stdgates.inc";

// This is the only qubit declaration. Array indices are physical addresses.
qubit[256] machine;
output bit[8] results;

// Distinct pairs keep every physical qubit measured exactly once.
let phiPlusA = machine[12];
let phiPlusB = machine[173];
let phiMinusA = machine[40];
let phiMinusB = machine[190];
let psiPlusA = machine[71];
let psiPlusB = machine[205];
let psiMinusA = machine[99];
let psiMinusB = machine[233];

// Prepare every state before measuring any qubit.

// |Phi+> = (|00> + |11>) / sqrt(2)
reset phiPlusA;
reset phiPlusB;
h phiPlusA;
cx phiPlusA, phiPlusB;

// |Phi-> = (|00> - |11>) / sqrt(2)
reset phiMinusA;
reset phiMinusB;
h phiMinusA;
z phiMinusA;
cx phiMinusA, phiMinusB;

// |Psi+> = (|01> + |10>) / sqrt(2)
reset psiPlusA;
reset psiPlusB;
h psiPlusA;
x psiPlusB;
cx psiPlusA, psiPlusB;

// |Psi-> = (|01> - |10>) / sqrt(2)
reset psiMinusA;
reset psiMinusB;
h psiMinusA;
z psiMinusA;
x psiMinusB;
cx psiMinusA, psiMinusB;

// Base Profile requires measurements at the end of the program.
results[0] = measure phiPlusA;
results[1] = measure phiPlusB;
results[2] = measure phiMinusA;
results[3] = measure phiMinusB;
results[4] = measure psiPlusA;
results[5] = measure psiPlusB;
results[6] = measure psiMinusA;
results[7] = measure psiMinusB;
