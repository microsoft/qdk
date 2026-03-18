OPENQASM 3.0;
include "stdgates.inc";

qubit[2] qs;
ry(2.0 * arcsin(1.0 / sqrt(3.0))) qs[0];
negctrl @ h qs[0], qs[1];
