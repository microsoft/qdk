OPENQASM 3.0;
include "stdgates.inc";

qubit[2] qs;
ry(2.0 * arccos(sqrt(10.0 / 12.0))) qs[0];
negctrl @ ry(2.0 * arccos(3.0 / sqrt(10.0))) qs[0], qs[1];
ctrl @ ry(2.0 * pi / 4.0) qs[0], qs[1];
