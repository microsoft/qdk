OPENQASM 3.0;
include "stdgates.inc";

qubit q;
output bool result;
bit c = measure q;
result = !bool(c);
