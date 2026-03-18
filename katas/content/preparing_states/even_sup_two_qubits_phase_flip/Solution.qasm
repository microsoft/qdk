OPENQASM 3.0;
include "stdgates.inc";

qubit[2] qs;
h qs[0];
h qs[1];
cz qs[0], qs[1];
