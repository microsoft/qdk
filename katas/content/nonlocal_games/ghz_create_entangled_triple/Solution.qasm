OPENQASM 3.0;
include "stdgates.inc";

qubit[3] qs;
x qs[0];
x qs[1];
h qs[0];
h qs[1];
cz qs[0], qs[1];
// ApplyControlledOnBitString([false, true], X): flip qs[0], apply ccx, flip back
x qs[0];
ccx qs[0], qs[1], qs[2];
x qs[0];
// ApplyControlledOnBitString([true, false], X): flip qs[1], apply ccx, flip back
x qs[1];
ccx qs[0], qs[1], qs[2];
x qs[1];
