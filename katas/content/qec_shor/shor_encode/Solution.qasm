OPENQASM 3.0;
include "stdgates.inc";

qubit[9] qs;
// Phase-flip encoding on qubits 0, 3, 6
cx qs[0], qs[3];
cx qs[0], qs[6];
h qs[0];
h qs[3];
h qs[6];
// Bit-flip encoding on each block of 3
cx qs[0], qs[1];
cx qs[0], qs[2];
cx qs[3], qs[4];
cx qs[3], qs[5];
cx qs[6], qs[7];
cx qs[6], qs[8];
