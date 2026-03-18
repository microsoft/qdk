OPENQASM 3.0;
include "stdgates.inc";

qubit[3] qs;
ctrl(2) @ z qs[0], qs[1], qs[2];
