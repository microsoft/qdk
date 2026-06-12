// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/*
Generator source for checked-in Rz synthesis lookup artifacts.

This script regenerates:
  - source/npm/qsharp/rz-array.json
  - source/npm/qsharp/rz-details.json

It is intentionally not on the normal build path. Keep this file in-repo so
the lookup data remains reproducible when synthesis parameters are updated.

Run with: npm run rz
*/

import { compare, Hadamard, Ident, M2x2, PauliX, TGate } from "../ux/cplx.js";
import { writeFileSync } from "node:fs";

// The below two constants define how many points in a 2*PI circle we want to find, and the
// tolerance for error off the plane (i.e. how much it may affect the 0 or 1 magnitude).

// An epsilon of 0.002 takes about 500M tries to find 1256 points, mostly containing around 50 - 60 gates
// An epsilon of 0.001 takes about 4.4B tries to find 1256 points, mostly containing around 64 gates
const points = Math.floor(Math.PI * 2 * 200); // 1256 points around the sphere
const epsilon = 0.001;

// We only find 1/8 of the total points, as the other 7/8 can be derived from the 1/8 using a simple
// T, S, ST, Z, ZT, S*, or T* addition, which is cheaper than any further Hadamard + T sequences.
if (points % 8 !== 0) throw "Points must be a multiple of 8";
const phaseCount = points / 8;
let phaseCountFound = 0;

// We store the the calculated matrices in a buffer for all permutations up to 2^24 operations.
// That way once the buffer is full, we can check beyond 2^24 operations by multiplying the pre-calculated matrices.
//
// i.e. If A is a matrix after multiplying "HTHTHT...", and B is a matrix after multiplying "HtHtHt...",
// then B.mul(A) is just one matrix multiplication that is equivalent to "HTHTHT...HtHtHt..."
//
// To optimze space and avoid saving the list of operations, we can simply map the index to a list of operations.
// The power of 2 indicates how many operations are in that index, i.e.
// - Indices 0 & 1 have 1 op (2 permutations)
// - Indices 2, 3, 4, 5 have 2 ops (4 permutations)
// - Index 6, 7, 8, 9, 10, 11, 12, 13 have 3 ops (8 permutations)
// - etc.
// The Math.log(x) function returns 0 for 1, 1 for 2, 2 for 4, 3 for 8, etc.
const opCountForIndex = (x: number) => Math.floor(Math.log2(x + 2));
const startIndexForOpCount = (ops: number) => 2 ** ops - 2;

function indexToOpSequence(idx: number): string[] {
  // We'll use op 'A' to indicate H then T, and 'B' to indicate H then T_adjoint.
  const result: string[] = [];
  const ops = opCountForIndex(idx);
  // Once we know how many ops we have, we can just iterate through the bits of the index
  for (let i = 0; i < ops; i++) {
    if (idx & 0x01) {
      result.push("A");
    } else {
      result.push("B");
    }
    idx >>= 1;
  }

  return result;
}

// Convert the op sequence (which may include 'A' and 'B') to the unitary gate sequence (which
// may include H, X, etc.). We'll also remove any HH sequences as they are no-ops.
function opSequenceToGateSequence(seq: string[]): string {
  return seq
    .map((op) => (op === "A" ? "HT" : op === "B" ? "Ht" : op))
    .join("")
    .replace(/HH/g, "");
}

// Pre-calculate the matrix for the Hadamard and T / T_adjoint sequences
const Aseq = TGate.mul(Hadamard);
const Bseq = TGate.adjoint().mul(Hadamard);

const MatrixLenBytes = 4 * 2 * 4; // 4 bytes in a float, 2 floats in a complex, 4 complex values in a 2x2 matrix
const EntryCount = startIndexForOpCount(25); // Test up to 2^24 permutations

console.log(`Matrix entries to pre-calculate: ${EntryCount}`);
console.log(`Using an espilon of ${epsilon}`);
console.log(`Phases to find: ${phaseCount}`);

// Buffer (and view) for the pre-calcuated matrices
const entryBuffer = new ArrayBuffer(EntryCount * MatrixLenBytes);
const bufferView = new DataView(entryBuffer);

const phaseEntries: Array<{
  phase: number;
  sequence: string;
  matrix: M2x2;
  offset: number;
}> = [];

