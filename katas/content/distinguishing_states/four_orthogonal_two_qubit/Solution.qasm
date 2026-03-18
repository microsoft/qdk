OPENQASM 3.0;
include "stdgates.inc";

qubit[2] qs;
output int result;
h qs[0];
h qs[1];
bit c0 = measure qs[0];
bit c1 = measure qs[1];
result = int(c0) * 2 + int(c1);
