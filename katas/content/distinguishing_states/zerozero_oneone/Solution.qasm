OPENQASM 3.0;
include "stdgates.inc";

qubit[2] qs;
output int result;
bit c = measure qs[0];
result = int(c);
