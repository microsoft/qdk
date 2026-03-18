OPENQASM 3.0;
include "stdgates.inc";

qubit qAlice;
qubit qBob;
h qAlice;
cx qAlice, qBob;
