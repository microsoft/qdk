OPENQASM 3.0;
include "stdgates.inc";

qubit q;
output bool result;
h q;
bit c = measure q;
result = !bool(c);
