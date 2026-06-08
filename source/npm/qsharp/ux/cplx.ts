// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Three.js-aware extension of `./quantum-math.js`. The complex-number,
// 2-vector, 2x2-matrix, and gate-matrix primitives live in
// `quantum-math.ts` (worker-safe, no `three` import); this file adds the
// quaternion-driven `Rotations` engine that powers the Bloch sphere
// animation. We re-export everything from `quantum-math` so existing
// consumers (notably `bloch.tsx` and `tools/rz-synthesis.ts`) keep
// working without changing their import paths.

import { Quaternion, Vector3 } from "three";
import { compare, numToStr } from "./quantum-math.js";

export * from "./quantum-math.js";

// Holds a set of rotations for a qubit, and the points in that rotation
export type AppliedGate = {
  name: string;
  axis: Vector3;
  angle: number;
  path: { pos: Quaternion; ref?: any }[];
  endPos: Quaternion;
};

export type PathEntry = { pos: Quaternion; ref?: any };

export class Rotations {
  gates: AppliedGate[] = [];
  currPosition = new Quaternion();

  constructor(
    public pointsPerRotation = 32, // Assuming a common gate rotation of pi radians
    public timePerGateMs = 500,
  ) {}

  reset() {
    this.gates = [];
    this.currPosition = new Quaternion();
  }

  getPathLength(axis: Vector3, rotationAngle: number): number {
    /*
       To calculate the distance a point travels around a unit sphere as a rotation is applied.
       - Calculate the angle (theta) between the axis of rotation and the point
       - Get the radius for the circle around the (unit) sphere at theta
       - Calculate the distance travelled as the rotation angle * radius
    */

    const pointStart = new Vector3(0, 1, 0);
    const pointCurrent = pointStart.applyQuaternion(this.currPosition);
    const pointToAxisAngle = pointCurrent.angleTo(axis);
    const arcRadius = Math.sin(pointToAxisAngle);
    const pathTravelled = arcRadius * rotationAngle;
    return Math.abs(pathTravelled);
  }

  applyGate(name: string, axis: Vector3, angle: number): AppliedGate {
    // Get the target position by applying the rotation to the current position
    const endPos = new Quaternion()
      .setFromAxisAngle(axis, angle)
      .multiply(this.currPosition);

    const pathDistance = this.getPathLength(axis, angle);
    const pointCount = Math.floor(
      (pathDistance * this.pointsPerRotation) / Math.PI,
    );

    // Generate a set of points between the current and target position
    const path: PathEntry[] = [];
    for (let i = 0; i < pointCount; i++) {
      const t = i / pointCount;
      path.push({ pos: this.currPosition.clone().slerp(endPos, t) });
    }
    const gate = { name, path, endPos, axis, angle };
    this.gates.push(gate);

    // Update the current position to the final target
    this.currPosition = endPos;
    return gate;
  }

  rotateX(angle?: number): AppliedGate {
    const name = angle === undefined ? "X" : `X(${numToStr(angle)})`;
    if (angle === undefined) angle = Math.PI;
    // The Bloch sphere X axis is the Z axis in WebGL
    return this.applyGate(name, new Vector3(0, 0, 1), angle);
  }
  rotateY(angle?: number): AppliedGate {
    const name = angle === undefined ? "Y" : `Y(${numToStr(angle)})`;
    if (angle === undefined) angle = Math.PI;
    // The Bloch sphere Y axis is the X axis in WebGL
    return this.applyGate(name, new Vector3(1, 0, 0), angle);
  }

  rotateZ(angle?: number): AppliedGate {
    const name =
      angle === undefined
        ? "Z"
        : compare(angle, Math.PI / 2)
          ? "S"
          : compare(angle, Math.PI / 4)
            ? "T"
            : `Z(${numToStr(angle)})`;
    if (angle === undefined) angle = Math.PI;
    // The Bloch sphere Z axis is the Y axis in WebGL
    return this.applyGate(name, new Vector3(0, 1, 0), angle);
  }

  rotateH(angle?: number): AppliedGate {
    const name = angle === undefined ? "H" : `H(${numToStr(angle)})`;
    if (angle === undefined) angle = Math.PI;
    // Bloch sphere X & Z axes are the Y and Z axes in WebGL
    const hAxis = new Vector3(0, 1, 1).normalize();
    return this.applyGate(name, hAxis, angle);
  }

  getRotationAtPercent(
    gate: AppliedGate,
    percent: number,
  ): {
    pos: Quaternion;
    path: PathEntry[];
  } {
    if (percent < 0 || percent > 1) throw Error("Invalid percent");

    // If there is no path, it didn't move. Start and end are the same
    if (!gate.path.length) return { pos: gate.endPos.clone(), path: [] };

    // Get the path up until this percent. Note that the first element is at
    // 0%, and the 100% has no entry. For example, if the path has 4 entries
    // these are at 0, 0.25, 0.5, and 0.75 of the rotation path.

    const stepSize = 1 / gate.path.length;
    const steps = Math.floor(percent / stepSize);

    // As the first point is at 0%, add one (unless at 100%)
    const path = gate.path.slice(0, Math.min(steps + 1, gate.path.length));
    return {
      pos: gate.path[0].pos.clone().slerp(gate.endPos, percent),
      path,
    };
  }
}
