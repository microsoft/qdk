OPENQASM 3.0;
include "stdgates.inc";

qubit[2] qs;
h qs[0];
ch qs[0], qs[1];
