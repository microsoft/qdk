// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/* The math for converting basis coefficients (a, b) to a Bloch-sphere
   point is:
     theta = 2 * acos(magnitude(a))
     phi   = arg(b) - arg(a), normalized to [0, 2 * PI)
*/

import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { ComponentChildren } from "preact";

import {
  BoxGeometry,
  BufferGeometry,
  CanvasTexture,
  ConeGeometry,
  CylinderGeometry,
  DirectionalLight,
  Group,
  Line,
  LineBasicMaterial,
  Mesh,
  MeshBasicMaterial,
  MeshLambertMaterial,
  PerspectiveCamera,
  Scene,
  SphereGeometry,
  Sprite,
  SpriteMaterial,
  Vector3,
  WebGLRenderer,
} from "three";

import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

import {
  AppliedGate,
  Rotations,
  Ket0,
  Vec2,
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
    /** Pre-rendered LaTeX for the matrix used in the trace pane. */
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
  // depthWrite off: the sprite is a mostly-transparent quad, and writing
  // depth for the whole quad would punch a box-shaped hole in the grid
  // circles drawn behind it (the transparent corners still occlude). The
  // glyph itself still shows because the sprite renders after the lines.
  const material = new SpriteMaterial({
    map: texture,
    transparent: true,
    depthWrite: false,
  });
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

// Markdown for the initial |0> state shown as the first trace row. Kept
// as a module constant so the trace list and the hidden width-probe
// (see `widthProbe`) render exactly the same source.
const INITIAL_KET_MARKDOWN =
  "$$ | \\psi \\rangle_0 = \\begin{bmatrix} 1 \\\\ 0 \\end{bmatrix} $$";

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
  sphereLineMaterial: LineBasicMaterial;
  markerMaterial: MeshBasicMaterial;
  directionalLight: DirectionalLight;
  labelSprites: Sprite[] = [];
  isDark: boolean;
  // Shared GPU resources for trail dots. Without these, every replayed gate
  // (snapTo) and every animation frame (queueGate) used to allocate a fresh
  // SphereGeometry + MeshBasicMaterial per path point -- ~3.2k geometries and
  // ~6.4k materials per click for a 50-gate sequence -- which is what was
  // freezing the UI on trace-row clicks.
  private trailDotGeometry!: SphereGeometry;
  // Pre-built palette of fade colors. Trail-dot age maps to an index via
  // `getTrailDotMaterial`; lookups replace per-dot `new MeshBasicMaterial`.
  private trailDotMaterials!: MeshBasicMaterial[];

  constructor(canvas: HTMLCanvasElement, isDark: boolean) {
    this.rotations = new Rotations(64);
    this.isDark = isDark;

    // Build the shared trail-dot resources up front so the hot paths in
    // snapTo/queueGate are pure object linking, not allocation.
    this.trailDotGeometry = new SphereGeometry(0.05, 16, 16);
    const TRAIL_PALETTE_SIZE = 32;
    this.trailDotMaterials = [];
    for (let i = 0; i < TRAIL_PALETTE_SIZE; i++) {
      const sat = i / (TRAIL_PALETTE_SIZE - 1);
      this.trailDotMaterials.push(
        new MeshBasicMaterial({ color: hslToRgb(0.6, sat, 0.5) }),
      );
    }
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

    // The sphere itself (and its grid lines) stay fixed in the scene: a
    // gate rotates only the qubit's *state*, not the reference frame, so
    // the Bloch axes and sphere surface must not spin. Only the position
    // marker below lives in the rotating `qubit` group; the sphere and
    // grid lines are parented to a separate, non-rotating group instead.
    const sphereFrame = new Group();

    // Add the main sphere.
    const sphereGeometry = new SphereGeometry(5, 96, 64);
    const material = new MeshLambertMaterial({
      emissive: palette.sphereColor,
      emissiveIntensity: palette.sphereBrightness,
      transparent: true,
      opacity: palette.sphereOpacity,
    });
    this.sphereMaterial = material;
    const sphere = new Mesh(sphereGeometry, material);
    // Draw the (transparent, emissive) sphere before the grid lines. Both
    // are transparent, so without an explicit order three.js sorts them by
    // centroid distance and the sphere sometimes lands *after* the near-side
    // line circles -- painting the emissive surface over them and washing a
    // whole hemisphere of lines to white. The washed half flips as the
    // camera rotates. Pinning the sphere to renderOrder 0 (and the lines to
    // 1, below) makes the order deterministic.
    sphere.renderOrder = 0;
    sphereFrame.add(sphere);

    // Add the 'spin' direction marker. This is the only part of the qubit
    // group that should move when a gate is applied -- it tracks the
    // current state vector across the (fixed) sphere surface.
    const coneGeometry = new ConeGeometry(0.2, 0.75, 32);
    const coneMat = new MeshBasicMaterial({ color: palette.markerColor });
    this.markerMaterial = coneMat;
    const marker = new Mesh(coneGeometry, coneMat);
    marker.position.set(0, 5.125, 0.4);
    marker.rotateX(Math.PI / 2);
    qubit.add(marker);

    // Draw smooth latitude/longitude grid lines on the sphere. Each circle
    // is a single high-segment line loop, which reads as a clean great-circle.
    const gridRadius = 5.1;
    const circleSegments = 128;
    const lineMaterial = new LineBasicMaterial({
      // Test against the sphere's depth so far-side circles stay occluded,
      // but don't write depth: the lines render after the sphere (renderOrder
      // below) and shouldn't depth-fight one another.
      depthTest: true,
      depthWrite: false,
      transparent: true,
      opacity: palette.sphereLinesOpacity,
    });
    this.sphereLineMaterial = lineMaterial;
    const sphereLines = new Group();

    // Build a closed circle of `circleSegments` points from a function that
    // maps an angle in [0, 2*PI) to a point on the sphere.
    const addCircle = (pointAt: (angle: number) => Vector3) => {
      const points: Vector3[] = [];
      for (let i = 0; i <= circleSegments; i++) {
        points.push(pointAt((i / circleSegments) * Math.PI * 2));
      }
      const geometry = new BufferGeometry().setFromPoints(points);
      const line = new Line(geometry, lineMaterial);
      // Render after the sphere (renderOrder 0) so the sphere never paints
      // over the lines. depthTest still occludes the far-side circles.
      line.renderOrder = 1;
      sphereLines.add(line);
    };

    // Latitude circles: evenly spaced rings of constant polar angle. We skip
    // the poles (the degenerate zero-radius rings) and draw the rest.
    const latitudeCount = 18;
    for (let i = 1; i < latitudeCount; i++) {
      const theta = (i / latitudeCount) * Math.PI;
      const y = gridRadius * Math.cos(theta);
      const r = gridRadius * Math.sin(theta);
      addCircle(
        (angle) => new Vector3(r * Math.cos(angle), y, r * Math.sin(angle)),
      );
    }

    // Longitude circles: great circles through both poles, evenly spaced in
    // azimuth. Each half-meridian repeats on the far side, so stepping over
    // half the circle covers the whole sphere.
    const longitudeCount = 18;
    for (let i = 0; i < longitudeCount; i++) {
      const phi = (i / longitudeCount) * Math.PI;
      const cosPhi = Math.cos(phi);
      const sinPhi = Math.sin(phi);
      addCircle(
        (angle) =>
          new Vector3(
            gridRadius * Math.sin(angle) * cosPhi,
            gridRadius * Math.cos(angle),
            gridRadius * Math.sin(angle) * sinPhi,
          ),
      );
    }

    sphereFrame.add(sphereLines);
    scene.add(sphereFrame);
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
        // Shared geometry + a placeholder material from the palette; the
        // fade pass below will assign the correct material this frame.
        const trackBall = new Mesh(
          this.trailDotGeometry,
          this.trailDotMaterials[0],
        );
        trackBall.position.set(0, 5, 0);

        // Convert to world space
        trackBall.position.applyQuaternion(val.pos);

        // Save along with the interpolation point
        this.trail.add(trackBall);
        val.ref = trackBall;
      });

      // Set qubit position to slerped values
      this.qubit.quaternion.copy(currentRotation.pos);

      // Fade out the path trail as needed. Use shared palette materials
      // instead of allocating a fresh MeshBasicMaterial per dot per frame.
      this.trail.children.forEach((child, idx, arr) => {
        const ball = child as Mesh;
        const sat = easeOutSine((idx + 1) / arr.length);
        ball.material = this.getTrailDotMaterial(sat);
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
   * waited for each animation to finish. Used by the "inspect trace"
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
    // from its interpolation path -- otherwise navigating the trace would
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
      // Same trackball construction as the animation loop in queueGate,
      // but we don't know the final dot count until we've walked every
      // step, so we defer color/scale to a single fade pass after the
      // loop. Shared geometry + a placeholder palette material keep this
      // allocation-free apart from the lightweight Mesh wrapper.
      // We deliberately do not set val.ref here: these are throwaway
      // visuals owned by the snap (cleared on the next snapTo), not the
      // long-lived references the animation path uses to skip
      // already-drawn points.
      for (const val of applied.path) {
        const trackBall = new Mesh(
          this.trailDotGeometry,
          this.trailDotMaterials[0],
        );
        trackBall.position.set(0, 5, 0);
        trackBall.position.applyQuaternion(val.pos);
        this.trail.add(trackBall);
      }
    }
    // Apply the same age-based fade the animation loop applies on every
    // frame so the trail looks the same whether it was drawn step by
    // step or rebuilt in one shot. Palette lookup, no allocations.
    this.trail.children.forEach((child, idx, arr) => {
      const ball = child as Mesh;
      const sat = easeOutSine((idx + 1) / arr.length);
      ball.material = this.getTrailDotMaterial(sat);
      ball.scale.setScalar(sat + 0.5);
    });
    this.qubit.quaternion.copy(this.rotations.currPosition);
    this.render();
  }

  /**
   * Map an age-fade saturation in [0, 1] to a pre-built material from
   * `trailDotMaterials`. Replaces `new MeshBasicMaterial({ color })` on
   * the per-dot hot path -- the visual difference of bucketing 64 unique
   * sat values into 32 palette entries is imperceptible, the perf
   * difference (no allocation, no GC) is not.
   */
  private getTrailDotMaterial(sat: number): MeshBasicMaterial {
    const n = this.trailDotMaterials.length;
    const idx = Math.min(n - 1, Math.max(0, Math.floor(sat * n)));
    return this.trailDotMaterials[idx];
  }

  render() {
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
  }

  // Resize the WebGL drawing buffer to match the on-screen size of the
  // canvas's container. Passing `false` as the third arg to `setSize`
  // tells three.js to update the backing buffer only and leave the
  // canvas's CSS size alone, so the element keeps stretching to fill its
  // flex cell while the render resolution tracks the actual pixels. The
  // perspective camera's aspect ratio is updated to match so the sphere
  // stays round at any container shape.
  resize(width: number, height: number) {
    const w = Math.max(1, Math.floor(width));
    const h = Math.max(1, Math.floor(height));
    this.renderer.setPixelRatio(window.devicePixelRatio || 1);
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.render();
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

    this.sphereLineMaterial.opacity = palette.sphereLinesOpacity;
    this.sphereLineMaterial.needsUpdate = true;

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
    // Trail dots all share the same geometry + a small material palette, so
    // we skip them here and dispose those shared resources exactly once below.
    const sharedGeo = this.trailDotGeometry;
    const sharedMats = new Set<MeshBasicMaterial>(this.trailDotMaterials);
    this.scene.traverse((obj) => {
      const mesh = obj as Mesh;
      if (mesh.geometry && mesh.geometry !== sharedGeo) {
        mesh.geometry.dispose();
      }
      const mat = mesh.material as
        | { map?: { dispose: () => void }; dispose?: () => void }
        | { map?: { dispose: () => void }; dispose?: () => void }[]
        | undefined;
      if (Array.isArray(mat)) {
        mat.forEach((m) => {
          if (sharedMats.has(m as MeshBasicMaterial)) return;
          m.map?.dispose();
          m.dispose?.();
        });
      } else if (mat && !sharedMats.has(mat as MeshBasicMaterial)) {
        mat.map?.dispose();
        mat.dispose?.();
      }
    });
    // Dispose the shared trail-dot resources exactly once.
    sharedGeo.dispose();
    this.trailDotMaterials.forEach((m) => m.dispose());
    this.trailDotMaterials = [];
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
  /** Optional host-supplied control rendered just after the Run button in
   * the gate-editor row. The playground uses this to place its
   * "share link" button alongside Run rather than floating it over the
   * widget. */
  actionSlot?: ComponentChildren;
}

