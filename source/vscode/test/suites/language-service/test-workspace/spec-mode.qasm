OPENQASM 3.0;
include "stdgates.inc";
defcalgrammar "openpulse";
qubit q;
defcal x $0 {
    delay[100ns] $0;
}
x q;
