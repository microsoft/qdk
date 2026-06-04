// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/* Future enhancements (tracked in BLOCH_TODO.md alongside the
   ship-readiness work): an undo/step-back button, a slider to scrub
   through gate history, equator / Bloch-angle overlays, and synthesis
   of arbitrary points from H/T gates. The math for converting basis
   coefficients (a, b) to a Bloch-sphere point is:
     theta = 2 * acos(magnitude(a))
     phi   = arg(b) - arg(a), normalized to [0, 2 * PI)
*/

import { useEffect, useMemo, useRef, useState } from "preact/hooks";

import {
  BoxGeometry,
  CanvasTexture,
  ConeGeometry,
  CylinderGeometry,
  DirectionalLight,
  Group,
  LineSegments,
  Mesh,
  MeshBasicMaterial,
  MeshBasicMaterialParameters,
  MeshLambertMaterial,
  PerspectiveCamera,
  Scene,
  SphereGeometry,
  Sprite,
  SpriteMaterial,
  WebGLRenderer,
  WireframeGeometry,
} from "three";

import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

import {
  AppliedGate,
  Rotations,
  Ket0,
  vec2,
  PauliX,
  PauliY,
  PauliZ,
  SGate,
  TGate,
  Hadamard,
} from "./cplx.js";
import { Markdown } from "./renderers.js";
import { detectThemeChange, ensureTheme } from "./themeObserver.js";
import {
  MAX_GATE_SEQUENCE_LENGTH,
  sanitizeGateSequence,
  VALID_GATE_CODES,
} from "./blochGates.js";

import rzOps from "../rz-array.json";

// Two color palettes for parts of the scene we draw directly with WebGL
// (sphere material, label sprites). Picked by eye to stay legible against
// the typical VS Code light / dark / playground backgrounds. CSS-styled
// parts of the widget instead pull from the shared QDK theme tokens.
const lightThemeColors = {
  sphereColor: 0x404080,
  sphereBrightness: 2,
  sphereOpacity: 0.5,
  directionalLightBrightness: 0.25,
  markerColor: 0xc00000,
  sphereLinesOpacity: 0.2,
  labelCanvasColor: "#606080",
};

const darkThemeColors = {
  sphereColor: 0x8080c0,
  sphereBrightness: 1.6,
  sphereOpacity: 0.55,
  directionalLightBrightness: 0.35,
  markerColor: 0xff5050,
  sphereLinesOpacity: 0.35,
  labelCanvasColor: "#d0d0e0",
};

function colorsFor(isDark: boolean) {
  return isDark ? darkThemeColors : lightThemeColors;
}

/**
 * Axis names accepted by the renderer's animated rotation methods and by
 * `BlochRenderer.snapTo`. Distinct from x/y/z labels in the visualization;
 * these are the rotation primitives the renderer exposes.
 */
type RotationAxis = "X" | "Y" | "Z" | "H";

/**
 * Per-gate metadata used by both the visualization layer (to animate or
 * snap the sphere) and the math layer (to display the LaTeX equation and
 * update the basis-coefficient state vector). Keyed by the single-character
 * gate code (see `VALID_GATE_CODES`).
 *
 * Keeping one table avoids the previous duplication where the same code was
 * mentioned in a `switch` in the React component, a separate `gateLaTeX`
 * dictionary, and the `cplx` matrix imports.
 */
const gateInfo: Record<
  string,
  {
    /** Display name for the LaTeX equation header (e.g. "X", "S\u2020"). */
    display: string;
    /** The 2x2 matrix in the computational basis. */
    matrix: typeof PauliX;
    /** Pre-rendered LaTeX for the matrix used in the history pane. */
    latex: string;
    /** Which renderer rotation primitive to invoke. */
    rotateAxis: RotationAxis;
    /** Angle in radians (sign matters for adjoint variants). */
    rotateAngle: number;
  }
> = {
  X: {
    display: "X",
    matrix: PauliX,
    latex: "\\begin{bmatrix} 0 & 1 \\\\ 1 & 0 \\end{bmatrix}",
    rotateAxis: "X",
    rotateAngle: Math.PI,
  },
  Y: {
    display: "Y",
    matrix: PauliY,
    latex: "\\begin{bmatrix} 0 & -i \\\\ i & 0 \\end{bmatrix}",
    rotateAxis: "Y",
    rotateAngle: Math.PI,
  },
  Z: {
    display: "Z",
    matrix: PauliZ,
    latex: "\\begin{bmatrix} 1 & 0 \\\\ 0 & -1 \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: Math.PI,
  },
  S: {
    display: "S",
    matrix: SGate,
    latex:
      "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{i {\\pi \\over 2}} \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: Math.PI / 2,
  },
  s: {
    display: "S\u2020",
    matrix: SGate.adjoint(),
    latex:
      "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{-i {\\pi \\over 2}} \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: -Math.PI / 2,
  },
  T: {
    display: "T",
    matrix: TGate,
    latex:
      "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{i {\\pi \\over 4}} \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: Math.PI / 4,
  },
  t: {
    display: "T\u2020",
    matrix: TGate.adjoint(),
    latex:
      "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{-i {\\pi \\over 4}} \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: -Math.PI / 4,
  },
  H: {
    display: "H",
    matrix: Hadamard,
    latex:
      "{1 \\over \\sqrt{2}} \\begin{bmatrix} 1 & 1 \\\\ 1 & -1 \\end{bmatrix}",
    rotateAxis: "H",
    rotateAngle: Math.PI,
  },
};

// See https://gizma.com/easing/#easeInOutSine
function easeInOutSine(x: number) {
  return -(Math.cos(Math.PI * x) - 1) / 2;
}

function easeOutSine(x: number) {
  return Math.sin((x * Math.PI) / 2);
}

function hslToRgb(h: number, s: number, l: number) {
  let r, g, b;

  if (s === 0) {
    r = g = b = l; // achromatic
  } else {
    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    r = hueToRgb(p, q, h + 1 / 3);
    g = hueToRgb(p, q, h);
    b = hueToRgb(p, q, h - 1 / 3);
  }
  return (
    (Math.min(r * 255, 255) << 16) |
    (Math.min(g * 255, 255) << 8) |
    Math.min(b * 255, 255)
  );
}

function hueToRgb(p: number, q: number, t: number) {
  if (t < 0) t += 1;
  if (t > 1) t -= 1;
  if (t < 1 / 6) return p + (q - p) * 6 * t;
  if (t < 1 / 2) return q;
  if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
  return p;
}

function makeLabelSprite(text: string, fillStyle: string): Sprite {
  // Render the label into an offscreen canvas and use it as a sprite texture.
  // Sprites always face the camera, so labels stay legible as the user
  // orbits the sphere. No font asset, no async load, no extra three.js
  // example modules required.
  const size = 128;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d")!;
  ctx.font = "bold 96px sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillStyle = fillStyle;
  ctx.fillText(text, size / 2, size / 2);

  const texture = new CanvasTexture(canvas);
  const material = new SpriteMaterial({ map: texture, transparent: true });
  const sprite = new Sprite(material);
  // Scale chosen to roughly match the visual size of the previous 3D text
  // (size: 0.6 extrusion with bevel).
  sprite.scale.set(1.2, 1.2, 1);
  return sprite;
}

