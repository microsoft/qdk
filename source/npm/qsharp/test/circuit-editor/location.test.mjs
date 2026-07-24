// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Location value-type tests — exercises the immutable `Location` value type that owns
// hierarchical-address parse/compose for the circuit editor. Pure-data, no JSDOM.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { Location } from "../../dist/ux/circuit-vis/data/location.js";

test("Location.root() exposes the root invariants", () => {
  const root = Location.root();
  assert.equal(root.isRoot, true);
  assert.equal(root.depth, 0);
  assert.equal(root.toString(), "");
  assert.equal(root.last(), null);
  // parse("") resolves to the same empty address.
  assert.equal(Location.parse("").equals(root), true);
});

test("Location.parse yields correctly-structured addresses and round-trips", () => {
  const one = Location.parse("0,1");
  assert.equal(one.depth, 1);
  assert.equal(one.isRoot, false);
  assert.deepEqual(one.last(), [0, 1]);

  const two = Location.parse("0,1-2,3");
  assert.equal(two.depth, 2);
  assert.deepEqual(two.last(), [2, 3]);

  for (const s of ["", "0,0", "5,7", "0,1-2,3", "1,2-3,4-5,6"]) {
    assert.equal(Location.parse(s).toString(), s);
  }
});

test("Location.parse throws on malformed input", () => {
  for (const bad of [
    "abc",
    "1",
    "1,2,3",
    "1,",
    ",1",
    "1,2-",
    "-1,2",
    "1,2--3,4",
  ]) {
    assert.throws(() => Location.parse(bad), /Invalid location/, bad);
  }
});

test("Location.parent drops the last segment and saturates at root", () => {
  assert.equal(Location.root().parent().toString(), "");
  assert.equal(Location.parse("0,1").parent().toString(), "");
  assert.equal(Location.parse("0,1-2,3").parent().toString(), "0,1");
  assert.equal(Location.parse("1,2-3,4-5,6").parent().toString(), "1,2-3,4");
});

test("Location.child appends a segment", () => {
  assert.equal(Location.root().child(0, 1).toString(), "0,1");
  assert.equal(Location.parse("0,0").child(1, 2).toString(), "0,0-1,2");
  assert.equal(Location.parse("0,1-2,3").child(4, 5).toString(), "0,1-2,3-4,5");
});

test("Location.child + parent round-trips", () => {
  const base = Location.parse("0,1-2,3");
  assert.equal(base.child(4, 5).parent().toString(), base.toString());
});

test("Location.equals compares by structural value", () => {
  assert.equal(Location.parse("0,1").equals(Location.parse("0,1")), true);
  assert.equal(Location.parse("0,1").equals(Location.parse("0,2")), false);
  assert.equal(Location.parse("0,1").equals(Location.parse("0,1-2,3")), false);
  assert.equal(Location.root().equals(Location.parse("")), true);
  assert.equal(Location.root().equals(Location.parse("0,1")), false);
});

test("Location.of(...) matches Location.parse", () => {
  assert.equal(Location.of().toString(), "");
  assert.equal(Location.of([0, 1]).toString(), "0,1");
  assert.equal(Location.of([0, 1], [2, 3]).toString(), "0,1-2,3");
});
