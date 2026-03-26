OPENQASM 3.0;
include "stdgates.inc";

qubit[3] j;

// Binary fraction in-place for j[0..]
h j[0];
ctrl @ p(pi / 2) j[1], j[0];
ctrl @ p(pi / 4) j[2], j[0];

// Binary fraction in-place for j[1..]
h j[1];
ctrl @ p(pi / 2) j[2], j[1];

// Binary fraction in-place for j[2..]
h j[2];

// Reverse qubit order
swap j[0], j[2];
