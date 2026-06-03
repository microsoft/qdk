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

import { useEffect, useRef, useState } from "preact/hooks";

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
import { sanitizeGateSequence, VALID_GATE_CODES } from "./blochGates.js";

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

const gateLaTeX = {
  X: "\\begin{bmatrix} 0 & 1 \\\\ 1 & 0 \\end{bmatrix}",
  Y: "\\begin{bmatrix} 0 & -i \\\\ i & 0 \\end{bmatrix}",
  Z: "\\begin{bmatrix} 1 & 0 \\\\ 0 & -1 \\end{bmatrix}",
  S: "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{i {\\pi \\over 2}} \\end{bmatrix}",
  SA: "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{-i {\\pi \\over 2}} \\end{bmatrix}",
  T: "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{i {\\pi \\over 4}} \\end{bmatrix}",
  TA: "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{-i {\\pi \\over 4}} \\end{bmatrix}",
  H: "{1 \\over \\sqrt{2}} \\begin{bmatrix} 1 & 1 \\\\ 1 & -1 \\end{bmatrix}",
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

const rotationTimeMs = 100;

class BlochRenderer {
  scene: Scene;
  camera: PerspectiveCamera;
  renderer: WebGLRenderer;
  controls: OrbitControls;
  qubit: Group;
  trail: Group;
  rotationAxis: Group;
  animationCallbackId = 0;
  gateQueue: AppliedGate[] = [];
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

  queueGate(gate: AppliedGate) {
    this.gateQueue.push(gate);
    if (this.animationCallbackId) return; // Queue is already running

    // Close over these values for the running queue
    let currentGate: AppliedGate | undefined;
    let startTime = 0;

    const processQueue = () => {
      if (!currentGate) {
        currentGate = this.gateQueue.shift();
        if (!currentGate) {
          // Queue was empty. Done
          this.animationCallbackId = 0;
          return;
        } else {
          const axisInLocal = this.qubit.worldToLocal(currentGate.axis);
          this.rotationAxis.lookAt(axisInLocal);
          this.qubit.add(this.rotationAxis);
          startTime = performance.now();
        }
      }

      // Calculate the percent of rotation time elapsed from start to now
      const x = (performance.now() - startTime) / rotationTimeMs;

      // Ease the rotation
      const t = x < 1 ? easeInOutSine(x) : 1;

      // Rotate the qubit to the correct position
      const currentRotation = this.rotations.getRotationAtPercent(
        currentGate,
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

      // If that gate is done, unset it
      if (t >= 1) {
        currentGate = undefined;
        this.qubit.remove(this.rotationAxis);
        this.render();
      }

      this.animationCallbackId = requestAnimationFrame(processQueue);
    };

    // Kick off processing
    processQueue();
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

  const [gateArray, setGateArray] = useState<string[]>([]);
  const [state, setState] = useState(Ket0);
  const [rzAngle, setRzAngle] = useState(0);
  let newState = state;
  // Tracks the raw single-character gate codes actually applied to the
  // sphere, in order. `gateArray` above stores LaTeX strings for display
  // and isn't suitable for round-tripping through a URL.
  const appliedGatesRef = useRef<string>("");

  useEffect(() => {
    if (!canvasRef.current) return;
    // Resolve the initial theme exactly once; subsequent changes are
    // delivered via detectThemeChange below.
    const initialIsDark = ensureTheme() ?? false;
    const r = new BlochRenderer(canvasRef.current, initialIsDark);
    renderer.current = r;
    // Replay any gates supplied via the URL on initial mount. We just call
    // the regular `rotate` so the renderer animations, state vector, and
    // history pane all stay in sync with the rest of the UI. Inputs are
    // sanitized so a stale or hostile URL can't flood the animation queue
    // or trigger `console.error` per character.
    if (props.initialGates) {
      const { gates, modified } = sanitizeGateSequence(props.initialGates);
      if (modified) {
        console.warn(
          `BlochSphere: ignored unknown gates or excess length in initialGates ` +
            `(input length ${props.initialGates.length}, applied ${gates.length}). ` +
            `Valid gate codes are: ${VALID_GATE_CODES}.`,
        );
      }
      for (const ch of gates) {
        rotate(ch);
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

  const getLaTeX = (
    gateName: string,
    gateMatrix: string,
    oldState: string,
    newState: string,
  ) => `$$ ${gateName} | \\psi \\rangle_{${gateArray.length}} =
  ${gateMatrix}
  \\cdot ${oldState}
  = ${newState}
  $$`;

  function rotate(gate: string): void {
    const priorState = vec2(newState);
    if (renderer.current) {
      switch (gate) {
        case "X":
          renderer.current.rotateX(Math.PI);
          newState = PauliX.mulVec2(newState);
          gateArray.push(
            getLaTeX(
              "X",
              gateLaTeX.X,
              priorState.toLaTeX(),
              newState.toLaTeX(),
            ),
          );
          break;
        case "Y":
          renderer.current.rotateY(Math.PI);
          newState = PauliY.mulVec2(newState);
          gateArray.push(
            getLaTeX(
              "Y",
              gateLaTeX.Y,
              priorState.toLaTeX(),
              newState.toLaTeX(),
            ),
          );
          break;
        case "Z":
          renderer.current.rotateZ(Math.PI);
          newState = PauliZ.mulVec2(newState);
          gateArray.push(
            getLaTeX(
              "Z",
              gateLaTeX.Z,
              priorState.toLaTeX(),
              newState.toLaTeX(),
            ),
          );
          break;
        case "S":
          renderer.current.rotateZ(Math.PI / 2);
          newState = SGate.mulVec2(newState);
          gateArray.push(
            getLaTeX(
              "S",
              gateLaTeX.S,
              priorState.toLaTeX(),
              newState.toLaTeX(),
            ),
          );
          break;
        case "s":
          renderer.current.rotateZ(-Math.PI / 2);
          newState = SGate.adjoint().mulVec2(newState);
          gateArray.push(
            getLaTeX(
              "S†",
              gateLaTeX.SA,
              priorState.toLaTeX(),
              newState.toLaTeX(),
            ),
          );
          break;
        case "T":
          renderer.current.rotateZ(Math.PI / 4);
          newState = TGate.mulVec2(newState);
          gateArray.push(
            getLaTeX(
              "T",
              gateLaTeX.T,
              priorState.toLaTeX(),
              newState.toLaTeX(),
            ),
          );
          break;
        case "t":
          renderer.current.rotateZ(-Math.PI / 4);
          newState = TGate.adjoint().mulVec2(newState);
          gateArray.push(
            getLaTeX(
              "T†",
              gateLaTeX.TA,
              priorState.toLaTeX(),
              newState.toLaTeX(),
            ),
          );
          break;
        case "H":
          renderer.current.rotateH(Math.PI);
          newState = Hadamard.mulVec2(newState);
          gateArray.push(
            getLaTeX(
              "H",
              gateLaTeX.H,
              priorState.toLaTeX(),
              newState.toLaTeX(),
            ),
          );
          break;
        default:
          console.error("Unknown gate: " + gate);
          return;
      }
    }
    setState(newState);
    setGateArray([...gateArray]);
    appliedGatesRef.current += gate;
    props.onGatesChanged?.(appliedGatesRef.current);
  }

  function reset() {
    setGateArray([]);
    setState(vec2(Ket0));
    if (renderer.current) {
      renderer.current.reset();
    }
    appliedGatesRef.current = "";
    props.onGatesChanged?.("");
  }

  function applyGates() {
    const input = document.getElementById("run_gates") as HTMLInputElement;
    // Same defensive filter as the URL-replay path: a user pasting
    // arbitrary text shouldn't trigger `console.error` per character or
    // queue an unbounded number of animations.
    const { gates } = sanitizeGateSequence(input.value);
    for (const gate of gates) {
      rotate(gate);
    }
  }

  function sliderChange(e: Event) {
    const slider = e.target as HTMLInputElement;
    const angleIdx = Math.round(parseFloat(slider.value) * 200) % 1256;
    const button = document.getElementById("rz_button") as HTMLSpanElement;
    button.textContent = `Rz(${slider.value})`;

    const input = document.getElementById("run_gates") as HTMLInputElement;
    input.value = rzOps[angleIdx];
    setRzAngle(parseFloat(slider.value));
  }

  return (
    <div style="position: relative;">
      <canvas ref={canvasRef} width="600" height="600"></canvas>
      <div
        class="qs-bloch-history"
        style="font-size: 0.8em; position: absolute; left: 600px; top: 50px; height: 700px; min-width: 200px; overflow-y: scroll; display: flex; flex-direction: column; align-items: flex-start;"
      >
        {gateArray.map((str) => (
          <div style="border-bottom: 1px dotted gray; text-align: left">
            <Markdown markdown={str}></Markdown>
          </div>
        ))}
      </div>
      <div class="qs-gate-buttons">
        <button type="button" onClick={() => rotate("X")}>
          X
        </button>
        <button type="button" onClick={() => rotate("Y")}>
          Y
        </button>
        <button type="button" onClick={() => rotate("Z")}>
          Z
        </button>
        <button type="button" onClick={() => rotate("H")}>
          H
        </button>
        <button type="button" onClick={() => rotate("S")}>
          S
        </button>
        <button type="button" onClick={() => rotate("s")}>
          S†
        </button>
        <button type="button" onClick={() => rotate("T")}>
          T
        </button>
        <button type="button" onClick={() => rotate("t")}>
          T†
        </button>

        <button style="margin: 0 8px;" type="button" onClick={reset}>
          Reset
        </button>
      </div>
      <div style="margin-top: 12px">
        <input
          id="run_gates"
          type="text"
          size={60}
          placeholder="Enter gates then tab away"
        />
        <button
          style="margin-left: 8px; padding: 0 4px"
          type="button"
          onClick={applyGates}
        >
          Run
        </button>
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
        <span
          style="margin: 0 12px; font-style: italic; font-size: 1.2em;"
          id="rz_button"
        >
          Rz(0)
        </span>
      </div>
    </div>
  );
}
