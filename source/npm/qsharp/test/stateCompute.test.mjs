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

const getAmp = (ampMap, label) => ampMap[label] ?? { re: 0, im: 0 };
const mkQubit = (n = 1) => Array.from({ length: n }, (_, id) => ({ id }));
const colUnitaryAt = (gate, target, args, opts) => ({
  components: [
    {
      kind: "unitary",
      gate,
      targets: [{ qubit: target }],
      ...(opts?.controls?.length
        ? { controls: opts.controls.map((qubit) => ({ qubit })) }
        : null),
      args,
      ...(opts?.isAdjoint ? { isAdjoint: true } : null),
    },
  ],
});
const colUnitary = (gate, args, opts) => colUnitaryAt(gate, 0, args, opts);
const colReset0 = () => ({
  components: [{ kind: "ket", gate: "0", targets: [{ qubit: 0 }] }],
});

const assertAmp = (amp, re, im, eps = 1e-12) => {
  assert.ok(approxEq(amp.re, re, eps), `re expected ${re} got ${amp.re}`);
  assert.ok(approxEq(amp.im, im, eps), `im expected ${im} got ${amp.im}`);
};

// --------------------
// Adjoint tests
// --------------------

test("Single adjoint: S† on |1⟩ yields -i|1⟩", () => {
  const qubits = mkQubit();
  const componentGrid = [
    // Prepare |1⟩ then apply S†: S†|1⟩ = -i|1⟩
    colUnitary("X"),
    colUnitary("S", undefined, { isAdjoint: true }),
  ];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  assertAmp(getAmp(ampMap, "0"), 0, 0);
  assertAmp(getAmp(ampMap, "1"), 0, -1);
});

test("Single adjoint: T† on |1⟩ yields e^{-iπ/4}|1⟩", () => {
  const qubits = mkQubit();
  const componentGrid = [
    // Prepare |1⟩ then apply T†: T†|1⟩ = e^{-iπ/4}|1⟩
    colUnitary("X"),
    colUnitary("T", undefined, { isAdjoint: true }),
  ];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  assertAmp(getAmp(ampMap, "0"), 0, 0);
  assertAmp(getAmp(ampMap, "1"), Math.SQRT1_2, -Math.SQRT1_2, 1e-11);
});

test("Single adjoint: SX† on |0⟩ matches expected amplitudes", () => {
  const qubits = mkQubit();
  const componentGrid = [colUnitary("SX", undefined, { isAdjoint: true })];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  // SX†|0⟩ = (0.5-0.5i)|0⟩ + (0.5+0.5i)|1⟩
  assertAmp(getAmp(ampMap, "0"), 0.5, -0.5, 1e-11);
  assertAmp(getAmp(ampMap, "1"), 0.5, 0.5, 1e-11);
});

test("Gate then adjoint returns |0⟩ (Rx(π/3))", () => {
  const qubits = mkQubit();
  const componentGrid = [
    colUnitary("Rx", ["π/3"]),
    colUnitary("Rx", ["π/3"], { isAdjoint: true }),
  ];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  assertAmp(getAmp(ampMap, "0"), 1, 0, 1e-11);
  assertAmp(getAmp(ampMap, "1"), 0, 0, 1e-11);
});

test("Single adjoint: Ry†(π/3) from |0⟩ flips |1⟩ sign", () => {
  const qubits = mkQubit();
  const componentGrid = [colUnitary("Ry", ["π/3"], { isAdjoint: true })];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  const c = Math.cos(Math.PI / 6);
  const s = Math.sin(Math.PI / 6);
  assertAmp(getAmp(ampMap, "0"), c, 0, 1e-11);
  assertAmp(getAmp(ampMap, "1"), -s, 0, 1e-11);
});

test("Single adjoint: Rz†(π/3) on |1⟩ applies e^{-iπ/6}", () => {
  const qubits = mkQubit();
  const componentGrid = [
    colUnitary("X"),
    colUnitary("Rz", ["π/3"], { isAdjoint: true }),
  ];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  assertAmp(getAmp(ampMap, "0"), 0, 0);
  assertAmp(
    getAmp(ampMap, "1"),
    Math.cos(-Math.PI / 6),
    Math.sin(-Math.PI / 6),
    1e-11,
  );
});

test("Single adjoint: S† after H gives |0⟩ - i|1⟩ over √2", () => {
  const qubits = mkQubit();
  const componentGrid = [
    colUnitary("H"),
    colUnitary("S", undefined, { isAdjoint: true }),
  ];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  assertAmp(getAmp(ampMap, "0"), Math.SQRT1_2, 0, 1e-11);
  assertAmp(getAmp(ampMap, "1"), 0, -Math.SQRT1_2, 1e-11);
});

test("Single adjoint: controlled S† phases |11⟩ by -i", () => {
  const qubits = mkQubit(2);
  const componentGrid = [
    colUnitaryAt("X", 0),
    colUnitaryAt("X", 1),
    colUnitaryAt("S", 1, undefined, { controls: [0], isAdjoint: true }),
  ];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  assertAmp(getAmp(ampMap, "11"), 0, -1);
});

test("Single adjoint: controlled Rz† no-ops when control is |0⟩", () => {
  const qubits = mkQubit(2);
  const componentGrid = [
    // Prepare |01⟩ (control qubit 0 is 0, target qubit 1 is 1)
    colUnitaryAt("X", 1),
    colUnitaryAt("Rz", 1, ["π/3"], { controls: [0], isAdjoint: true }),
  ];
  const ampMap = computeAmpMapForCircuit(qubits, componentGrid, "big");
  assertAmp(getAmp(ampMap, "01"), 1, 0);
});

test("Reset is unsupported by stateCompute", () => {
  const qubits = mkQubit();
  const componentGrid = [colUnitary("H"), colReset0()];
  assert.throws(
    () => computeAmpMapForCircuit(qubits, componentGrid, "big"),
    /reset/i,
  );
});
