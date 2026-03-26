OPENQASM 3.0;
include "stdgates.inc";

qubit[3] j;
h j[0];
ctrl @ p(pi / 2) j[1], j[0];
ctrl @ p(pi / 4) j[2], j[0];
