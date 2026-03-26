OPENQASM 3.0;
include "stdgates.inc";

qubit[3] qs;

// QFT on |000⟩ produces equal superposition of all basis vectors
h qs[0];
ctrl @ p(pi / 2) qs[1], qs[0];
ctrl @ p(pi / 4) qs[2], qs[0];

h qs[1];
ctrl @ p(pi / 2) qs[2], qs[1];

h qs[2];

swap qs[0], qs[2];
