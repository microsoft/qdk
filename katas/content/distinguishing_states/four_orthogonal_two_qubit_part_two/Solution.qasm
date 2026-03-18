OPENQASM 3.0;
include "stdgates.inc";

qubit[2] qs;
output int result;
h qs[1];
cx qs[0], qs[1];
h qs[0];
bit c0 = measure qs[0];
bit c1 = measure qs[1];
result = (1 - int(c1)) * 2 + (1 - int(c0));
