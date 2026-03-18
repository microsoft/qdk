OPENQASM 3.0;
include "stdgates.inc";

qubit qAlice;
qubit qBob;
qubit qMessage;
cx qMessage, qAlice;
h qMessage;
ctrl @ z qMessage, qBob;
ctrl @ x qAlice, qBob;