// This is called once we find what looks like a valid Rz to see if we should save it.
function onPhaseFound(phase: number, ops: string[], matrix: M2x2) {
  const phaseIdx = Math.round((phase * points) / (2 * Math.PI));
  const segment = phaseIdx % phaseCount;

  if (
    phaseEntries[segment] === undefined ||
    matrix.b.mag() * 1.25 < phaseEntries[segment].offset // 25% error improvement is worth extra gates
  ) {
    // Track if we found a new phase point
    if (phaseEntries[segment] === undefined) ++phaseCountFound;
    phaseEntries[segment] = {
      phase,
      sequence: opSequenceToGateSequence(ops),
      matrix,
      offset: matrix.b.mag(),
    };
    checkEntry(phaseEntries[segment]);
  }
}

// Validate if the phase entry is correct as we go (mainly for debugging/verification)
function checkEntry(entry: { phase: number; sequence: string; matrix: M2x2 }) {
  let state = Ident.mul(1);
  for (const gate of entry.sequence) {
    if (gate === "H") {
      state = Hadamard.mul(state);
    } else if (gate === "X") {
      state = PauliX.mul(state);
    } else if (gate === "T") {
      state = TGate.mul(state);
    } else if (gate === "t") {
      state = TGate.adjoint().mul(state);
    }
  }
  if (!state.isDiagonal(epsilon)) {
    console.error(`offset error: `, entry);
  }
  if (!compare(state.phase(), entry.phase, epsilon)) {
    console.error(`Phase mismatch: ${state.phase()} != ${entry.phase}`, entry);
  }
}

function writeMatrixToBuffer(idx: number, matrix: M2x2) {
  const offset = idx * MatrixLenBytes;
  bufferView.setFloat32(offset + 0, matrix.a.re);
  bufferView.setFloat32(offset + 4, matrix.a.im);
  bufferView.setFloat32(offset + 8, matrix.b.re);
  bufferView.setFloat32(offset + 12, matrix.b.im);
  bufferView.setFloat32(offset + 16, matrix.c.re);
  bufferView.setFloat32(offset + 20, matrix.c.im);
  bufferView.setFloat32(offset + 24, matrix.d.re);
  bufferView.setFloat32(offset + 28, matrix.d.im);
}

// Rather than return a new matrix, which means allocating a new object, we'll just update
// an existing one passed as a parameter. This is a bit faster and saves on memory allocation
// if the caller passes in the same working matrix every time.
function setMatrixFromBuffer(idx: number, matrix: M2x2) {
  const offset = idx * MatrixLenBytes;
  matrix.a.re = bufferView.getFloat32(offset + 0);
  matrix.a.im = bufferView.getFloat32(offset + 4);
  matrix.b.re = bufferView.getFloat32(offset + 8);
  matrix.b.im = bufferView.getFloat32(offset + 12);
  matrix.c.re = bufferView.getFloat32(offset + 16);
  matrix.c.im = bufferView.getFloat32(offset + 20);
  matrix.d.re = bufferView.getFloat32(offset + 24);
  matrix.d.im = bufferView.getFloat32(offset + 28);
}

// This turns an array of gates into an array of gates, expanding any buffer indices in the array
// into the gate sequence. The ability to pass in a buffer index is an optimization to not have to
// allocate the list of gates as a string array on every iteration if it may not be needed.
function gatesList(gates: Array<string | number>): string[] {
  return gates
    .map((entry) => {
      if (typeof entry === "number") {
        return indexToOpSequence(entry);
      } else {
        return entry;
      }
    })
    .flat();
}

function checkMatrix(matrix: M2x2, gates: Array<string | number>) {
  // Here is where we check if the resulting matrix is a phase gate.
  // Basically, a phase gate is one where the off-diagonal elements are zero, and the diagonal
  // elements are 1 in magnitude. This means the 0 & 1 probabilities are not affected by the gate.
  // The 'relative phase' of the matrix is the difference in phase between the two diagonal elements.
  // The global phase is irrelevant if just doing a single gate operation.

  // All matrices come in with a trailing T or t, but that's of no interest as without that gate it's
  // already a phase operation, but with less gates. So we skip testing the matrix as-is, and only
  // with additional trailing gates.

  // Test with an additional Hadamard on the end
  const plusHEnd = Hadamard.mul(matrix);
  if (plusHEnd.isDiagonal(epsilon)) {
    onPhaseFound(plusHEnd.phase(), gatesList([...gates, "H"]), plusHEnd);
  }

  // It may well be that applying an X turns it into a valid Rz. So we check that too.
  // Note that due to the matrix math, Y will only find a hit if X did, so no point in checking Y.
  // (And obviously Z is already just a phase shift, so no need to check that either)
  const plusXEnd = PauliX.mul(matrix);
  if (plusXEnd.isDiagonal(epsilon)) {
    onPhaseFound(plusXEnd.phase(), gatesList([...gates, "X"]), plusXEnd);
  }

  // Both together (H & X) is worth a check. In testing this found some shorter sequences.
  const plusHXEnd = PauliX.mul(plusHEnd);
  if (plusHXEnd.isDiagonal(epsilon)) {
    onPhaseFound(plusHXEnd.phase(), gatesList([...gates, "H", "X"]), plusHXEnd);
  }
}

