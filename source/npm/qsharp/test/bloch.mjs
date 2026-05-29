import assert from "node:assert";
import { describe, it } from "node:test";

import {
  Ident,
  PauliX,
  PauliY,
  PauliZ,
  Hadamard,
  SGate,
  TGate,
  Cplx,
  vec2,
  m2x2,
  Ket0,
  Ket1,
  KetPlus,
  KetMinus,
  KetPlusI,
  KetMinusI,
  Rotations,
  compare,
} from "../dist/cplx.js";
import { Vector3 } from "three";

describe("Gate combos", () => {
  it("HZH† = X", () => {
    const HZHt = Hadamard.mul(PauliZ).mul(Hadamard.adjoint());
    assert(HZHt.compare(PauliX));
  });

  it("SS† = I", () => {
    const SSt = SGate.mul(SGate.adjoint());
    assert(SSt.compare(Ident));
  });

  it("TT = S", () => {
    const TT = TGate.mul(TGate);
    assert(TT.compare(SGate));
  });

  it("transposes", () => {
    const yTranspose = m2x2("0,i,-i,0");
    assert(PauliY.transpose().compare(yTranspose));
  });
});

describe("Gate application", () => {
  it("Applies Hadamard to |0>", () => {
    const result = Hadamard.mulVec2(Ket0);
    assert(result.compare(KetPlus));
  });

  it("Applies ZH to |0>", () => {
    const result = PauliZ.mulVec2(Hadamard.mulVec2(Ket0));

    assert(result.compare(KetMinus));
  });

  it("Applies XH to |0>", () => {
    const result = PauliX.mulVec2(Hadamard.mulVec2(Ket0));

    assert(result.compare(KetPlus));
  });

  it("Applies X to |0>", () => {
    const result = PauliX.mulVec2(Ket0);
    assert(result.compare(Ket1));
  });

  it("Applies Y to |0>", () => {
    const result = PauliY.mulVec2(Ket0);
    const expected = vec2("0,i");
    assert(result.compare(expected));
  });

  it("|0> lands in |+i> after Hadamard and SGate", () => {
    const Xplus = Hadamard.mulVec2(Ket0);
    const result = SGate.mulVec2(Xplus);
    assert(result.compare(KetPlusI));
  });

  it("|1> lands in |-i> after Hadamard and SGate", () => {
    const Xneg = Hadamard.mulVec2(Ket1);
    const result = SGate.mulVec2(Xneg);
    assert(result.compare(KetMinusI));
  });
});

describe("Math tests", () => {
  it("Checks tolerance inside bounds", () => {
    const a = new Cplx(1.0000002, 0);
    assert(Ident.a.compare(a));
  });

  it("Checks tolerance outside bounds", () => {
    const a = new Cplx(1.000002, 0);
    assert(!Ident.a.compare(a));
  });

  it("Checks matrix equality", () => {
    const mx = m2x2("1,0,0,1");
    assert(mx.compare(Ident));
  });

  it("Checks matrix inequality", () => {
    const mx = m2x2("1,0,0,i");
    assert(!mx.compare(Ident));
  });

  it("Accurately converts to and from polar", () => {
    const c1 = new Cplx(3.14, 2);
    const pol = c1.toPolar();
    const c2 = Cplx.fromPolar(pol.magnitude, pol.phase);
    assert(c1.compare(c2));
  });

  it("Checks the string representation of a complex number", () => {
    let a = new Cplx(1, 1);
    assert(a.toString() === "1+i");

    a = new Cplx(1, -1);
    assert(a.toString() === "1-i");

    a = new Cplx(1, 0);
    assert(a.toString() === "1");

    a = new Cplx(0, 1);
    assert(a.toString() === "i");

    a = new Cplx(0, -1);
    assert(a.toString() === "-i");

    a = new Cplx(0, 0);
    assert(a.toString() === "0");

    a = new Cplx(1, 1e-10);
    assert(a.toString() === "1");

    a = new Cplx(-1e-9, 1);
    assert(a.toString() === "i");

    a = new Cplx(1, 1).mul(Math.SQRT1_2);
    assert(a.toString() === "0.7071+0.7071i");
  });
});

