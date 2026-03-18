OPENQASM 3.0;
include "stdgates.inc";

qubit q;
output bool result;
ry(-2.0 * arctan(4.0 / 3.0)) q;
bit c = measure q;
result = !bool(c);
