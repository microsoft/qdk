OPENQASM 3.0;
include "stdgates.inc";

qubit q;
qubit[2] j;
ctrl @ p(pi) j[0], q;
ctrl @ p(pi / 2) j[1], q;
