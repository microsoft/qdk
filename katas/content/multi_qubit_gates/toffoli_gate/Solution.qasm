OPENQASM 3.0;
include "stdgates.inc";

qubit[3] qs;
ccx qs[0], qs[1], qs[2];
