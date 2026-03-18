OPENQASM 3.0;
include "stdgates.inc";

qubit qAlice;
qubit qBob;
qubit qCharlie;
h qBob;
cx qBob, qCharlie;
h qAlice;
cx qAlice, qCharlie;
