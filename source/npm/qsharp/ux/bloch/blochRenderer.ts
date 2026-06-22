// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// The three.js rendering layer for the Bloch sphere widget. Owns the
// WebGL scene, the animated/snap rotation logic, and the trail/axis
// visuals. Kept separate from `bloch.tsx` (the preact component) so the
// rendering code can be reasoned about without the UI state machine, and
// vice versa.

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

import { AppliedGate, Rotations } from "../cplx.js";
import { RotationAxis } from "./blochGates.js";

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
export const DEFAULT_ROTATION_TIME_MS = 333;

export class BlochRenderer {
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