describe("Rotation tests", () => {
  it("Rotates by X", () => {
    const qubit = new Rotations(50);
    assert(qubit.gates.length === 0);
    qubit.rotateX();
    assert(qubit.gates.length === 1);
    assert(qubit.gates[0].path.length === 50);
    const pos = qubit.currPosition;
    assert(compare(pos.w, 0));
    assert(compare(pos.x, 0));
    assert(compare(pos.y, 0));
    assert(compare(pos.z, 1));
  });

  it("Rotates by H", () => {
    const qubit = new Rotations();
    qubit.rotateH();
    const pos = qubit.currPosition;
    assert(compare(pos.w, 0));
    assert(compare(pos.x, 0));
    assert(compare(pos.y, Math.SQRT1_2));
    assert(compare(pos.z, Math.SQRT1_2));
  });

  it("Rotates by H then T twice", () => {
    const qubit = new Rotations(50);
    qubit.rotateH();
    qubit.rotateZ(Math.PI / 4);
    qubit.rotateZ(Math.PI / 4);
    assert(qubit.gates[0].name === "H");
    assert(qubit.gates[1].name === "T");
    assert(qubit.gates[1].path.length === 12);
    assert(qubit.gates[2].name === "T");

    const zeroPos = new Vector3(0, 1, 0);
    zeroPos.applyQuaternion(qubit.currPosition);

    assert(compare(zeroPos.x, 1));
    assert(compare(zeroPos.y, 0));
    assert(compare(zeroPos.z, 0));
  });

  it("Gets the path length of Pi", () => {
    const qubit = new Rotations();
    qubit.rotateH(); // Put the point on the equator

    // Calculate the path length of rotating half way around the Bloch Z axis
    const pathLen = qubit.getPathLength(new Vector3(0, 1, 0), Math.PI);
    assert(compare(pathLen, Math.PI));
  });

  it("Gets the path length of 0", () => {
    const qubit = new Rotations();

    // Calculate the path length of rotating half way around the Bloch Z axis
    const pathLen = qubit.getPathLength(new Vector3(0, 1, 0), Math.PI);
    assert(compare(pathLen, 0));
  });

  it("Gets the path length of a T gate", () => {
    const qubit = new Rotations();
    qubit.rotateH(); // Put the point on the equator
    qubit.rotateZ(Math.PI / 4); // Rotate by T

    // Calculate the path length of rotating by T again
    const pathLen = qubit.getPathLength(new Vector3(0, 1, 0), Math.PI / 4);
    assert(compare(pathLen, Math.PI / 4));
  });

  it("Gets the path length of a Y rotation after a T gate", () => {
    const qubit = new Rotations();
    qubit.rotateH(); // Put the point on the equator
    qubit.rotateZ(Math.PI / 4); // Rotate by T

    // Calculate the path length of rotating around the Bloch X axis
    const pathLen = qubit.getPathLength(new Vector3(0, 0, 2), Math.PI);

    // Radius is 1 / sqrt 2, and circumference is Pi
    const expected = Math.PI * Math.SQRT1_2;
    assert(compare(pathLen, expected));
  });

  it("Rotates by -T", () => {
    const qubit = new Rotations(64);
    qubit.rotateH();
    const minusTDistance = qubit.getPathLength(
      new Vector3(0, 1, 0),
      -Math.PI / 4,
    );
    assert(compare(minusTDistance, Math.PI / 4));

    qubit.rotateZ(-Math.PI / 4);
    assert(qubit.gates[1].path.length === 16);

    const bitPos = new Vector3(0, 1, 0);
    bitPos.applyQuaternion(qubit.currPosition);
    assert(compare(bitPos.x, -Math.SQRT1_2));
    assert(compare(bitPos.y, 0));
    assert(compare(bitPos.z, Math.SQRT1_2));
  });
});

describe("It has correct path entries", () => {
  it("returns the first point", () => {
    const qubit = new Rotations(64);
    qubit.rotateY();
    assert(qubit.gates.length === 1);
    assert(qubit.gates[0].path.length === 64);

    const after1Percent = qubit.getRotationAtPercent(qubit.gates[0], 0.01);
    assert(after1Percent.path.length === 1);

    const after50Percent = qubit.getRotationAtPercent(qubit.gates[0], 0.49);
    assert(after50Percent.path.length === 32);

    const after99Percent = qubit.getRotationAtPercent(qubit.gates[0], 0.99);
    assert(after99Percent.path.length === 64);
  });
});
