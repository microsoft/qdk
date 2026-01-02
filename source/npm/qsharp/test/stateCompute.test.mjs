import test from "node:test";
import assert from "node:assert/strict";
import {
  evaluateAngleExpression,
  computeAmpMapForCircuit,
} from "../dist/ux/circuit-vis/stateCompute.js";

const approxEq = (a, b, eps = 1e-12) => Math.abs(a - b) <= eps;

test("π and pi keyword", () => {
  assert.ok(approxEq(evaluateAngleExpression("π"), Math.PI));
  assert.ok(approxEq(evaluateAngleExpression("+π"), Math.PI));
  assert.ok(approxEq(evaluateAngleExpression("-π"), -Math.PI));
  assert.ok(approxEq(evaluateAngleExpression("pi"), Math.PI));
  assert.ok(approxEq(evaluateAngleExpression("+pi"), Math.PI));
  assert.ok(approxEq(evaluateAngleExpression("-pi"), -Math.PI));
  assert.ok(approxEq(evaluateAngleExpression("Pi"), Math.PI));
  assert.ok(approxEq(evaluateAngleExpression("+Pi"), Math.PI));
  assert.ok(approxEq(evaluateAngleExpression("-Pi"), -Math.PI));
  assert.ok(approxEq(evaluateAngleExpression("PI"), Math.PI));
  assert.ok(approxEq(evaluateAngleExpression("+PI"), Math.PI));
  assert.ok(approxEq(evaluateAngleExpression("-PI"), -Math.PI));
});

test("basic numbers", () => {
  assert.ok(approxEq(evaluateAngleExpression("5"), 5));
  assert.ok(approxEq(evaluateAngleExpression("+5"), 5));
  assert.ok(approxEq(evaluateAngleExpression("-5"), -5));
  assert.ok(approxEq(evaluateAngleExpression("3.5"), 3.5));
  assert.ok(approxEq(evaluateAngleExpression("+3.5"), 3.5));
  assert.ok(approxEq(evaluateAngleExpression("-3.5"), -3.5));
  assert.ok(approxEq(evaluateAngleExpression("5."), 5));
  assert.ok(approxEq(evaluateAngleExpression("+5."), 5));
  assert.ok(approxEq(evaluateAngleExpression("-5."), -5));
});

test("arithmetic operations", () => {
  assert.ok(approxEq(evaluateAngleExpression("π/2"), Math.PI / 2));
  assert.ok(approxEq(evaluateAngleExpression("-π/2"), -Math.PI / 2));
  assert.ok(approxEq(evaluateAngleExpression("2*pi"), 2 * Math.PI));
  assert.ok(approxEq(evaluateAngleExpression("π + 2 - 3"), Math.PI - 1));
  assert.ok(approxEq(evaluateAngleExpression("2 * (pi / 4)"), Math.PI / 2));
});

test("parentheses nesting", () => {
  assert.ok(approxEq(evaluateAngleExpression("((π))"), Math.PI));
});

test("invalid inputs return undefined", () => {
  assert.equal(evaluateAngleExpression("++π"), undefined);
  assert.equal(evaluateAngleExpression("--π"), undefined);
  assert.equal(evaluateAngleExpression("π // 2"), undefined);
  assert.equal(evaluateAngleExpression("1..2"), undefined);
  assert.equal(evaluateAngleExpression("(π"), undefined);
  assert.equal(evaluateAngleExpression("π / 0"), undefined); // Infinity -> undefined
  assert.equal(evaluateAngleExpression(""), undefined);
  assert.equal(evaluateAngleExpression(".5"), undefined);
  assert.equal(evaluateAngleExpression("+.5"), undefined);
  assert.equal(evaluateAngleExpression("-.5"), undefined);
});

// --------------------
// Reset phase-preservation tests
// --------------------

const getAmp = (ampMap, label) => ampMap[label] ?? { re: 0, im: 0 };
const mkQubit = () => [{ id: 0 }];
const colUnitary = (gate, args) => ({
  components: [{ kind: "unitary", gate, targets: [{ qubit: 0 }], args }],
});
const colReset0 = () => ({
  components: [{ kind: "ket", gate: "0", targets: [{ qubit: 0 }] }],
});

test("Reset preserves phase after X", () => {
  const qubits = mkQubit();
  const componentGrid = [colUnitary("X"), colReset0()];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  const a0 = getAmp(ampMap, "0");
  assert.ok(approxEq(a0.re, 1));
  assert.ok(approxEq(a0.im, 0));
});

test("Reset preserves phase after Y", () => {
  const qubits = mkQubit();
  const componentGrid = [colUnitary("Y"), colReset0()];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  const a0 = getAmp(ampMap, "0");
  assert.ok(approxEq(a0.re, 0));
  assert.ok(approxEq(a0.im, 1));
});

test("Reset after H yields real +1", () => {
  const qubits = mkQubit();
  const componentGrid = [colUnitary("H"), colReset0()];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  const a0 = getAmp(ampMap, "0");
  assert.ok(approxEq(a0.re, 1));
  assert.ok(approxEq(a0.im, 0));
});

test("Reset tie-case uses sum direction when magnitudes tie", () => {
  const qubits = mkQubit();
  const componentGrid = [colUnitary("H"), colUnitary("Z"), colReset0()];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  const a0 = getAmp(ampMap, "0");
  assert.ok(approxEq(a0.re, 1));
  assert.ok(approxEq(a0.im, 0));
});

test("Reset after Ry(π/2) yields real +1", () => {
  const qubits = mkQubit();
  const componentGrid = [colUnitary("Ry", ["π/2"]), colReset0()];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  const a0 = getAmp(ampMap, "0");
  assert.ok(approxEq(a0.re, 1));
  assert.ok(approxEq(a0.im, 0));
});

test("Reset after Rz(π/2) preserves phase e^{-iπ/4}", () => {
  const qubits = mkQubit();
  const componentGrid = [colUnitary("Rz", ["π/2"]), colReset0()];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  const a0 = getAmp(ampMap, "0");
  const re = Math.SQRT1_2; // √2/2
  const im = -Math.SQRT1_2; // -√2/2
  assert.ok(approxEq(a0.re, re));
  assert.ok(approxEq(a0.im, im));
});
