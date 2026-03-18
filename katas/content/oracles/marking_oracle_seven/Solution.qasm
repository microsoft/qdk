OPENQASM 3.0;
include "stdgates.inc";

qubit[3] inp;
qubit target;
ctrl(3) @ x inp[0], inp[1], inp[2], target;
