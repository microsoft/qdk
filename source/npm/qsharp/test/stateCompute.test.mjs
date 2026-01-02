import test from "node:test";
import assert from "node:assert/strict";
import { evaluateAngleExpression } from "../dist/ux/circuit-vis/stateCompute.js";

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
  assert.ok(approxEq(evaluateAngleExpression(".5"), 0.5));
  assert.ok(approxEq(evaluateAngleExpression("+.5"), 0.5));
  assert.ok(approxEq(evaluateAngleExpression("-.5"), -0.5));
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
});