function createLabels(isDark: boolean): Sprite[] {
  // Positions preserved verbatim from the original FontLoader/TextGeometry
  // implementation so the labels land in exactly the same spots.
  const fill = colorsFor(isDark).labelCanvasColor;

  const xLabel = makeLabelSprite("x", fill);
  xLabel.position.set(0, 0, 6.4);

  const yLabel = makeLabelSprite("y", fill);
  yLabel.position.set(6.4, 0, 0);

  const zLabel = makeLabelSprite("z", fill);
  zLabel.position.set(0, 6.4, 0);

  return [xLabel, yLabel, zLabel];
}

// Default duration of a single gate animation, in milliseconds. The
// component exposes a live speed slider that overwrites
// `BlochRenderer.rotationTimeMs` directly; the rAF loop re-reads the
// value every frame so changes take effect mid-animation.
//
// Tuned by feel: 333ms (~3 gates/sec at 1x) is slow enough to actually
// follow each rotation visually. The slider runs 0.25x..4x, so users
// who want the original snappy 100ms-per-gate feel can dial it up to
// ~3.3x.
const DEFAULT_ROTATION_TIME_MS = 333;

class BlochRenderer {
  scene: Scene;
  camera: PerspectiveCamera;
  renderer: WebGLRenderer;
  controls: OrbitControls;
  qubit: Group;
  trail: Group;
  rotationAxis: Group;
  animationCallbackId = 0;
  // Per-renderer override for animation speed. Public so the React
  // component can mutate it from the speed slider without going through
  // a setter -- there is no derived state to keep in sync.
  rotationTimeMs = DEFAULT_ROTATION_TIME_MS;
  // Animation queue. Each entry wraps an `AppliedGate` (which carries the
  // interpolation path used by the animation loop) with an optional
  // `onComplete` callback fired the moment that gate's animation ends.
  // The play loop relies on this callback to chain to the next gate; the
  // one-off `applyGate` path doesn't need it.
  gateQueue: { gate: AppliedGate; onComplete?: () => void }[] = [];
  rotations: Rotations;
  // Stored so setTheme() can mutate the sphere material and swap out the
  // axis label sprites when light/dark changes after construction.
  sphereMaterial: MeshLambertMaterial;
  sphereLineMaterial: MeshBasicMaterialParameters;
  markerMaterial: MeshBasicMaterial;
  directionalLight: DirectionalLight;
  labelSprites: Sprite[] = [];
  isDark: boolean;

  constructor(canvas: HTMLCanvasElement, isDark: boolean) {
    this.rotations = new Rotations(64);
    this.isDark = isDark;
    const palette = colorsFor(isDark);

    const renderer = new WebGLRenderer({
      canvas,
      antialias: true,
      alpha: true,
    });

    const scene = new Scene();
    const camera = new PerspectiveCamera(
      30, // fov
      1, // aspect
      0.1, // near
      1000, // far
    );

    // In WebGL, Z is towards the camera (viewer looking towards -Z), Y is up, X is right
    // Position slightly towards the X and Y axis to give a 'canonical' view
    camera.position.x = 4;
    camera.position.y = 4;
    camera.position.z = 27;
    camera.lookAt(0, 0, 0);

    const light = new DirectionalLight(
      0xffffff,
      palette.directionalLightBrightness,
    );
    light.position.set(-1, 2, 4);
    scene.add(light);
    this.directionalLight = light;

    // Note that the orbit controls move the camera, they don't rotate the
    // scene, so the X, Y, and Z axis for the Bloch sphere remain fixed.
    const controls = new OrbitControls(camera, renderer.domElement);

    // Create a group to hold the qubit
    const qubit = new Group();

    // Add the main sphere
    const sphereGeometry = new SphereGeometry(5, 32, 16);
    const material = new MeshLambertMaterial({
      emissive: palette.sphereColor,
      emissiveIntensity: palette.sphereBrightness,
      transparent: true,
      opacity: palette.sphereOpacity,
    });
    this.sphereMaterial = material;
    const sphere = new Mesh(sphereGeometry, material);
    qubit.add(sphere);

    // Add the 'spin' direction marker
    const coneGeometry = new ConeGeometry(0.2, 0.75, 32);
    const coneMat = new MeshBasicMaterial({ color: palette.markerColor });
    this.markerMaterial = coneMat;
    const marker = new Mesh(coneGeometry, coneMat);
    marker.position.set(0, 5.125, 0.4);
    marker.rotateX(Math.PI / 2);
    qubit.add(marker);

    // Draw the wires on it
    const sphereWireGeometry = new SphereGeometry(5.1, 16, 16);
    const wireframe = new WireframeGeometry(sphereWireGeometry);
    const sphereLines = new LineSegments(wireframe);
    const materialProps = sphereLines.material as MeshBasicMaterialParameters;
    materialProps.depthTest = true;
    materialProps.opacity = palette.sphereLinesOpacity;
    materialProps.transparent = true;
    this.sphereLineMaterial = materialProps;
    qubit.add(sphereLines);
    scene.add(qubit);

    // Create a group to hold the trailing points
    const trail = new Group();
    scene.add(trail);

    // Add the axes
    const axisMaterial = new MeshBasicMaterial({ color: 0xe0d0c0 });
    const zAxis = new CylinderGeometry(0.075, 0.075, 12, 32, 8);
    const zAxisMesh = new Mesh(zAxis, axisMaterial);
    scene.add(zAxisMesh);

    const zPointer = new ConeGeometry(0.2, 0.8, 16);
    const zPointerMesh = new Mesh(zPointer, axisMaterial);
    zPointerMesh.position.set(0, 6, 0);
    scene.add(zPointerMesh);

    const yAxisMesh = new Mesh(zAxis, axisMaterial);
    yAxisMesh.rotateZ(Math.PI / 2);
    scene.add(yAxisMesh);
    const yPointerMesh = new Mesh(zPointer, axisMaterial);
    yPointerMesh.position.set(6, 0, 0);
    yPointerMesh.rotateZ(-Math.PI / 2);
    scene.add(yPointerMesh);

    const xAxisMesh = new Mesh(zAxis, axisMaterial);
    xAxisMesh.rotateX(Math.PI / 2);
    scene.add(xAxisMesh);
    const xPointerMesh = new Mesh(zPointer, axisMaterial);
    xPointerMesh.position.set(0, 0, 6);
    xPointerMesh.rotateX(Math.PI / 2);
    scene.add(xPointerMesh);

    const rotationAxis = new Group();
    const rotationAxisMaterial = new MeshLambertMaterial({
      emissive: 0x808080,
      emissiveIntensity: 1.5,
      transparent: true,
      opacity: 0.75,
    });
    const axisBox = new BoxGeometry(0.33, 0.33, 12.5);
    const axisBoxMesh = new Mesh(axisBox, rotationAxisMaterial);
    rotationAxis.add(axisBoxMesh);

    const fins = [
      [2, 0.25, 0.25, 0, 0, 5.75],
      [0.25, 2, 0.25, 0, 0, 5.75],
      [2, 0.25, 0.25, 0, 0, -5.75],
      [0.25, 0.25, 2, 0, 0, -5.75],
    ];

    fins.forEach((fin) => {
      const finBox = new BoxGeometry(fin[0], fin[1], fin[2]);
      const finBoxMesh = new Mesh(finBox, rotationAxisMaterial);
      finBoxMesh.position.set(fin[3], fin[4], fin[5]);
      rotationAxis.add(finBoxMesh);
    });

    // TODO: Only to be added when rotating
    // scene.add(rotationAxis);
    this.rotationAxis = rotationAxis;

    // See https://threejs.org/manual/#en/rendering-on-demand
    controls.addEventListener("change", () =>
      requestAnimationFrame(() => this.render()),
    );

    this.renderer = renderer;
    this.scene = scene;
    this.camera = camera;
    this.controls = controls;
    this.qubit = qubit;
    this.trail = trail;

    // Labels are now synchronous, so just create them and render once.
    this.labelSprites = createLabels(isDark);
    this.labelSprites.forEach((s) => scene.add(s));
    this.render();
  }