function fillBuffer(): boolean {
  // Fill the buffer up with pre-calculated matrices, testing each one for phase points as we go.
  for (let i = 0; i < EntryCount; i++) {
    if (phaseCountFound === phaseCount) return true;
    if (i % 1000000 === 0)
      console.log(
        `Processing entry ${i}. Intervals found: ${phaseCountFound}. Current gate length: ${opCountForIndex(i) * 2}`,
      );
    const gates = indexToOpSequence(i);
    const matrix = gates.reduce((acc, gate) => {
      if (gate === "A") {
        return Aseq.mul(acc);
      } else {
        return Bseq.mul(acc);
      }
    }, Ident.mul(1));
    writeMatrixToBuffer(i, matrix);
    checkMatrix(matrix, gates);
  }
  return false;
}

// Put a no-op in the buffer for Rz(0)
onPhaseFound(0, [], Ident.mul(1));

if (!fillBuffer()) {
  // Just filling the buffer didn't find all the intervals, so we need to start running further
  // permutations of the pre-calcuated matrices to find the remaining intervals.

  // No point having the first matrix be less than 24 gates, so the inner-loop will just run
  // over all permutations of the 24-gate matrices.
  const startFullGatesIndex = startIndexForOpCount(24);
  const endFullGatesIndex = startIndexForOpCount(25) - 1;

  // The working matrices will be reused for each multiplication, so we'll just create them once.
  const fullMatrix = Ident.mul(1);
  const tinyMatrix = Ident.mul(1);

  // Running counter starts where the pre-calculated list finished
  let i = EntryCount;

  // The nested loop will run over all the 24-gate matrices (inner loop), and multiply them with the
  // pre-calculated matrices starting from the shortest (outer loop)
  outer: for (let tinyIdx = 0; tinyIdx < startFullGatesIndex; tinyIdx++) {
    setMatrixFromBuffer(tinyIdx, tinyMatrix);
    for (
      let fullIdx = startFullGatesIndex;
      fullIdx <= endFullGatesIndex;
      fullIdx++
    ) {
      if (i % 1000000 === 0) {
        console.log(
          `Processing entry ${i}. Intervals found: ${phaseCountFound}.  Current gate length: ${(24 + opCountForIndex(tinyIdx)) * 2}`,
        );
      }
      ++i;
      setMatrixFromBuffer(fullIdx, fullMatrix);
      const matrix = tinyMatrix.mul(fullMatrix);
      checkMatrix(matrix, [fullIdx, tinyIdx]);
      if (phaseCountFound === phaseCount) break outer;
    }
  }
}

// Write a file with the details of each found rotation for manual inspection if needed
const result = phaseEntries.map((entry) =>
  entry
    ? {
        phase: entry.phase,
        gates: entry.sequence,
        matrix: entry.matrix.toShortString(),
        offset: entry.matrix.b.mag(),
      }
    : null,
);
writeFileSync("./rz-details.json", JSON.stringify(result, null, 2), "utf8");

// Create the array with the index being the phase points, and the value being the gate sequence
const phaseGates: string[] = new Array(points).fill("****");

// Here we fill in the other 7/8 of the points by adding T, S, ST, Z, ZT, S*, and T* to the 1/8 we found.
phaseEntries.forEach((entry) => {
  const phaseIdx = Math.round((entry.phase * points) / (2 * Math.PI));
  phaseGates[phaseIdx] = entry.sequence;

  const Tidx = (phaseIdx + phaseCount) % points;
  phaseGates[Tidx] = entry.sequence + "T";

  const Sidx = (phaseIdx + phaseCount * 2) % points;
  phaseGates[Sidx] = entry.sequence + "S";

  const STidx = (phaseIdx + phaseCount * 3) % points;
  phaseGates[STidx] = entry.sequence + "ST";

  const Zidx = (phaseIdx + phaseCount * 4) % points;
  phaseGates[Zidx] = entry.sequence + "Z";

  const ZTidx = (phaseIdx + phaseCount * 5) % points;
  phaseGates[ZTidx] = entry.sequence + "ZT";

  const sidx = (phaseIdx + phaseCount * 6) % points;
  phaseGates[sidx] = entry.sequence + "s";

  const tidx = (phaseIdx + phaseCount * 7) % points;
  phaseGates[tidx] = entry.sequence + "t";
});
writeFileSync("./rz-array.json", JSON.stringify(phaseGates, null, 1), "utf8");

console.log("DONE");
