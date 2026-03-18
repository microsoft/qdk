OPENQASM 3.0;
include "stdgates.inc";

qubit[3] qs;
cx qs[0], qs[1];
cx qs[0], qs[2];
h qs[0];
h qs[1];
h qs[2];