  queueGate(gate: AppliedGate, onComplete?: () => void) {
    this.gateQueue.push({ gate, onComplete });
    if (this.animationCallbackId) return; // Queue is already running

    // Close over these values for the running queue
    let currentEntry:
      | { gate: AppliedGate; onComplete?: () => void }
      | undefined;
    let startTime = 0;

    const processQueue = () => {
      if (!currentEntry) {
        currentEntry = this.gateQueue.shift();
        if (!currentEntry) {
          // Queue was empty. Done
          this.animationCallbackId = 0;
          return;
        } else {
          const axisInLocal = this.qubit.worldToLocal(currentEntry.gate.axis);
          this.rotationAxis.lookAt(axisInLocal);
          this.qubit.add(this.rotationAxis);
          startTime = performance.now();
        }
      }

      // Calculate the percent of rotation time elapsed from start to now
      const x = (performance.now() - startTime) / this.rotationTimeMs;

      // Ease the rotation
      const t = x < 1 ? easeInOutSine(x) : 1;

      // Rotate the qubit to the correct position
      const currentRotation = this.rotations.getRotationAtPercent(
        currentEntry.gate,
        t,
      );

      currentRotation.path.forEach((val) => {
        // Draw any that don't already have a point
        if (val.ref) return;
        const trackGeo = new SphereGeometry(0.05, 16, 16);
        const trackBall = new Mesh(
          trackGeo,
          new MeshBasicMaterial({ color: 0x808080 }),
        );
        trackBall.position.set(0, 5, 0);

        // Conver to world space
        trackBall.position.applyQuaternion(val.pos);

        // Save along with the interpolation point
        this.trail.add(trackBall);
        val.ref = trackBall;
      });

      // Set qubit position to slerped values
      this.qubit.quaternion.copy(currentRotation.pos);

      // Fade out the path trail as needed
      this.trail.children.forEach((child, idx, arr) => {
        const ball = child as Mesh;
        const sat = easeOutSine((idx + 1) / arr.length);
        const color = hslToRgb(0.6, sat, 0.5);
        ball.material = new MeshBasicMaterial({ color });
        ball.scale.setScalar(sat + 0.5);
      });

      this.render();

      // If that gate is done, unset it and fire the completion callback.
      // The callback may queue another gate (that's exactly how the play
      // loop chains): in that case `queueGate` sees a live
      // `animationCallbackId` and just appends to the queue, and the
      // rAF we schedule below will pick it up next frame.
      if (t >= 1) {
        const finishedCb = currentEntry.onComplete;
        currentEntry = undefined;
        this.qubit.remove(this.rotationAxis);
        this.render();
        finishedCb?.();
      }

      this.animationCallbackId = requestAnimationFrame(processQueue);
    };

    // Kick off processing
    processQueue();
  }

  /**
   * Animate a single gate by axis + angle, optionally invoking
   * `onComplete` when the rotation finishes. This is the seam the play
   * loop in the React component uses to chain gates without having to
   * know about `AppliedGate` / `Rotations`.
   */
  animateStep(axis: RotationAxis, angle: number, onComplete?: () => void) {
    let applied: AppliedGate;
    switch (axis) {
      case "X":
        applied = this.rotations.rotateX(angle);
        break;
      case "Y":
        applied = this.rotations.rotateY(angle);
        break;
      case "Z":
        applied = this.rotations.rotateZ(angle);
        break;
      case "H":
        applied = this.rotations.rotateH(angle);
        break;
    }
    this.queueGate(applied, onComplete);
  }

  rotateX(angle: number) {
    this.queueGate(this.rotations.rotateX(angle));
  }

  rotateY(angle: number) {
    this.queueGate(this.rotations.rotateY(angle));
  }

  rotateZ(angle: number) {
    this.queueGate(this.rotations.rotateZ(angle));
  }

  rotateH(angle: number) {
    this.queueGate(this.rotations.rotateH(angle));
  }

  reset() {
    this.controls.reset();
    this.rotations.reset();
    this.trail.clear();
    this.scene.position.set(0, 0, 0);
    this.qubit.rotation.set(0, 0, 0);
    this.camera.position.set(4, 4, 27);
    this.camera.lookAt(0, 0, 0);
    this.render();
  }