export function BlochSphere(props: BlochSphereProps = {}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // Wrapper around the canvas whose size the WebGL buffer tracks. The
  // canvas itself stretches to fill this element via CSS; we observe the
  // wrapper (not the canvas) so the ResizeObserver reports the intended
  // layout size rather than the size three.js just wrote to the canvas.
  const stageRef = useRef<HTMLDivElement>(null);
  const renderer = useRef<BlochRenderer | null>(null);
  // Scrollable container holding the trace rows. We keep a ref so we
  // can pull the currently-active row into view whenever the cursor
  // moves (e.g. during playback). Doing it manually instead of via
  // `Element.scrollIntoView` so we only ever move the trace pane and
  // never accidentally scroll the page.
  const traceScrollRef = useRef<HTMLDivElement>(null);

  // The widget's interaction model is a time-travel trace:
  //
  //   * `gates` is the canonical list of single-character gate codes that
  //     have been applied to |0\u27e9, in order. It is the only durable state;
  //     everything else (sphere position, displayed state vectors,
  //     trace rows) is derived from it.
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
  // persistent banner, dimmed/italicized future rows, and disabled
  // Undo/Redo buttons. Applying a new gate while inspecting commits the
  // truncation (future rows become discarded). This mirrors how
  // browsers and most editors handle "navigate back, then act".
  const [gates, setGates] = useState<string[]>([]);
  const [cursor, setCursor] = useState(0);
  const [redoStack, setRedoStack] = useState<string[]>([]);
  const [rzAngle, setRzAngle] = useState(0);

  // Whether the gate controls (gate buttons, gate-string editor, Rz
  // slider) are collapsed. When collapsed, the whole control stack is
  // replaced by a compact read-only view of the current gate program
  // plus a button to expand the controls again -- handy for users who
  // just want to scrub the trace without the editing chrome taking up
  // vertical space.
  const [controlsCollapsed, setControlsCollapsed] = useState(false);

  // Playback state for the media-player-style controls. Stored both as
  // React state (drives button labels and disabled flags) and as a ref
  // (read from inside animation-completion callbacks, which capture
  // their value at call time so plain state would go stale).
  //
  // `animatingToIndexRef` tracks the trace index the in-flight
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
  // The redo stack is cleared, since editing invalidates undone trace
  // the same way applying a new gate does.
  const [draft, setDraft] = useState<string | null>(null);
  const displayValue = draft ?? gates.join("");

  // Measured natural width (px) of the widest piece of trace content,
  // used to size the trace pane so it grows horizontally to fit the
  // wide `gate . |psi> = result` equations instead of clipping them or
  // showing a horizontal scrollbar. Null until first measurement.
  const [traceContentWidth, setTraceContentWidth] = useState<number | null>(
    null,
  );
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
    // the regular `applyGate` path here because reading back state
    // inside a tight setState loop hits stale-closure bugs. Instead,
    // seed `gates` directly and position the cursor at the start so
    // the widget opens on |0⟩ in inspect mode -- the user can then
    // press Play (or step-forward) to watch the supplied program
    // execute, rather than being shown only the final state. The
    // trace pane and transport controls let them scrub freely.
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
        // Cursor at 0 puts the widget in inspect mode on the initial
        // |0⟩ state; the renderer is already there by default so no
        // snapTo is needed. `onGatesChanged` still fires so the parent
        // sees the full sequence -- only the visible step starts at
        // the beginning rather than the end.
        setCursor(0);
        props.onGatesChanged?.(cleaned);
      }
    }
    // Wire live theme switches (e.g. user toggles VS Code light/dark while
    // the widget is open) through to the WebGL scene. CSS-styled parts of
    // the widget pick up the change automatically via theme tokens.
    const themeCleanup = detectThemeChange(document.body, (isDark) => {
      r.setTheme(isDark);
    });
    // Keep the WebGL drawing buffer in sync with the on-screen size of the
    // stage. Observing the wrapper element lets the widget fill whatever
    // host it sits in (a full VS Code editor tab, a playground pane, or an
    // inline Jupyter output) and stay sharp on high-DPI displays.
    let resizeObserver: ResizeObserver | undefined;
    const stage = stageRef.current;
    if (stage) {
      r.resize(stage.clientWidth, stage.clientHeight);
      resizeObserver = new ResizeObserver((entries) => {
        const rect = entries[0]?.contentRect;
        if (rect) r.resize(rect.width, rect.height);
      });
      resizeObserver.observe(stage);
    }
    return () => {
      resizeObserver?.disconnect();
      themeCleanup();
      r.dispose();
      renderer.current = null;
    };
  }, []);

  // Derived: per-step trace entries (LaTeX strings) for the whole
  // `gates` sequence, walking the matrix product forward from |0\u27e9.
  // Computed in one pass instead of being mirrored in state, so the
  // trace rows can never disagree with the underlying gate list.
  const traceEntries = useMemo(() => {
    let prior: Vec2 = Ket0;
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

  // Measure the natural width of the trace content and drive the pane
  // width from it. The visible content lives in an absolutely-positioned
  // inner layer (so a long trace can't grow the grid's rows -- see
  // `.qs-bloch-trace` in the CSS), which means it no longer contributes
  // its width to the `auto` grid column either; the column would collapse
  // to the pane's `min-width`. So we measure the widest rendered row here
  // and feed it back as an explicit column width (via the
  // `--qs-trace-width` custom property), letting the pane grow
  // horizontally in both hosts without a horizontal scrollbar.
  //
  // The equation rows live inside the scroll container, which clips its
  // overflow -- so we can't read their width from the inner layer's
  // `scrollWidth`. Instead we measure each row's own `scrollWidth` (the
  // rows are typeset `white-space: nowrap`, so each one's width is its
  // intrinsic content width, independent of the pane width) and add the
  // actual rendered scrollbar gutter so the vertical scrollbar never eats
  // into a row. Because row widths don't depend on the pane width, the
  // result is a stable fixed point with no layout feedback loop. We round
  // up and only update on a real change to avoid sub-pixel thrashing.
  const PANE_MIN_WIDTH = 300;
  useEffect(() => {
    const list = traceScrollRef.current;
    if (!list) return;
    const measure = () => {
      let widestRow = 0;
      for (const row of Array.from(list.children)) {
        widestRow = Math.max(widestRow, (row as HTMLElement).scrollWidth);
      }
      // Width consumed by the (possibly absent) vertical scrollbar, so a
      // long trace doesn't clip the right edge of the widest row.
      const scrollbar = list.offsetWidth - list.clientWidth;
      // +2 for the pane's 1px left/right border.
      const next = Math.max(
        PANE_MIN_WIDTH,
        Math.ceil(widestRow + scrollbar + 2),
      );
      setTraceContentWidth((prev) =>
        prev !== null && Math.abs(prev - next) <= 1 ? prev : next,
      );
    };
    measure();
    // Re-measure when fonts finish loading or the host font size changes,
    // since either can change the typeset width after first paint.
    const ro = new ResizeObserver(measure);
    ro.observe(list);
    for (const row of Array.from(list.children)) ro.observe(row);
    return () => ro.disconnect();
  }, [traceEntries]);

  // Keep the currently-active trace row in view as `cursor` advances
  // (most visibly during playback). We scroll the trace container
  // directly via `scrollTo` instead of `Element.scrollIntoView` --
  // `scrollIntoView` walks up the ancestor chain and will scroll the
  // page itself once the trace pane has bottomed out (e.g. when the
  // active row is near the end of a long sequence). Driving
  // `container.scrollTop` keeps the scrolling strictly local to the
  // trace pane.
  //
  // The bottom of the visible band is partially covered by the sticky
  // `.qs-bloch-trace-item-latest` row (the pinned final step), so we
  // subtract its height -- otherwise the active row could slip behind
  // the sticky row and look stuck. When we do scroll, we aim to center
  // the active row in the (visible band minus the sticky overlap) so
  // long sequences keep the active step in the middle of the pane.
  useEffect(() => {
    const container = traceScrollRef.current;
    if (!container) return;
    const active = container.querySelector<HTMLElement>(
      ".qs-bloch-trace-item-current",
    );
    if (!active) return;
    // The sticky latest row only overlaps when the active row isn't
    // *also* the latest one -- otherwise it's the same element.
    const sticky = container.querySelector<HTMLElement>(
      ".qs-bloch-trace-item-latest",
    );
    const stickyOverlap = sticky && sticky !== active ? sticky.offsetHeight : 0;
    const visibleHeight = container.clientHeight - stickyOverlap;
    const cTop = container.scrollTop;
    const cBottom = cTop + visibleHeight;
    const aTop = active.offsetTop;
    const aBottom = aTop + active.offsetHeight;
    if (aTop < cTop || aBottom > cBottom) {
      // Target scrollTop that centers the active row inside the
      // visible band. Clamp to the container's scrollable range so we
      // don't ask for a negative offset (active row near the very top)
      // or overshoot past the end (active row near the bottom of a
      // short list).
      const desired = aTop - (visibleHeight - active.offsetHeight) / 2;
      const maxScroll = container.scrollHeight - container.clientHeight;
      const target = Math.max(0, Math.min(maxScroll, desired));
      container.scrollTo({ top: target, behavior: "smooth" });
    }
  }, [cursor, gates]);

  // Current Bloch-sphere spherical coordinates (theta, phi) for the qubit
  // state after applying the first `cursor` gates. Derived by re-walking
  // the gate list through a throwaway `Rotations` instance so the overlay
  // can never drift out of sync with the renderer. We don't follow the
  // inter-step animation here on purpose: the overlay shows the discrete
  // post-step state, matching what the LaTeX trace pane shows.
  //
  // Three.js axes are not the Bloch axes the user sees on the diagram:
  //   axis label X is drawn at three.js (0, 0, 6.4)  -> Bloch x = three.js z
  //   axis label Y is drawn at three.js (6.4, 0, 0)  -> Bloch y = three.js x
  //   axis label Z is drawn at three.js (0, 6.4, 0)  -> Bloch z = three.js y
  // The state vector starts pointing along three.js +Y (i.e. Bloch +z),
  // which is the |0> north pole.
  const blochAngles = useMemo(() => {
    const rot = new Rotations();
    for (let i = 0; i < cursor; i++) {
      const info = gateInfo[gates[i]];
      switch (info.rotateAxis) {
        case "X":
          rot.rotateX(info.rotateAngle);
          break;
        case "Y":
          rot.rotateY(info.rotateAngle);
          break;
        case "Z":
          rot.rotateZ(info.rotateAngle);
          break;
        case "H":
          rot.rotateH(info.rotateAngle);
          break;
      }
    }
    const tip = new Vector3(0, 1, 0).applyQuaternion(rot.currPosition);
    const blochZ = Math.max(-1, Math.min(1, tip.y));
    const theta = Math.acos(blochZ);
    // phi is undefined at the poles; flag it so the overlay can hide it.
    const polar = Math.abs(blochZ) > 0.999999;
    const phi = polar ? 0 : Math.atan2(tip.x, tip.z);
    return { theta, phi, polar };
  }, [gates, cursor]);

  // Current state-vector amplitudes at the cursor, as a column-vector
  // ket. Walks the same matrix product as the trace list but stops at
  // the cursor, so the overlay always shows the state the sphere is
  // currently displaying. Rendered in the top-right corner of the stage.
  const currentStateLatex = useMemo(() => {
    let state: Vec2 = Ket0;
    for (let i = 0; i < cursor; i++) {
      state = gateInfo[gates[i]].matrix.mulVec2(state);
    }
    return `$$ | \\psi \\rangle = ${state.toLaTeX()} $$`;
  }, [gates, cursor]);

  const inInspectMode = cursor < gates.length;
  const canUndo = !inInspectMode && cursor > 0 && !isPlaying;
  const canRedo = !inInspectMode && redoStack.length > 0 && !isPlaying;
  // Playback affordance. These cover the media-control row; everything
  // is derived from `cursor` / `gates` / `isPlaying` so the buttons can
  // never disagree with what the sphere is actually doing.
  const atStart = cursor === 0;
  const atEnd = cursor >= gates.length;
  const canStepBack = !atStart;
  const canStepForward = !atEnd;
  const canPlay = gates.length > 0;

  /**
   * Cancel any in-flight playback animation and land cleanly on a
   * trace step. Called by Pause directly, and called as a guard by
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
   * without modifying it. Used by clicks on trace rows and the
   * "Jump to latest" button. Snaps the renderer instantly (no animation
   * noise) because the user is inspecting, not acting.
   */
  function navigateTo(pos: number) {
    if (!renderer.current) return;
    if (pos < 0 || pos > gates.length) return;
    // Any deliberate seek (trace-row click, jump button) implicitly
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
    // Also return the Rz slider to its zero position so the control
    // reflects the cleared state rather than a stale angle.
    setRzAngle(0);
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

  // The Rz angle is chosen with a circular dial (see the JSX below). The
  // dial stores the angle snapped to the lookup-table resolution so the
  // previewed decomposition matches exactly what gets committed. The
  // table is indexed by angle*200, so one step is 1/200 rad and the full
  // turn is rzOps.length steps (== 2*PI*200).
  const dialRef = useRef<SVGSVGElement>(null);
  // Holds the pending requestAnimationFrame id while dragging the dial,
  // so pointermove can coalesce rapid moves into one update per frame.
  const dialFrameRef = useRef<number | null>(null);
  const RZ_STEP = 1 / 200;
  const RZ_STEPS = rzOps.length;

  // Snap an arbitrary angle (radians) onto the lookup-table grid and wrap
  // it into [0, 2*PI). Keeping every angle on the grid means the dial,
  // the readout, and the committed decomposition can never disagree.
  function snapAngle(a: number): number {
    let idx = Math.round(a * 200) % RZ_STEPS;
    if (idx < 0) idx += RZ_STEPS;
    return idx * RZ_STEP;
  }

  // Convert a pointer position to an angle measured from the dial center.
  // 0 rad points right (3 o'clock) and increases counterclockwise, the
  // standard math convention; SVG's y-axis points down, so we negate the
  // vertical delta to flip it back.
  function angleFromPointer(clientX: number, clientY: number): number {
    const svg = dialRef.current;
    if (!svg) return rzAngle;
    const rect = svg.getBoundingClientRect();
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    let a = Math.atan2(-(clientY - cy), clientX - cx);
    if (a < 0) a += Math.PI * 2;
    return snapAngle(a);
  }

  function dialPointerDown(e: PointerEvent) {
    if (isPlaying) return;
    const svg = e.currentTarget as SVGSVGElement;
    svg.setPointerCapture(e.pointerId);
    setRzAngle(angleFromPointer(e.clientX, e.clientY));
  }

  function dialPointerMove(e: PointerEvent) {
    const svg = e.currentTarget as SVGSVGElement;
    if (!svg.hasPointerCapture(e.pointerId)) return;
    // Coalesce moves to one state update per animation frame. Pointer
    // events can fire far more often than the display refreshes, and each
    // setRzAngle triggers a re-render, so without this a fast drag queues
    // up many redundant renders and feels sluggish.
    const next = angleFromPointer(e.clientX, e.clientY);
    if (dialFrameRef.current !== null) return;
    dialFrameRef.current = requestAnimationFrame(() => {
      dialFrameRef.current = null;
      setRzAngle(next);
    });
  }

  function dialPointerUp(e: PointerEvent) {
    const svg = e.currentTarget as SVGSVGElement;
    if (svg.hasPointerCapture(e.pointerId))
      svg.releasePointerCapture(e.pointerId);
    // Flush any frame queued by the last move so the final position isn't
    // dropped, and clear the pending-frame guard for the next drag.
    if (dialFrameRef.current !== null) {
      cancelAnimationFrame(dialFrameRef.current);
      dialFrameRef.current = null;
    }
    setRzAngle(angleFromPointer(e.clientX, e.clientY));
  }

  // Keyboard support for the dial (focusable, role="slider"). Arrow keys
  // nudge by one grid step; Home/End jump to 0 / just under a full turn.
  // The angle wraps, matching the circular control.
  function dialKeyDown(e: KeyboardEvent) {
    if (isPlaying) return;
    let next: number;
    switch (e.key) {
      case "ArrowRight":
      case "ArrowUp":
        next = rzAngle + RZ_STEP;
        break;
      case "ArrowLeft":
      case "ArrowDown":
        next = rzAngle - RZ_STEP;
        break;
      case "PageUp":
        next = rzAngle + RZ_STEP * 10;
        break;
      case "PageDown":
        next = rzAngle - RZ_STEP * 10;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = (RZ_STEPS - 1) * RZ_STEP;
        break;
      default:
        return;
    }
    e.preventDefault();
    setRzAngle(snapAngle(next));
  }

  // Map the current Rz angle to its precomputed Clifford+T decomposition.
  // The table is indexed by angle*200 (mod 1256 == 2*PI*200), matching the
  // resolution the slider steps at. Empty string for angle 0 (identity).
  const rzAngleIdx = Math.round(rzAngle * 200) % rzOps.length;
  const rzDecomposition = rzOps[rzAngleIdx] ?? "";

  // Append the current Rz decomposition to the gate sequence, mirroring
  // the way the single-gate buttons commit a gate: truncate any future
  // (inspected-past) steps, add the new gates, move the cursor to the end,
  // and clear the redo stack. The renderer snaps straight to the final
  // state rather than animating through all ~60 decomposition gates; the
  // user can scrub the trace to watch the decomposition step by step.
  function applyRzDecomposition() {
    if (!renderer.current || rzDecomposition.length === 0) return;
    stopPlayback(false);
    let base = gates;
    if (cursor < gates.length) {
      base = gates.slice(0, cursor);
    }
    const next = [...base, ...rzDecomposition.split("")];
    renderer.current.snapTo(gatesToSteps(next));
    setGates(next);
    setCursor(next.length);
    setRedoStack([]);
    // The committed sequence is now the source of truth; drop any draft.
    // Leave the dial at its current angle so the user can add the same
    // rotation again without re-dialing it.
    setDraft(null);
    props.onGatesChanged?.(next.join(""));
  }

  // Memoized trace row list. Rebuilding these vnodes on every render is
  // what made the dial feel slower as the sequence grew: each dial move
  // calls setRzAngle, re-rendering the whole component, and preact then
  // has to re-create and diff one vnode (plus a Markdown child) per gate.
  // Keying the list on the values it actually depends on -- the rendered
  // entries and the cursor position -- lets preact reuse the exact same
  // vnodes when only the Rz angle changed, so the trace cost drops out of
  // the drag entirely. `navigateTo` reads `gates` via closure, which only
  // changes when `traceEntries` does, so the captured closure stays
  // correct between rebuilds.
  const traceRows = useMemo(() => {
    return traceEntries.map((str, i) => {
      const stepIndex = i + 1;
      const classes = ["qs-bloch-trace-item"];
      if (stepIndex === cursor) classes.push("qs-bloch-trace-item-current");
      if (stepIndex > cursor) classes.push("qs-bloch-trace-item-future");
      // Pin the bottom-most row so the latest step stays visible
      // when the rest of the trace scrolls. See the CSS rule
      // for `.qs-bloch-trace-item-latest` for the mechanics.
      if (i === traceEntries.length - 1)
        classes.push("qs-bloch-trace-item-latest");
      return (
        <div
          class={classes.join(" ")}
          title={`Go to step ${stepIndex}`}
          onClick={() => navigateTo(stepIndex)}
        >
          <Markdown markdown={str}></Markdown>
        </div>
      );
    });
  }, [traceEntries, cursor]);

  return (
    <div
      class={"qs-bloch" + (controlsCollapsed ? " qs-bloch-collapsed" : "")}
      style={
        // Drive the trace column's width from the measured content
        // width (see the `traceContentWidth` effect). Exposed as a CSS
        // custom property the grid template consumes, so the single-column
        // media query can simply ignore it. Unset until first measurement,
        // when the column falls back to its default floor.
        traceContentWidth !== null
          ? ({
              "--qs-trace-width": `${traceContentWidth}px`,
            } as Record<string, string>)
          : undefined
      }
    >
      <div class="qs-bloch-stage" ref={stageRef}>
        <canvas ref={canvasRef}></canvas>
        <div class="qs-bloch-coords" aria-hidden="true">
          <span>
            <span class="qs-bloch-coords-greek">θ</span>
            {" = "}
            {blochAngles.theta.toFixed(2)} rad
          </span>
          <span>
            <span class="qs-bloch-coords-greek">φ</span>
            {" = "}
            {blochAngles.polar ? "n/a" : `${blochAngles.phi.toFixed(2)} rad`}
          </span>
        </div>
        <div class="qs-bloch-state" aria-hidden="true">
          <Markdown markdown={currentStateLatex}></Markdown>
        </div>
        {controlsCollapsed && (
          <div class="qs-bloch-gate-overlay" aria-hidden="true">
            {gates.length > 0 ? gates.join("") : "\u2014"}
          </div>
        )}
        <button
          type="button"
          class="qs-bloch-controls-toggle"
          onClick={() => setControlsCollapsed((c) => !c)}
          title={
            controlsCollapsed ? "Show gate controls" : "Hide gate controls"
          }
          aria-label={
            controlsCollapsed ? "Show gate controls" : "Hide gate controls"
          }
          aria-expanded={!controlsCollapsed}
        >
          {controlsCollapsed ? "\u2699" : "\u2715"}
        </button>
      </div>
      <div class="qs-bloch-trace" style="font-size: 0.9em;">
        <div class="qs-bloch-trace-inner">
          <div class="qs-bloch-trace-title">
            <span>Trace</span>
            {gates.length > 0 && (
              <span
                class="qs-bloch-trace-step-counter"
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
          without color or icons.
        */}
          <div
            class="qs-bloch-media-controls"
            role="group"
            aria-label="Playback"
          >
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
          <div
            ref={traceScrollRef}
            style="overflow-y: auto; overflow-x: hidden; flex: 1; display: flex; flex-direction: column; align-items: stretch; min-height: 0;"
          >
            <div
              class={
                "qs-bloch-trace-item" +
                (cursor === 0 ? " qs-bloch-trace-item-current" : "") +
                (traceEntries.length === 0 ? " qs-bloch-trace-item-latest" : "")
              }
              title="Initial state |0⟩"
              onClick={() => navigateTo(0)}
            >
              <Markdown markdown={INITIAL_KET_MARKDOWN}></Markdown>
            </div>
            {traceRows}
          </div>
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
          title={isPlaying ? "Pause to reset" : "Clear the entire trace"}
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
            style="margin-left: 8px; margin-right: 8px; padding: 0 12px; height: 25px"
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
          {props.actionSlot}
        </div>
        {/*
          Gate-count breakdown for the sanitized draft. Shows one
          chip per gate type used (in canonical X Y Z H S S† T T†
          order), then a T-count callout. T-count is a meaningful
          quantum-cost metric for fault-tolerant implementations -- T
          and T† gates are the expensive primitives -- so surfacing it
          live gives users a quick sense of "how heavy" their program
          is, especially after the Rz slider expands a single rotation
          into a dozens-of-gates decomposition.
        */}
        <div
          class={
            "qs-bloch-gate-editor-feedback" +
            (hasUnsavedDraft ? " qs-bloch-gate-editor-unsaved" : "")
          }
          aria-hidden="true"
        >
          <span class="qs-bloch-gate-editor-breakdown">
            {(() => {
              const counts: Record<string, number> = {};
              for (const ch of sanitizedDraft) {
                counts[ch] = (counts[ch] ?? 0) + 1;
              }
              const chips = [];
              for (const code of VALID_GATE_CODES) {
                const n = counts[code] ?? 0;
                if (n === 0) continue;
                chips.push(
                  <span
                    key={code}
                    class="qs-bloch-gate-editor-chip"
                    title={`${n}× ${gateInfo[code].display}`}
                  >
                    <span class="qs-bloch-gate-editor-chip-name">
                      {gateInfo[code].display}
                    </span>
                    <span class="qs-bloch-gate-editor-chip-count">{n}</span>
                  </span>,
                );
              }
              const tCount = (counts["T"] ?? 0) + (counts["t"] ?? 0);
              if (chips.length === 0) {
                return <span class="qs-bloch-gate-editor-empty">no gates</span>;
              }
              return (
                <>
                  {chips}
                  {tCount > 0 && (
                    <span
                      class="qs-bloch-gate-editor-tcount"
                      title="T-count: number of T and T† gates. T gates are the expensive primitive in fault-tolerant quantum computing."
                    >
                      T-count: {tCount}
                    </span>
                  )}
                </>
              );
            })()}
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
      <div class="qs-bloch-rz">
        <div class="qs-bloch-rz-row">
          {(() => {
            // Knob sits on the track at the current angle. 0 rad is at
            // 3 o'clock, increasing counterclockwise; SVG y points down
            // so the vertical term is negated.
            const trackR = 46;
            const knobX = 60 + trackR * Math.cos(rzAngle);
            const knobY = 60 - trackR * Math.sin(rzAngle);
            return (
              <svg
                ref={dialRef}
                class={
                  "qs-bloch-rz-dial" +
                  (isPlaying ? " qs-bloch-rz-dial-disabled" : "")
                }
                viewBox="0 0 120 120"
                role="slider"
                tabIndex={isPlaying ? -1 : 0}
                aria-label="Rz angle in radians"
                aria-valuemin={0}
                aria-valuemax={(RZ_STEPS - 1) * RZ_STEP}
                aria-valuenow={rzAngle}
                aria-valuetext={`${rzAngle.toFixed(2)} radians`}
                onPointerDown={dialPointerDown}
                onPointerMove={dialPointerMove}
                onPointerUp={dialPointerUp}
                onKeyDown={dialKeyDown}
              >
                <circle
                  class="qs-bloch-rz-dial-track"
                  cx="60"
                  cy="60"
                  r={trackR}
                />
                {/* Tick marks at 0, π/2, π, 3π/2 for orientation. */}
                {[0, Math.PI / 2, Math.PI, (3 * Math.PI) / 2].map((a) => (
                  <line
                    key={a}
                    class="qs-bloch-rz-dial-tick"
                    x1={60 + (trackR - 5) * Math.cos(a)}
                    y1={60 - (trackR - 5) * Math.sin(a)}
                    x2={60 + (trackR + 5) * Math.cos(a)}
                    y2={60 - (trackR + 5) * Math.sin(a)}
                  />
                ))}
                <line
                  class="qs-bloch-rz-dial-needle"
                  x1="60"
                  y1="60"
                  x2={knobX}
                  y2={knobY}
                />
                <circle class="qs-bloch-rz-dial-center" cx="60" cy="60" r="3" />
                <circle
                  class="qs-bloch-rz-dial-knob"
                  cx={knobX}
                  cy={knobY}
                  r="8"
                />
              </svg>
            );
          })()}
          <div class="qs-bloch-rz-info">
            <span class="qs-bloch-rz-readout">
              Rz({rzAngle.toFixed(2)} rad)
            </span>
            <button
              type="button"
              class="qs-bloch-rz-apply"
              onClick={applyRzDecomposition}
              disabled={isPlaying || rzDecomposition.length === 0}
              title={
                rzDecomposition.length === 0
                  ? "Set a non-zero angle to add an Rz rotation"
                  : "Append this Rz decomposition to the gate sequence"
              }
            >
              Add to sequence
            </button>
            <div class="qs-bloch-rz-decomposition" aria-live="polite">
              <span class="qs-bloch-rz-decomposition-label">
                Decomposition:
              </span>
              <span class="qs-bloch-rz-decomposition-gates">
                {rzDecomposition.length > 0
                  ? rzDecomposition
                  : "identity (no gates)"}
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