  /**
   * Apply a sequence of rotations instantly with no animation. The
   * dotted trail showing the qubit's path through each rotation is
   * reconstructed from the same interpolation points the animated path
   * (`queueGate`) uses, so the visible result is identical to what the
   * user would see if they had clicked the gates one at a time and
   * waited for each animation to finish. Used by the "inspect history"
   * UI and undo/redo paths where the user wants to see a specific past
   * state without sitting through replay animations.
   */
  snapTo(steps: { axis: RotationAxis; angle: number }[]) {
    // Cancel any in-flight animation so its render callback doesn't fight
    // us by writing the in-progress quaternion back over our snap.
    if (this.animationCallbackId) {
      cancelAnimationFrame(this.animationCallbackId);
      this.animationCallbackId = 0;
    }
    this.gateQueue.length = 0;
    this.trail.clear();
    // The rotation-axis indicator group is added to the qubit only while
    // a gate is animating; detach it in case we're cancelling mid-flight.
    this.qubit.remove(this.rotationAxis);

    // Reset the underlying rotation model, then apply each step. We keep
    // the AppliedGate returned by each call so we can rebuild the trail
    // from its interpolation path -- otherwise navigating history would
    // erase the dotted trace the user was following.
    this.rotations.reset();
    this.qubit.quaternion.identity();
    for (const { axis, angle } of steps) {
      let applied;
      switch (axis) {
        case "X":
          applied = this.rotations.rotateX(angle);
          break;
        case "Y":
          applied = this.rotations.rotateY(angle);
          break;
        case "Z":
          applied = this.rotations.rotateZ(angle);
          break;
        case "H":
          applied = this.rotations.rotateH(angle);
          break;
      }
      // Same trackball construction as the animation loop in queueGate.
      // We deliberately do not set val.ref here: these are throwaway
      // visuals owned by the snap (cleared on the next snapTo), not the
      // long-lived references the animation path uses to skip
      // already-drawn points.
      for (const val of applied.path) {
        const trackGeo = new SphereGeometry(0.05, 16, 16);
        const trackBall = new Mesh(
          trackGeo,
          new MeshBasicMaterial({ color: 0x808080 }),
        );
        trackBall.position.set(0, 5, 0);
        trackBall.position.applyQuaternion(val.pos);
        this.trail.add(trackBall);
      }
    }
    // Apply the same age-based fade the animation loop applies on every
    // frame so the trail looks the same whether it was drawn step by
    // step or rebuilt in one shot.
    this.trail.children.forEach((child, idx, arr) => {
      const ball = child as Mesh;
      const sat = easeOutSine((idx + 1) / arr.length);
      const color = hslToRgb(0.6, sat, 0.5);
      ball.material = new MeshBasicMaterial({ color });
      ball.scale.setScalar(sat + 0.5);
    });
    this.qubit.quaternion.copy(this.rotations.currPosition);
    this.render();
  }

  render() {
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
  }

  setTheme(isDark: boolean) {
    if (this.isDark === isDark) return;
    this.isDark = isDark;
    const palette = colorsFor(isDark);

    this.sphereMaterial.emissive.setHex(palette.sphereColor);
    this.sphereMaterial.emissiveIntensity = palette.sphereBrightness;
    this.sphereMaterial.opacity = palette.sphereOpacity;
    this.sphereMaterial.needsUpdate = true;

    this.markerMaterial.color.setHex(palette.markerColor);
    this.markerMaterial.needsUpdate = true;

    if (this.sphereLineMaterial.opacity !== undefined) {
      this.sphereLineMaterial.opacity = palette.sphereLinesOpacity;
    }

    this.directionalLight.intensity = palette.directionalLightBrightness;

    // Canvas-backed sprite textures are baked at the colors they were
    // generated with; the cheapest correct fix is to recreate them.
    this.labelSprites.forEach((sprite) => {
      this.scene.remove(sprite);
      // Both texture and material are disposable; clean up before
      // releasing the reference to keep WebGL resources tidy.
      sprite.material.map?.dispose();
      sprite.material.dispose();
    });
    this.labelSprites = createLabels(isDark);
    this.labelSprites.forEach((s) => this.scene.add(s));

    this.render();
  }

  dispose() {
    // Stop any in-flight animation frame so it doesn't try to render into
    // a context we're about to throw away.
    if (this.animationCallbackId) {
      cancelAnimationFrame(this.animationCallbackId);
      this.animationCallbackId = 0;
    }
    this.controls.dispose();
    // Walk every Mesh in the scene and release its geometry/material/textures.
    // three.js doesn't do this automatically; failing to do so accumulates
    // GPU memory and (more visibly) eats WebGL contexts on every remount.
    this.scene.traverse((obj) => {
      const mesh = obj as Mesh;
      if (mesh.geometry) mesh.geometry.dispose();
      const mat = mesh.material as
        | { map?: { dispose: () => void }; dispose?: () => void }
        | { map?: { dispose: () => void }; dispose?: () => void }[]
        | undefined;
      if (Array.isArray(mat)) {
        mat.forEach((m) => {
          m.map?.dispose();
          m.dispose?.();
        });
      } else if (mat) {
        mat.map?.dispose();
        mat.dispose?.();
      }
    });
    this.labelSprites.forEach((sprite) => {
      sprite.material.map?.dispose();
      sprite.material.dispose();
    });
    this.labelSprites = [];
    this.renderer.dispose();
  }
}

export interface BlochSphereProps {
  /** Sequence of single-character gate codes to replay on mount. Each
   * character must be one of the gate keys understood by `rotate` (X, Y, Z,
   * H, S, s, T, t); see `VALID_GATE_CODES`. Unknown characters are silently
   * dropped and the total length is capped (`MAX_GATE_SEQUENCE_LENGTH`),
   * so it is safe to pass straight from an untrusted URL parameter. */
  initialGates?: string;
  /** Called whenever the applied-gate sequence changes (gate applied, gates
   * applied in bulk via Run, or reset). The argument is the full sequence
   * of single-character gate codes applied so far. Parents can use this to
   * keep a URL or other external state in sync. */
  onGatesChanged?: (gates: string) => void;
}

export function BlochSphere(props: BlochSphereProps = {}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const renderer = useRef<BlochRenderer | null>(null);

  // The widget's interaction model is a time-travel history:
  //
  //   * `gates` is the canonical list of single-character gate codes that
  //     have been applied to |0\u27e9, in order. It is the only durable state;
  //     everything else (sphere position, displayed state vectors,
  //     history rows) is derived from it.
  //   * `cursor` is the current viewing position, in [0, gates.length].
  //     0 means "at the initial |0\u27e9 state", gates.length means "at the
  //     end of the applied sequence". Values in between put the widget
  //     into *inspect mode*: the sphere shows that intermediate state
  //     without truncating the future part of the sequence.
  //   * `redoStack` holds gates discarded by Undo, in the order they would
  //     be re-applied by Redo. It is cleared whenever a new gate is
  //     applied (standard time-travel semantics).
  //
  // Inspect mode (cursor < gates.length) is signalled visually by a
  // persistent banner, dimmed/italicised future rows, and disabled
  // Undo/Redo buttons. Applying a new gate while inspecting commits the
  // truncation (future rows become discarded). This mirrors how
  // browsers and most editors handle "navigate back, then act".
  const [gates, setGates] = useState<string[]>([]);
  const [cursor, setCursor] = useState(0);
  const [redoStack, setRedoStack] = useState<string[]>([]);
  const [rzAngle, setRzAngle] = useState(0);

  // Playback state for the media-player-style controls. Stored both as
  // React state (drives button labels and disabled flags) and as a ref
  // (read from inside animation-completion callbacks, which capture
  // their value at call time so plain state would go stale).
  //
  // `animatingToIndexRef` tracks the history index the in-flight
  // animation is heading toward, so that Pause can snap there cleanly
  // instead of leaving the sphere mid-rotation. Null when nothing is
  // animating.
  const [isPlaying, setIsPlaying] = useState(false);
  const isPlayingRef = useRef(false);
  const animatingToIndexRef = useRef<number | null>(null);

  // Playback speed multiplier. 1× is the original 100ms-per-gate
  // default; 0.25× → 400ms, 4× → 25ms. We push the value straight into
  // `renderer.current.rotationTimeMs` on change so live-dragging the
  // slider during a Play actually affects the in-flight animation
  // (the rAF loop re-reads the field every frame). A small visual jump
  // is possible when the speed changes mid-rotation -- elapsed-time
  // arithmetic doesn't carry over -- but it's not worth the extra
  // state-tracking machinery to smooth out, given the slider is mainly
  // used between gates.
  const [speed, setSpeed] = useState(1);

  function speedChange(e: Event) {
    const slider = e.target as HTMLInputElement;
    const next = parseFloat(slider.value);
    setSpeed(next);
    if (renderer.current) {
      renderer.current.rotationTimeMs = DEFAULT_ROTATION_TIME_MS / next;
    }
  }

  // Live-editor state for the gate-string textbox.
  //
  //   * `draft === null` means the textbox is canonical -- it displays
  //     whatever the current `gates.join("")` is and updates
  //     automatically when gate buttons / undo / etc. change `gates`.
  //   * `draft !== null` means the user has been typing and the textbox
  //     value has diverged from `gates`. We hold the typed value here
  //     verbatim (no sanitization) so the per-char validation marker
  //     can show exactly what the user typed. Sanitization happens on
  //     commit.
  //
  // Commit (Enter or Run button): sanitize the draft, replace `gates`
  // wholesale, snap the sphere to start, and play through to the end
  // -- treating the textbox as "this is the program, please run it".
  // The redo stack is cleared, since editing invalidates undone history
  // the same way applying a new gate does.
  const [draft, setDraft] = useState<string | null>(null);
  const displayValue = draft ?? gates.join("");
  const hasUnsavedDraft = draft !== null && draft !== gates.join("");
  // Sanitized draft drives what commit() would actually apply, and also
  // what the char-count readout shows. Computed eagerly so the readout
  // matches what's about to happen, not the raw text the user typed.
  const sanitizedDraft = sanitizeGateSequence(displayValue).gates;
  const draftHasInvalid = displayValue
    .split("")
    .some((c) => !VALID_GATE_CODES.includes(c));
  // Either failure mode disables Run and paints the input red: the
  // sanitizer would silently drop something the user typed, and we'd
  // rather they fix it explicitly than press Run and wonder why their
  // sequence got shorter.
  const isDraftInvalid =
    draftHasInvalid || displayValue.length > MAX_GATE_SEQUENCE_LENGTH;

  // Convert a gate-code sequence to the format `BlochRenderer.snapTo`
  // expects. Keeping this as a small helper keeps the model/view seam
  // narrow: the renderer never has to know about gate codes.
  function gatesToSteps(codes: string[]) {
    return codes.map((c) => ({
      axis: gateInfo[c].rotateAxis,
      angle: gateInfo[c].rotateAngle,
    }));
  }

  useEffect(() => {
    if (!canvasRef.current) return;
    // Resolve the initial theme exactly once; subsequent changes are
    // delivered via detectThemeChange below.
    const initialIsDark = ensureTheme() ?? false;
    const r = new BlochRenderer(canvasRef.current, initialIsDark);
    renderer.current = r;
    // Replay any gates supplied via the URL on initial mount. We bypass
    // the regular `applyGate` path here for two reasons: (a) reading
    // back state inside a tight setState loop hits stale-closure bugs,
    // and (b) URL-driven mounts often have many gates and the renderer
    // queue would visibly chew through them. Instead, seed `gates`
    // directly and snap the renderer straight to the final state. The
    // user can navigate back through the history pane to see each step.
    if (props.initialGates) {
      const { gates: cleaned, modified } = sanitizeGateSequence(
        props.initialGates,
      );
      if (modified) {
        console.warn(
          `BlochSphere: ignored unknown gates or excess length in initialGates ` +
            `(input length ${props.initialGates.length}, applied ${cleaned.length}). ` +
            `Valid gate codes are: ${VALID_GATE_CODES}.`,
        );
      }
      if (cleaned) {
        const arr = cleaned.split("");
        setGates(arr);
        setCursor(arr.length);
        r.snapTo(gatesToSteps(arr));
        props.onGatesChanged?.(cleaned);
      }
    }
    // Wire live theme switches (e.g. user toggles VS Code light/dark while
    // the widget is open) through to the WebGL scene. CSS-styled parts of
    // the widget pick up the change automatically via theme tokens.
    const themeCleanup = detectThemeChange(document.body, (isDark) => {
      r.setTheme(isDark);
    });
    return () => {
      themeCleanup();
      r.dispose();
      renderer.current = null;
    };
  }, []);

  // Derived: per-step history entries (LaTeX strings) for the whole
  // `gates` sequence, walking the matrix product forward from |0\u27e9.
  // Computed in one pass instead of being mirrored in state, so the
  // history rows can never disagree with the underlying gate list.
  const historyEntries = useMemo(() => {
    let prior = vec2(Ket0);
    return gates.map((code, i) => {
      const info = gateInfo[code];
      const next = info.matrix.mulVec2(prior);
      const latex = `$$ ${info.display} | \\psi \\rangle_{${i}} =
        ${info.latex}
        \\cdot ${prior.toLaTeX()}
        = ${next.toLaTeX()}
        $$`;
      prior = next;
      return latex;
    });
  }, [gates]);

  const inInspectMode = cursor < gates.length;
  const canUndo = !inInspectMode && cursor > 0 && !isPlaying;
  const canRedo = !inInspectMode && redoStack.length > 0 && !isPlaying;
  // Playback affordances. These cover the media-control row; everything
  // is derived from `cursor` / `gates` / `isPlaying` so the buttons can
  // never disagree with what the sphere is actually doing.
  const atStart = cursor === 0;
  const atEnd = cursor >= gates.length;
  const canStepBack = !atStart;
  const canStepForward = !atEnd;
  const canPlay = gates.length > 0;

  /**
   * Cancel any in-flight playback animation and land cleanly on a
   * history step. Called by Pause directly, and called as a guard by
   * every editing or seeking action so the user can never "edit while
   * playing" or end up with the cursor stuck mid-rotation. No-op when
   * already stopped, so it is always safe to call.
   *
   * When called as a Pause (no follow-up seek), we snap forward to the
   * destination of the in-flight gate -- treating Pause as "finish the
   * current step, then stop". When called as a guard before another
   * seek (passed `snapToTarget=false`), we skip that snap because the
   * caller is about to snap somewhere else anyway.
   */
  function stopPlayback(snapToTarget = true) {
    if (!isPlayingRef.current) return;
    isPlayingRef.current = false;
    setIsPlaying(false);
    const targetIdx = animatingToIndexRef.current;
    animatingToIndexRef.current = null;
    if (snapToTarget && targetIdx !== null && renderer.current) {
      renderer.current.snapTo(gatesToSteps(gates.slice(0, targetIdx)));
      setCursor(targetIdx);
    }
  }

  /**
   * Animate one gate from the current sequence and, when the animation
   * completes, advance the cursor and chain to the next gate if play is
   * still active. Defined as a closure inside the component so it can
   * read `gates` and the refs directly; the recursive chain captures
   * `pos` per gate, so each callback knows which step it just finished.
   */
  function playFromIndex(pos: number) {
    if (!renderer.current) return;
    const code = gates[pos];
    const info = gateInfo[code];
    if (!info) {
      // Defensive: malformed gate code shouldn't be possible (the input
      // paths all run through sanitizeGateSequence), but if one slips
      // through we end playback cleanly rather than calling rotateX on
      // undefined.
      stopPlayback(false);
      return;
    }
    animatingToIndexRef.current = pos + 1;
    renderer.current.animateStep(info.rotateAxis, info.rotateAngle, () => {
      // We may have been paused while this gate was animating; in that
      // case Pause already advanced the cursor and we should not chain.
      // The ref check is belt-and-suspenders: snapTo cancels the rAF, so
      // in practice this callback won't fire after a pause -- but if it
      // ever does (e.g. callback fires the same tick pause clicks), we
      // bail.
      animatingToIndexRef.current = null;
      if (!isPlayingRef.current) return;
      const next = pos + 1;
      setCursor(next);
      if (next < gates.length) {
        playFromIndex(next);
      } else {
        isPlayingRef.current = false;
        setIsPlaying(false);
      }
    });
  }

  /**
   * Begin (or restart) playback from the current cursor through the end
   * of the sequence. If the cursor is already at the end, treat the
   * click as a Replay: snap to the start and play from there. Disabled
   * with no effect if the sequence is empty.
   */
  function play() {
    if (isPlayingRef.current || gates.length === 0 || !renderer.current) {
      return;
    }
    let startIdx = cursor;
    if (cursor >= gates.length) {
      // Replay: rewind to the start, then play.
      renderer.current.snapTo([]);
      setCursor(0);
      startIdx = 0;
    }
    isPlayingRef.current = true;
    setIsPlaying(true);
    playFromIndex(startIdx);
  }

  function pause() {
    stopPlayback(true);
  }

  function stepBack() {
    if (cursor === 0 || !renderer.current) return;
    stopPlayback(false);
    const target = cursor - 1;
    const r = renderer.current;
    // Make sure the renderer's pose and `rotations.currPosition` are
    // exactly at `cursor` before we animate. If a play just got
    // cancelled by stopPlayback they could otherwise be one gate ahead.
    r.snapTo(gatesToSteps(gates.slice(0, cursor)));
    // Animate the inverse of the last applied gate: same axis, negated
    // angle. For each gate primitive (X/Y/Z/H plus the angle-bearing
    // S/T variants), rotating by -angle around the same local axis is
    // the true inverse, so the qubit retraces the gate's arc backward
    // and lands exactly on the pose at `target`.
    //
    // Side effect during the animation: queueGate adds new trackballs
    // along the reverse path. Because the reverse traces the same arc
    // as the forward gate, those new dots visually overlap the existing
    // trail dots, so the user just sees the qubit gliding back along the
    // existing path. We clean them up in `onComplete` below by snapping
    // -- snapTo wipes the trail and rebuilds it for `[0..target-1]`.
    const info = gateInfo[gates[cursor - 1]];
    r.animateStep(info.rotateAxis, -info.rotateAngle, () => {
      r.snapTo(gatesToSteps(gates.slice(0, target)));
      setCursor(target);
    });
  }

  function stepForward() {
    if (cursor >= gates.length || !renderer.current) return;
    stopPlayback(false);
    const target = cursor + 1;
    const r = renderer.current;
    // Same guard as stepBack: align the renderer with `cursor` before
    // animating, so a half-finished play doesn't carry over.
    r.snapTo(gatesToSteps(gates.slice(0, cursor)));
    const info = gateInfo[gates[cursor]];
    r.animateStep(info.rotateAxis, info.rotateAngle, () => {
      setCursor(target);
    });
  }

  function jumpToStart() {
    stopPlayback(false);
    navigateTo(0);
  }

  function jumpToEnd() {
    stopPlayback(false);
    navigateTo(gates.length);
  }

  /**
   * Apply a single new gate to the sequence. If the user is currently
   * inspecting an earlier step, the future part of the sequence is
   * truncated (matching browser back-button + new-navigation semantics),
   * and the redo stack is cleared.
   */
  function applyGate(code: string) {
    const info = gateInfo[code];
    if (!info || !renderer.current) return;
    // Editing always stops playback first. We pass snapToTarget=false
    // because we're about to either snap (truncate-on-inspect path) or
    // start a fresh animation immediately -- the renderer's queue gets
    // cleared either way.
    stopPlayback(false);

    // Truncate future steps if the user is mid-inspect, then snap the
    // renderer there silently before kicking off the animated rotation
    // for the newly-applied gate.
    let base = gates;
    if (cursor < gates.length) {
      base = gates.slice(0, cursor);
      renderer.current.snapTo(gatesToSteps(base));
    }
    renderer.current.animateStep(info.rotateAxis, info.rotateAngle);
    const next = [...base, code];
    setGates(next);
    setCursor(next.length);
    setRedoStack([]);
    // Discard any uncommitted draft -- the gate button click is itself
    // an explicit edit, and leaving the draft in place would leave the
    // textbox showing a different program than the one now in `gates`.
    setDraft(null);
    props.onGatesChanged?.(next.join(""));
  }

  /**
   * Move the cursor to an arbitrary position in the existing sequence
   * without modifying it. Used by clicks on history rows and the
   * "Jump to latest" button. Snaps the renderer instantly (no animation
   * noise) because the user is inspecting, not acting.
   */
  function navigateTo(pos: number) {
    if (!renderer.current) return;
    if (pos < 0 || pos > gates.length) return;
    // Any deliberate seek (history-row click, jump button) implicitly
    // pauses playback. We pass snapToTarget=false because we're about
    // to snap to `pos` ourselves a couple of lines down.
    stopPlayback(false);
    renderer.current.snapTo(gatesToSteps(gates.slice(0, pos)));
    setCursor(pos);
  }

  /**
   * Remove the most recently applied gate and push it onto the redo stack.
   * Only valid when the cursor is at the end of the sequence; in inspect
   * mode the button is disabled so the user has to commit (apply a gate)
   * or leave inspect mode first.
   */
  function undo() {
    if (!canUndo || !renderer.current) return;
    stopPlayback(false);
    const removed = gates[gates.length - 1];
    const next = gates.slice(0, -1);
    renderer.current.snapTo(gatesToSteps(next));
    setGates(next);
    setCursor(next.length);
    setRedoStack([removed, ...redoStack]);
    // Drop any pending draft so the textbox stays consistent with the
    // sequence we just shortened.
    setDraft(null);
    props.onGatesChanged?.(next.join(""));
  }

  /**
   * Pop a gate from the redo stack and re-apply it. Snaps rather than
   * animating to stay visually symmetric with undo.
   */
  function redo() {
    if (!canRedo || !renderer.current) return;
    stopPlayback(false);
    const [restored, ...rest] = redoStack;
    const next = [...gates, restored];
    renderer.current.snapTo(gatesToSteps(next));
    setGates(next);
    setCursor(next.length);
    setRedoStack(rest);
    // Drop any pending draft so the textbox stays consistent with the
    // sequence we just re-extended.
    setDraft(null);
    props.onGatesChanged?.(next.join(""));
  }

  function reset() {
    stopPlayback(false);
    setGates([]);
    setCursor(0);
    setRedoStack([]);
    // Reset means "go back to a blank program"; the draft should match.
    setDraft(null);
    renderer.current?.reset();
    props.onGatesChanged?.("");
  }

  function applyGatesFromTextbox() {
    // The Run button (and the Enter key on the textbox) commit the
    // current displayed value. Treat this as "the textbox is a
    // program, please run it": replace gates wholesale, clear redo,
    // snap to the start, and play through to the end. The user
    // explicitly asked for replay semantics here -- even if the
    // sanitized result equals the existing gates list, we still snap
    // and play, because they pressed Run.
    if (!renderer.current) return;
    stopPlayback(false);
    const cleaned = sanitizedDraft;
    const arr = cleaned.split("");
    setGates(arr);
    setRedoStack([]);
    setDraft(null);
    props.onGatesChanged?.(cleaned);
    // Snap to start, then chain the play loop manually -- we can't go
    // through `play()` here because it reads `gates` from the closure
    // (which still has the pre-commit value until the next render).
    renderer.current.snapTo([]);
    setCursor(0);
    if (arr.length === 0) return;
    isPlayingRef.current = true;
    setIsPlaying(true);
    // Inline mini play loop: same shape as playFromIndex, but reads
    // `arr` from the local closure so we don't have to wait for React
    // to flush the gates update.
    const chain = (pos: number) => {
      if (!renderer.current) return;
      const code = arr[pos];
      const info = gateInfo[code];
      if (!info) {
        isPlayingRef.current = false;
        setIsPlaying(false);
        return;
      }
      animatingToIndexRef.current = pos + 1;
      renderer.current.animateStep(info.rotateAxis, info.rotateAngle, () => {
        animatingToIndexRef.current = null;
        if (!isPlayingRef.current) return;
        const next = pos + 1;
        setCursor(next);
        if (next < arr.length) {
          chain(next);
        } else {
          isPlayingRef.current = false;
          setIsPlaying(false);
        }
      });
    };
    chain(0);
  }

  function draftKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      applyGatesFromTextbox();
    } else if (e.key === "Escape") {
      e.preventDefault();
      setDraft(null);
    }
  }

  function draftInput(e: Event) {
    const input = e.target as HTMLInputElement;
    setDraft(input.value);
  }

  function sliderChange(e: Event) {
    const slider = e.target as HTMLInputElement;
    const angleIdx = Math.round(parseFloat(slider.value) * 200) % 1256;
    // Push the Rz decomposition into the live gate-string textbox as
    // a draft. The user can review it, edit it, and press Run/Enter to
    // replace gates and replay. We don't apply it directly because the
    // textbox is now the authoritative "program" input -- having the
    // slider rewrite `gates` silently would be confusing.
    setDraft(rzOps[angleIdx]);
    setRzAngle(parseFloat(slider.value));
  }

  return (
    <div style="position: relative;">
      <canvas ref={canvasRef} width="600" height="600"></canvas>
      <div
        class="qs-bloch-history"
        style="font-size: 0.8em; position: absolute; left: 600px; top: 50px; height: 700px; min-width: 220px; display: flex; flex-direction: column;"
      >
        <div class="qs-bloch-history-title">
          <span>History</span>
          {gates.length > 0 && (
            <span
              class="qs-bloch-history-step-counter"
              aria-live="polite"
              title={
                inInspectMode
                  ? "Viewing an earlier step. Apply a gate to discard later steps."
                  : "Current step / total steps"
              }
            >
              Step {cursor} / {gates.length}
            </span>
          )}
        </div>
        {/*
          Media-player-style transport controls. Layout left-to-right:
          jump-to-start, step-back, play/pause/replay, step-forward,
          jump-to-end. Step/jump buttons are seek-only (no animation) so
          they feel "instant"; the centre button is the only animated
          path and is also where Pause is wired. We render unicode media
          glyphs so the bar reads as the standard transport control even
          without colour or icons.
        */}
        <div class="qs-bloch-media-controls" role="group" aria-label="Playback">
          <button
            type="button"
            onClick={jumpToStart}
            disabled={!canStepBack}
            title="Jump to start"
            aria-label="Jump to start"
          >
            ⏮
          </button>
          <button
            type="button"
            onClick={stepBack}
            disabled={!canStepBack}
            title="Step back"
            aria-label="Step back"
          >
            ⏪
          </button>
          {isPlaying ? (
            <button
              type="button"
              onClick={pause}
              title="Pause"
              aria-label="Pause"
            >
              ⏸
            </button>
          ) : (
            <button
              type="button"
              onClick={play}
              disabled={!canPlay}
              title={atEnd ? "Replay from start" : "Play from here"}
              aria-label={atEnd ? "Replay from start" : "Play"}
            >
              {/* Show the circular-arrow "replay" glyph when the cursor
                  is at the end of the sequence, so users see at a glance
                  that clicking will rewind first. Otherwise show the
                  standard play triangle. */}
              {atEnd ? "\u21BB" : "\u23F5"}
            </button>
          )}
          <button
            type="button"
            onClick={stepForward}
            disabled={!canStepForward}
            title="Step forward"
            aria-label="Step forward"
          >
            ⏩
          </button>
          <button
            type="button"
            onClick={jumpToEnd}
            disabled={!canStepForward}
            title="Jump to end"
            aria-label="Jump to end"
          >
            ⏭
          </button>
        </div>
        {/*
          Speed slider. Lives directly below the transport bar so the two
          playback controls read as one cluster. Slider value IS the
          speed multiplier (higher = faster) which matches the natural
          mental model; the renderer translates it back to milliseconds.
        */}
        <div class="qs-bloch-speed-control">
          <label for="qs-bloch-speed-slider">Speed</label>
          <input
            id="qs-bloch-speed-slider"
            type="range"
            min="0.25"
            max="4"
            step="0.05"
            value={speed}
            onInput={speedChange}
            aria-label="Animation speed"
          />
          <span class="qs-bloch-speed-readout">{speed.toFixed(2)}×</span>
        </div>
        <div style="overflow-y: auto; flex: 1; display: flex; flex-direction: column; align-items: stretch; min-height: 0;">
          <div
            class={
              "qs-bloch-history-item" +
              (cursor === 0 ? " qs-bloch-history-item-current" : "") +
              (historyEntries.length === 0
                ? " qs-bloch-history-item-latest"
                : "")
            }
            title="Initial state |0⟩"
            onClick={() => navigateTo(0)}
          >
            <Markdown
              markdown={
                "$$ | \\psi \\rangle_0 = \\begin{bmatrix} 1 \\\\ 0 \\end{bmatrix} $$"
              }
            ></Markdown>
          </div>
          {historyEntries.map((str, i) => {
            const stepIndex = i + 1;
            const classes = ["qs-bloch-history-item"];
            if (stepIndex === cursor)
              classes.push("qs-bloch-history-item-current");
            if (stepIndex > cursor)
              classes.push("qs-bloch-history-item-future");
            // Pin the bottom-most row so the latest step stays visible
            // when the rest of the history scrolls. See the CSS rule
            // for `.qs-bloch-history-item-latest` for the mechanics.
            if (i === historyEntries.length - 1)
              classes.push("qs-bloch-history-item-latest");
            return (
              <div
                class={classes.join(" ")}
                title={`Go to step ${stepIndex}`}
                onClick={() => navigateTo(stepIndex)}
              >
                <Markdown markdown={str}></Markdown>
              </div>
            );
          })}
        </div>
      </div>
      <div class="qs-gate-buttons">
        <button
          type="button"
          onClick={() => applyGate("X")}
          disabled={isPlaying}
        >
          X
        </button>
        <button
          type="button"
          onClick={() => applyGate("Y")}
          disabled={isPlaying}
        >
          Y
        </button>
        <button
          type="button"
          onClick={() => applyGate("Z")}
          disabled={isPlaying}
        >
          Z
        </button>
        <button
          type="button"
          onClick={() => applyGate("H")}
          disabled={isPlaying}
        >
          H
        </button>
        <button
          type="button"
          onClick={() => applyGate("S")}
          disabled={isPlaying}
        >
          S
        </button>
        <button
          type="button"
          onClick={() => applyGate("s")}
          disabled={isPlaying}
        >
          S†
        </button>
        <button
          type="button"
          onClick={() => applyGate("T")}
          disabled={isPlaying}
        >
          T
        </button>
        <button
          type="button"
          onClick={() => applyGate("t")}
          disabled={isPlaying}
        >
          T†
        </button>

        <button
          style="margin-left: 8px;"
          type="button"
          onClick={undo}
          disabled={!canUndo}
          title={
            isPlaying
              ? "Pause to edit the sequence"
              : inInspectMode
                ? "Jump to latest to edit the sequence"
                : "Undo last gate"
          }
        >
          Undo
        </button>
        <button
          type="button"
          onClick={redo}
          disabled={!canRedo}
          title={
            isPlaying
              ? "Pause to edit the sequence"
              : inInspectMode
                ? "Jump to latest to edit the sequence"
                : "Redo last undone gate"
          }
        >
          Redo
        </button>
        <button
          style="margin-left: 8px;"
          type="button"
          onClick={reset}
          disabled={isPlaying}
          title={isPlaying ? "Pause to reset" : "Clear the entire history"}
        >
          Reset
        </button>
      </div>
      <div class="qs-bloch-gate-editor">
        <div class="qs-bloch-gate-editor-row">
          <input
            class={
              "qs-bloch-gate-editor-input" +
              (isDraftInvalid ? " qs-bloch-gate-editor-input-invalid" : "")
            }
            value={displayValue}
            onInput={draftInput}
            onKeyDown={draftKeyDown}
            placeholder="Type gates here (X Y Z H S s T t), Enter to run"
            disabled={isPlaying}
            spellcheck={false}
            autocomplete="off"
            autocorrect="off"
            autocapitalize="off"
            aria-label="Gate program"
            aria-invalid={isDraftInvalid}
          />
          <button
            style="margin-left: 8px; margin-right: 8px; padding: 0 8px"
            type="button"
            onClick={applyGatesFromTextbox}
            disabled={isPlaying || isDraftInvalid}
            title={
              isDraftInvalid
                ? "Fix invalid input before running"
                : "Apply this gate string and replay from the start"
            }
          >
            Run
          </button>
        </div>
        {/*
          Per-character validation feedback. Mirrors `displayValue` as a
          row of styled chars: valid gate codes render plain, invalid
          chars (anything outside `VALID_GATE_CODES`) get a red
          wavy underline, and chars beyond MAX_GATE_SEQUENCE_LENGTH also
          render as invalid to flag that they'll be dropped on Run.
        */}
        <div
          class={
            "qs-bloch-gate-editor-feedback" +
            (hasUnsavedDraft ? " qs-bloch-gate-editor-unsaved" : "")
          }
          aria-hidden="true"
        >
          <span class="qs-bloch-gate-editor-chars">
            {displayValue.split("").map((c, i) => {
              const valid =
                VALID_GATE_CODES.includes(c) && i < MAX_GATE_SEQUENCE_LENGTH;
              return (
                <span
                  class={
                    valid
                      ? "qs-bloch-gate-char"
                      : "qs-bloch-gate-char qs-bloch-gate-char-invalid"
                  }
                >
                  {c === " " ? "\u00B7" : c}
                </span>
              );
            })}
          </span>
          <span class="qs-bloch-gate-editor-status">
            {hasUnsavedDraft && (
              <span
                class="qs-bloch-gate-editor-unsaved-indicator"
                title="Unsaved changes \u2014 press Enter or Run to apply, Esc to discard"
              >
                — ● unsaved
              </span>
            )}
            <span
              class={
                draftHasInvalid ||
                displayValue.length > MAX_GATE_SEQUENCE_LENGTH
                  ? "qs-bloch-gate-editor-count qs-bloch-gate-editor-count-warn"
                  : "qs-bloch-gate-editor-count"
              }
              title={
                draftHasInvalid
                  ? `Invalid characters will be dropped on Run. Valid codes: ${VALID_GATE_CODES}`
                  : displayValue.length > MAX_GATE_SEQUENCE_LENGTH
                    ? `Sequence exceeds the ${MAX_GATE_SEQUENCE_LENGTH}-gate cap; excess will be dropped on Run`
                    : ""
              }
            >
              {sanitizedDraft.length} / {MAX_GATE_SEQUENCE_LENGTH}
            </span>
          </span>
        </div>
      </div>
      <div style="margin-top: 8px">
        <input
          aria-label="Rz"
          type="range"
          min="0"
          max="6.28"
          step="0.005"
          value={rzAngle}
          onInput={sliderChange}
        />
        <span style="margin: 0 12px; font-style: italic; font-size: 1.2em;">
          Rz({rzAngle})
        </span>
      </div>
    </div>
  );
}
