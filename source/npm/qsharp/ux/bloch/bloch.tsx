// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/* The math for converting the basis amplitudes (a, b) of a single-qubit
   state a|0> + b|1> to a Bloch-sphere point is:
     theta = 2 * acos(|a|)
     phi   = arg(b) - arg(a), normalized to [0, 2 * PI)
   where |a| is the complex modulus of a (a real number in [0, 1]), so
   acos maps that amplitude ratio to the polar angle theta.
*/

import "./bloch.css";

import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { ComponentChildren } from "preact";

import { Vector3 } from "three";

import { Rotations, Ket0, Vec2 } from "../cplx.js";
import { Markdown } from "../renderers.js";
import { detectThemeChange, ensureTheme } from "../themeObserver.js";
import {
  Gate,
  GateKind,
  RotationGateKind,
  resolveGate,
  parseGateSequence,
  formatGateSequence,
  FIXED_GATE_KINDS,
  MAX_GATE_SEQUENCE_LENGTH,
} from "./blochGates.js";
import { BlochRenderer, DEFAULT_ROTATION_TIME_MS } from "./blochRenderer.js";

import rzOps from "../../rz-array.json";

// Markdown for the initial |0> state shown as the first trace row.
const INITIAL_KET_MARKDOWN =
  "$$ | \\psi \\rangle_0 = \\begin{bmatrix} 1 \\\\ 0 \\end{bmatrix} $$";

// Playback transport icons are drawn as inline SVG rather than as Unicode
// media glyphs (U+23E9 etc.). Those code points have inconsistent font
// coverage across platforms, and the U+FE0E text-presentation selector we
// previously appended to force non-emoji rendering is not honored on
// macOS, so the controls showed up as tofu or color emoji there. Vector
// paths render identically everywhere. Paths use a 24x24 viewBox and are
// from the Material Design icon set.
type TransportIconName =
  | "jump-to-start"
  | "step-back"
  | "play"
  | "pause"
  | "replay"
  | "step-forward"
  | "jump-to-end";

const TRANSPORT_ICON_PATHS: Record<TransportIconName, string> = {
  "jump-to-start": "M6 6h2v12H6zm3.5 6l8.5 6V6z",
  "step-back": "M11 18V6l-8.5 6 8.5 6zm.5-6l8.5 6V6l-8.5 6z",
  play: "M8 5v14l11-7z",
  pause: "M6 5h4v14H6zm8 0h4v14h-4z",
  replay:
    "M12 5V1L7 6l5 5V7c3.31 0 6 2.69 6 6s-2.69 6-6 6-6-2.69-6-6H4c0 4.42 3.58 8 8 8s8-3.58 8-8-3.58-8-8-8z",
  "step-forward": "M4 18l8.5-6L4 6v12zm9-12v12l8.5-6L13 6z",
  "jump-to-end": "M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z",
};

function TransportIcon({ name }: { name: TransportIconName }) {
  return (
    <svg
      class="qs-bloch-transport-icon"
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
      focusable="false"
    >
      <path d={TRANSPORT_ICON_PATHS[name]} />
    </svg>
  );
}

/** Structural equality for a gate token (kind plus rotation angle). */
function gateEqual(a: Gate, b: Gate): boolean {
  return a.kind === b.kind && (a.angle ?? 0) === (b.angle ?? 0);
}

/** Structural equality for gate sequences. */
function gatesEqual(a: Gate[], b: Gate[]): boolean {
  return a.length === b.length && a.every((g, i) => gateEqual(g, b[i]));
}

// Maps the dial's axis selection to its rotation-gate kind.
const AXIS_TO_ROTATION: Record<"X" | "Y" | "Z", RotationGateKind> = {
  X: "Rx",
  Y: "Ry",
  Z: "Rz",
};

// Decodes a legacy single-character Clifford+T decomposition string (as
// stored in rz-array.json) into structured gate tokens.
const LEGACY_CODE_TO_KIND: Record<string, GateKind> = {
  X: "X",
  Y: "Y",
  Z: "Z",
  H: "H",
  S: "S",
  s: "S'",
  T: "T",
  t: "T'",
};
function decompStringToGates(s: string): Gate[] {
  const out: Gate[] = [];
  for (const ch of s) {
    const kind = LEGACY_CODE_TO_KIND[ch];
    if (kind) out.push({ kind });
  }
  return out;
}

export interface BlochSphereProps {
  /** Gate codes (X Y Z H S s T t) to replay on mount. Sanitized and
   * length-capped, so it's safe to pass straight from an untrusted URL. */
  initialGates?: string;
  /** Called with the full gate sequence whenever it changes, so parents
   * can keep a URL or other external state in sync. */
  onGatesChanged?: (gates: string) => void;
  /** Host-supplied control rendered after the gate-program input in the
   * editor row (the playground uses it for its "share link" button). */
  actionSlot?: ComponentChildren;
}

export function BlochSphere(props: BlochSphereProps = {}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // We observe this wrapper (not the canvas) for size changes, since the
  // canvas is stretched to fill it by CSS and three.js overwrites its size.
  const stageRef = useRef<HTMLDivElement>(null);
  const renderer = useRef<BlochRenderer | null>(null);
  // Scrollable trace container; kept as a ref so we can scroll the active
  // row into view without ever scrolling the page.
  const traceScrollRef = useRef<HTMLDivElement>(null);

  // The interaction model is a time-travel trace:
  //   * `gates` is the canonical, ordered list of applied gate codes. It is
  //     the only durable state; everything else is derived from it.
  //   * `cursor` is the viewing position in [0, gates.length]. Values
  //     between 0 and the end put the widget in *inspect mode*: the sphere
  //     shows that intermediate state without truncating the sequence.
  //   * `past` / `future` are undo/redo history as whole-sequence snapshots
  //     (one per user action), so undo reverts an entire action at once.
  //
  // Applying a new gate while inspecting commits the truncation (later rows
  // are discarded), mirroring "navigate back, then act" in browsers.
  const [gates, setGates] = useState<Gate[]>([]);
  const [cursor, setCursor] = useState(0);
  const [past, setPast] = useState<Gate[][]>([]);
  const [future, setFuture] = useState<Gate[][]>([]);
  const [rzAngle, setRzAngle] = useState(0);
  // Which axis the angle dial targets, so it can emit Rx/Ry/Rz.
  const [rotationAxis, setRotationAxis] = useState<"X" | "Y" | "Z">("Z");
  // While the user is typing in the Rz readout, this holds the in-progress
  // text; null means the field mirrors the live (snapped) angle instead.
  const [rzInputDraft, setRzInputDraft] = useState<string | null>(null);

  // The rotation control is a compact "Rz( ... rad)" field in the toolbar.
  // Focusing the angle field opens a popover holding the draggable dial and
  // the live Clifford+T decomposition; the axis dropdown opens its own menu.
  // Both close on an outside click (see the effect below).
  const [rotDialOpen, setRotDialOpen] = useState(false);
  const [axisMenuOpen, setAxisMenuOpen] = useState(false);
  const rotRef = useRef<HTMLDivElement>(null);

  // Whether the trace pane is collapsed, handing the reclaimed width to the
  // sphere visualization. The measured trace width (--qs-trace-width) is
  // retained while collapsed, so expanding restores the pane to its previous
  // size without re-measuring. Starts collapsed so the sphere gets the full
  // width when the widget first opens.
  const [traceCollapsed, setTraceCollapsed] = useState(true);

  // Playback state. Mirrored as a ref because animation-completion
  // callbacks capture state at call time and would otherwise read it stale.
  // `animatingToIndexRef` is the index the in-flight animation heads toward,
  // so Pause can snap there cleanly; null when nothing is animating.
  const [isPlaying, setIsPlaying] = useState(false);
  const isPlayingRef = useRef(false);
  const animatingToIndexRef = useRef<number | null>(null);

  // Playback speed multiplier (0.25x..4x). Pushed straight into
  // `renderer.current.rotationTimeMs` so dragging mid-Play takes effect
  // immediately (the rAF loop re-reads it every frame).
  const [speed, setSpeed] = useState(1);

  function speedChange(e: Event) {
    const slider = e.target as HTMLInputElement;
    const next = parseFloat(slider.value);
    setSpeed(next);
    if (renderer.current) {
      renderer.current.rotationTimeMs = DEFAULT_ROTATION_TIME_MS / next;
    }
  }

  // Live-text editing of the gate sequence. `draftText === null` means the
  // textbox mirrors the committed `gates`. While the user is typing,
  // `draftText` holds their input and is shown immediately; the expensive
  // trace/sphere update is deferred until they pause (debounced). Input is
  // sanitized per keystroke, so the textbox only ever holds valid codes.
  // Because the commit fires on a timer, two fail-safes guard the pending
  // timer: `cancelDraft` drops it when a non-text action supersedes the
  // edit, and the `draftTimerRef` unmount cleanup effect clears it so it
  // can't fire after the component is gone.
  const GATE_TEXT_DEBOUNCE_MS = 150;
  const [draftText, setDraftText] = useState<string | null>(null);
  const displayValue = draftText ?? formatGateSequence(gates);
  // Parse the visible text once per change for the count breakdown and the
  // gate-cap indicator (both work off token counts, not characters).
  const typedGates = useMemo(
    () => parseGateSequence(displayValue).gates,
    [displayValue],
  );
  // Pending-commit timer, the text awaiting commit, and a snapshot of
  // `gates` from the start of an editing burst (so the burst is one undo).
  const draftTimerRef = useRef<number | null>(null);
  const draftPendingRef = useRef<string | null>(null);
  const draftBaseRef = useRef<Gate[]>([]);
  // Whether the current editing burst has already pushed its history entry,
  // so multiple debounced commits in one burst collapse to a single undo.
  const draftPushedRef = useRef(false);

  // Measured natural width (px) of the widest trace row, fed back as an
  // explicit column width so the wide equations don't clip or scroll.
  const [traceContentWidth, setTraceContentWidth] = useState<number | null>(
    null,
  );

  // Convert gate codes to the {axis, angle} steps `snapTo` expects, keeping
  // the renderer ignorant of gate codes.
  function gatesToSteps(gs: Gate[]) {
    return gs.map((g) => {
      const info = resolveGate(g);
      return { axis: info.rotateAxis, angle: info.rotateAngle };
    });
  }

  useEffect(() => {
    if (!canvasRef.current) return;
    const initialIsDark = ensureTheme() ?? false;
    const r = new BlochRenderer(canvasRef.current, initialIsDark);
    renderer.current = r;
    // Replay any URL-supplied gates. We seed `gates` directly (rather than
    // the regular applyGate path, which hits stale-closure bugs in a tight
    // setState loop) and open on the latest step, so a linked-to program
    // shows its final state and the user can add gates without first
    // overwriting it. They can still Play/step back through the trace.
    if (props.initialGates) {
      const { gates: cleaned, modified } = parseGateSequence(
        props.initialGates,
      );
      if (modified) {
        console.warn(
          `BlochSphere: ignored unrecognized tokens or excess length in ` +
            `initialGates (input "${props.initialGates}", applied ` +
            `${cleaned.length} gates).`,
        );
      }
      if (cleaned.length) {
        setGates(cleaned);
        // Snap the sphere to the end of the sequence and park the cursor
        // there, matching the latest trace step.
        r.snapTo(gatesToSteps(cleaned));
        setCursor(cleaned.length);
        props.onGatesChanged?.(formatGateSequence(cleaned));
      }
    }
    // Live theme switches (e.g. VS Code light/dark toggled while open).
    const themeCleanup = detectThemeChange(document.body, (isDark) => {
      r.setTheme(isDark);
    });
    // Keep the WebGL buffer in sync with the stage's on-screen size, so the
    // widget fills whatever host it sits in and stays sharp on high-DPI.
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
  // `gates` sequence, walking the matrix product forward from |0>.
  // Computed in one pass instead of being mirrored in state, so the
  // trace rows can never disagree with the underlying gate list.
  const traceEntries = useMemo(() => {
    let prior: Vec2 = Ket0;
    return gates.map((gate, i) => {
      const info = resolveGate(gate);
      const next = info.matrix.mulVec2(prior);
      const latex = `$$ ${info.latexName} | \\psi \\rangle_{${i}} =
        ${info.matrixLatex}
        \\cdot ${prior.toLaTeX()}
        = ${next.toLaTeX()}
        $$`;
      prior = next;
      return latex;
    });
  }, [gates]);

  // Progressive trace rendering. Each row runs a synchronous KaTeX
  // conversion on mount, so committing a large batch at once would block
  // the main thread long enough to stall the animation. Cap how many rows
  // mount per render and fill the rest in during idle time.
  const PROGRESSIVE_CHUNK = 6;
  const prevTraceRef = useRef<string[]>([]);
  const renderLimitRef = useRef(0);
  // Set by snap-only navigation (undo/redo) to force the whole trace to
  // mount this render instead of ramping. Those paths stop the animation
  // first, so there's nothing for the ramp to protect -- and ramping would
  // otherwise leave the active row (and its highlight/anchor) unmounted
  // until requestIdleCallback happens to fire.
  const fullMountRef = useRef(false);
  const [, setRampTick] = useState(0);
  if (traceEntries !== prevTraceRef.current) {
    const prev = prevTraceRef.current;
    const total = traceEntries.length;
    // Unchanged leading rows are memoized by <Markdown>, so they're free.
    let shared = 0;
    const overlap = Math.min(prev.length, total);
    while (shared < overlap && prev[shared] === traceEntries[shared]) shared++;
    // Small changes (or a forced full mount) render everything now; a large
    // batch mounts only its unchanged prefix and lets the ramp add the rest.
    renderLimitRef.current =
      fullMountRef.current || total - shared <= PROGRESSIVE_CHUNK
        ? total
        : shared;
    prevTraceRef.current = traceEntries;
    fullMountRef.current = false;
  }
  const renderLimit = renderLimitRef.current;

  // Grow the render limit a chunk at a time during idle periods, yielding
  // to the animation loop between chunks.
  useEffect(() => {
    const total = traceEntries.length;
    if (renderLimit >= total) return;
    const w = window as Window & {
      requestIdleCallback?: (cb: () => void) => number;
      cancelIdleCallback?: (id: number) => void;
    };
    const schedule = w.requestIdleCallback
      ? w.requestIdleCallback.bind(w)
      : (cb: () => void) => w.setTimeout(cb, 16);
    const unschedule = w.cancelIdleCallback
      ? w.cancelIdleCallback.bind(w)
      : (id: number) => w.clearTimeout(id);
    const id = schedule(() => {
      renderLimitRef.current = Math.min(
        total,
        renderLimitRef.current + PROGRESSIVE_CHUNK,
      );
      setRampTick((n) => n + 1);
    });
    return () => unschedule(id);
  }, [renderLimit, traceEntries]);

  // Publish the trace pane width as --qs-trace-width, clamped between
  // PANE_MIN_WIDTH and PANE_MAX_WIDTH. Beyond the max, the item list
  // scrolls horizontally rather than the pane crowding the sphere.
  const PANE_MIN_WIDTH = 480;
  const PANE_MAX_WIDTH = 480;
  useEffect(() => {
    const list = traceScrollRef.current;
    if (!list) return;
    const measure = () => {
      let widestRow = 0;
      for (const row of Array.from(list.children)) {
        widestRow = Math.max(widestRow, (row as HTMLElement).scrollWidth);
      }
      const scrollbar = list.offsetWidth - list.clientWidth;
      // +2 for the pane's 1px left/right border.
      const next = Math.min(
        PANE_MAX_WIDTH,
        Math.max(PANE_MIN_WIDTH, Math.ceil(widestRow + scrollbar + 2)),
      );
      setTraceContentWidth((prev) =>
        prev !== null && Math.abs(prev - next) <= 1 ? prev : next,
      );
    };
    measure();
    // Re-measure when fonts finish loading or the host font size changes.
    const ro = new ResizeObserver(measure);
    ro.observe(list);
    for (const row of Array.from(list.children)) ro.observe(row);
    return () => ro.disconnect();
  }, [traceEntries, renderLimit]);

  // Clear any pending gate-text debounce timer on unmount so it can't
  // fire a state update after the component is gone.
  useEffect(() => {
    return () => {
      if (draftTimerRef.current !== null) clearTimeout(draftTimerRef.current);
    };
  }, []);

  // Keep the active trace row in view as `cursor` advances. We drive
  // `container.scrollTop` directly rather than `scrollIntoView`, which
  // would scroll the whole page once the pane bottoms out. The sticky
  // "latest" row overlaps the bottom of the band, so we subtract its
  // height and center the active row in the remaining visible band.
  useEffect(() => {
    const container = traceScrollRef.current;
    if (!container) return;
    const active = container.querySelector<HTMLElement>(
      ".qs-bloch-trace-item-current",
    );
    if (!active) return;
    // The latest row is sticky-pinned to the bottom, so it's always
    // visible -- skip scrolling when it's the active row, which otherwise
    // causes a small jump when the user clicks it.
    const sticky = container.querySelector<HTMLElement>(
      ".qs-bloch-trace-item-latest",
    );
    if (sticky === active) return;
    const stickyOverlap = sticky ? sticky.offsetHeight : 0;
    const visibleHeight = container.clientHeight - stickyOverlap;
    const cTop = container.scrollTop;
    const cBottom = cTop + visibleHeight;
    const aTop = active.offsetTop;
    const aBottom = aTop + active.offsetHeight;
    if (aTop < cTop || aBottom > cBottom) {
      // Center the active row, clamped to the scrollable range.
      const desired = aTop - (visibleHeight - active.offsetHeight) / 2;
      const maxScroll = container.scrollHeight - container.clientHeight;
      const target = Math.max(0, Math.min(maxScroll, desired));
      container.scrollTo({ top: target, behavior: "smooth" });
    }
  }, [cursor, gates]);

  // Spherical coordinates (theta, phi) of the qubit state after the first
  // `cursor` gates, re-walked through a throwaway `Rotations` so the
  // overlay can't drift from the renderer. Three.js axes differ from the
  // drawn Bloch axes: Bloch x = three.js z, Bloch y = three.js x, Bloch
  // z = three.js y; the state starts along three.js +Y (the |0> pole).
  const blochAngles = useMemo(() => {
    const rot = new Rotations();
    for (let i = 0; i < cursor; i++) {
      const info = resolveGate(gates[i]);
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

  // State-vector ket at the cursor, walking the same matrix product as
  // the trace but stopping at the cursor. Shown in the stage's corner.
  const currentStateLatex = useMemo(() => {
    let state: Vec2 = Ket0;
    for (let i = 0; i < cursor; i++) {
      state = resolveGate(gates[i]).matrix.mulVec2(state);
    }
    return `$$ | \\psi \\rangle = ${state.toLaTeX()} $$`;
  }, [gates, cursor]);

  // "Inspect mode" means the cursor is deliberately parked on an earlier
  // step (not a forward tail animation, which also has cursor <
  // gates.length -- hence !isPlaying). It gates editing, but NOT
  // undo/redo, which are always available when there's history.
  const inInspectMode = !isPlaying && cursor < gates.length;
  const canUndo = past.length > 0;
  const canRedo = future.length > 0;
  // Playback affordances, all derived from cursor/gates/isPlaying so the
  // media buttons can't disagree with what the sphere is doing.
  const atStart = cursor === 0;
  const atEnd = cursor >= gates.length;
  const canStepBack = !atStart;
  const canStepForward = !atEnd;
  const canPlay = gates.length > 0;

  /**
   * Stop any in-flight playback and land cleanly on a trace step. Called
   * by Pause, and as a guard before every edit/seek so the user can't
   * edit mid-rotation. No-op when already stopped. When called as a Pause
   * (snapToTarget=true) it snaps forward to the in-flight gate's
   * destination; guards pass false since they snap elsewhere next.
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
   * Animate one gate, then advance the cursor and chain to the next if
   * play is still active. The recursive chain captures `pos` per gate.
   */
  function playFromIndex(pos: number) {
    if (!renderer.current) return;
    const gate = gates[pos];
    const info = gate ? resolveGate(gate) : undefined;
    if (!info) {
      // Defensive: inputs are sanitized, but bail cleanly if one slips by.
      stopPlayback(false);
      return;
    }
    animatingToIndexRef.current = pos + 1;
    renderer.current.animateStep(info.rotateAxis, info.rotateAngle, () => {
      // If we were paused mid-gate, Pause already advanced the cursor and
      // cancelled the rAF; bail rather than chaining.
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
   * Play from the cursor to the end. If already at the end, treat the
   * click as Replay: rewind to the start and play from there.
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
    // Align the renderer's pose with `cursor` before animating, in case a
    // just-cancelled play left it a gate ahead.
    r.snapTo(gatesToSteps(gates.slice(0, cursor)));
    // Animate the inverse of the last gate (same axis, negated angle) so
    // the qubit retraces its arc backward. queueGate lays down trail dots
    // along the reverse path; they overlap the existing trail, and the
    // onComplete snapTo wipes and rebuilds the trail for [0..target-1].
    const info = resolveGate(gates[cursor - 1]);
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
    // Same guard as stepBack: align the renderer with `cursor` first.
    r.snapTo(gatesToSteps(gates.slice(0, cursor)));
    const info = resolveGate(gates[cursor]);
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
   * Snapshot the sequence before an action so Undo can return to it, and
   * clear the redo stack. Call once at the start of every gate-changing
   * action.
   */
  function pushHistory(prev: Gate[]) {
    setPast((p) => [...p, prev]);
    setFuture([]);
  }

  /**
   * Apply one new gate. If inspecting an earlier step, the future part of
   * the sequence is truncated (browser back-then-navigate semantics).
   */
  function applyGate(gate: Gate) {
    const info = resolveGate(gate);
    if (!info || !renderer.current) return;

    // Branch from the inspected step if mid-inspect, otherwise append.
    const base = cursor < gates.length ? gates.slice(0, cursor) : gates;
    // Enforce the gate cap: refuse to grow the sequence past the limit.
    if (base.length >= MAX_GATE_SEQUENCE_LENGTH) return;

    // Stop playback first; snapToTarget=false since we snap or animate next.
    stopPlayback(false);

    // Drop any pending text edit so its debounced commit can't overwrite this.
    cancelDraft();

    // Record the pre-action sequence so Undo reverts this whole action
    // (including any inspect-mode truncation) in a single step.
    pushHistory(gates);

    // Truncate future steps if the user is mid-inspect, then snap the
    // renderer there silently before kicking off the animated rotation
    // for the newly-applied gate.
    if (cursor < gates.length) {
      renderer.current.snapTo(gatesToSteps(base));
    }
    renderer.current.animateStep(info.rotateAxis, info.rotateAngle);
    const next = [...base, gate];
    setGates(next);
    setCursor(next.length);
    props.onGatesChanged?.(formatGateSequence(next));
  }

  /**
   * Move the cursor within the existing sequence without modifying it
   * (trace-row clicks, jump buttons). Snaps instantly since the user is
   * inspecting, not acting.
   */
  function navigateTo(pos: number) {
    if (!renderer.current) return;
    if (pos < 0 || pos > gates.length) return;
    // The seek implicitly pauses; snapToTarget=false since we snap below.
    stopPlayback(false);
    renderer.current.snapTo(gatesToSteps(gates.slice(0, pos)));
    setCursor(pos);
  }

  /**
   * Undo: restore the previous whole-sequence snapshot. One snapshot per
   * action, so this reverts an entire action at once (e.g. a whole Rz
   * decomposition). Always available when there's prior history.
   */
  function undo() {
    if (!canUndo || !renderer.current) return;
    stopPlayback(false);
    cancelDraft();
    const prev = past[past.length - 1];
    setPast(past.slice(0, -1));
    setFuture([gates, ...future]);
    renderer.current.snapTo(gatesToSteps(prev));
    // Mount the whole restored trace now so the active row appears at once.
    fullMountRef.current = true;
    setGates(prev);
    setCursor(prev.length);
    props.onGatesChanged?.(formatGateSequence(prev));
  }

  /**
   * Redo: restore the snapshot most recently undone away. Symmetric with
   * undo; always available when there's a state to redo.
   */
  function redo() {
    if (!canRedo || !renderer.current) return;
    stopPlayback(false);
    cancelDraft();
    const next = future[0];
    setFuture(future.slice(1));
    setPast([...past, gates]);
    renderer.current.snapTo(gatesToSteps(next));
    // Mount the whole restored trace now so the active row appears at once.
    fullMountRef.current = true;
    setGates(next);
    setCursor(next.length);
    props.onGatesChanged?.(formatGateSequence(next));
  }

  function clear() {
    stopPlayback(false);
    // Drop any pending text edit so it can't resurrect the old sequence.
    cancelDraft();
    // Record the cleared-from sequence so an accidental Clear can be undone.
    pushHistory(gates);
    setGates([]);
    setCursor(0);
    // Return the Rz slider to zero so it reflects the cleared state.
    setRzAngle(0);
    renderer.current?.reset();
    props.onGatesChanged?.("");
  }

  // ---- Live-text gate editing -------------------------------------------

  /**
   * Cancel any pending debounced commit and drop the draft so the textbox
   * mirrors `gates` again. Called by the non-text actions so a stale timer
   * can't clobber the change they just made.
   */
  function cancelDraft() {
    if (draftTimerRef.current !== null) {
      clearTimeout(draftTimerRef.current);
      draftTimerRef.current = null;
    }
    draftPendingRef.current = null;
    draftPushedRef.current = false;
    if (draftText !== null) setDraftText(null);
  }

  /**
   * Handle a keystroke in the gate textbox. Multi-character tokens
   * (`Rx(1.57)`, `SX'`) can't be sanitized per keystroke, so we keep the
   * raw text and defer parsing to the debounced commit, which extracts
   * whatever valid tokens it can (an in-progress token like `Rx(1.5` is
   * simply skipped until it's complete).
   */
  function gateTextInput(e: Event) {
    const value = (e.target as HTMLInputElement).value;
    // Typing interrupts any in-flight animation; the pending commit snaps
    // to the right place, so we snap nowhere here.
    if (isPlayingRef.current) stopPlayback(false);
    // First keystroke of a burst: snapshot the pre-edit sequence so the
    // whole burst undoes as one step, and arm the (once-per-burst) push.
    if (draftText === null) {
      draftBaseRef.current = gates;
      draftPushedRef.current = false;
    }
    setDraftText(value);
    draftPendingRef.current = value;
    if (draftTimerRef.current !== null) clearTimeout(draftTimerRef.current);
    draftTimerRef.current = window.setTimeout(
      commitDraftText,
      GATE_TEXT_DEBOUNCE_MS,
    );
  }

  /**
   * Reformat the textbox to the canonical form when it loses focus and end
   * the editing burst. Flushes any pending debounced commit first so the
   * last keystrokes aren't dropped.
   */
  function gateTextBlur() {
    if (draftTimerRef.current !== null) {
      clearTimeout(draftTimerRef.current);
      draftTimerRef.current = null;
      commitDraftText();
    }
    draftPendingRef.current = null;
    draftPushedRef.current = false;
    setDraftText(null);
  }

  /**
   * Snap to `arr[0..fromIndex]` then animate the remaining gates one at a
   * time. Shared by the actions that append a run and want the tail to
   * animate in (live-text commits, the Rz "Add to sequence" button).
   * Reads from `arr` because the caller's setGates hasn't flushed yet.
   */
  function animateTailFrom(arr: Gate[], fromIndex: number) {
    if (!renderer.current) return;
    renderer.current.snapTo(gatesToSteps(arr.slice(0, fromIndex)));
    if (fromIndex >= arr.length) {
      setCursor(arr.length);
      return;
    }
    setCursor(fromIndex);
    isPlayingRef.current = true;
    setIsPlaying(true);
    const chain = (pos: number) => {
      if (!renderer.current) return;
      const gate = arr[pos];
      const info = gate ? resolveGate(gate) : undefined;
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
    chain(fromIndex);
  }

  /**
   * Commit the pending draft text. Diffs against the sequence editing
   * started from: snap to the shared prefix, then animate the divergent
   * tail, so appending "H" to "XYZ" animates just the H. Recorded as one
   * undo step.
   */
  function commitDraftText() {
    draftTimerRef.current = null;
    const text = draftPendingRef.current;
    if (text === null || !renderer.current) return;
    const arr = parseGateSequence(text).gates;
    const current = gates;
    // Nothing changed (e.g. re-parsing to the same sequence): bail without
    // a history step so undo doesn't get a no-op entry.
    if (gatesEqual(current, arr)) return;
    stopPlayback(false);
    // Push one history entry per editing burst (the pre-edit snapshot), so
    // the whole burst -- even across several debounced commits -- undoes in
    // a single step.
    if (!draftPushedRef.current) {
      setPast((p) => [...p, draftBaseRef.current]);
      setFuture([]);
      draftPushedRef.current = true;
    }
    setGates(arr);
    props.onGatesChanged?.(formatGateSequence(arr));

    // Shared leading run between old and new; snap to it, animate the rest.
    const maxPrefix = Math.min(current.length, arr.length);
    let prefix = 0;
    while (prefix < maxPrefix && gateEqual(current[prefix], arr[prefix]))
      prefix++;
    animateTailFrom(arr, prefix);
  }

  // The Rz angle is chosen with a circular dial (JSX below). Angles are
  // snapped to the lookup-table resolution (1/200 rad per step, indexed
  // by angle*200) so the preview matches what gets committed.
  const dialRef = useRef<SVGSVGElement>(null);
  // Pending requestAnimationFrame id while dragging, to coalesce moves.
  const dialFrameRef = useRef<number | null>(null);
  const RZ_STEPS_PER_RAD = 200;
  const RZ_STEP = 1 / RZ_STEPS_PER_RAD;
  const RZ_STEPS = rzOps.length;
  // Decimal places shown in the angle field; the committed gate angle is
  // rounded to match so the sequence text agrees with the displayed value.
  const RZ_DISPLAY_DECIMALS = 3;

  // Dismiss the rotation popover (dial + decomposition) and the axis menu on
  // any pointer press outside the control. A press *inside* keeps them open,
  // so dragging the dial (which lives inside `rotRef`) never self-closes.
  useEffect(() => {
    if (!rotDialOpen && !axisMenuOpen) return;
    const onDown = (e: PointerEvent) => {
      if (!rotRef.current?.contains(e.target as Node)) {
        setRotDialOpen(false);
        setAxisMenuOpen(false);
      }
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [rotDialOpen, axisMenuOpen]);

  // Snap an angle (radians) onto the lookup-table grid and wrap into
  // [0, 2*PI), so dial, readout, and decomposition can't disagree.
  function snapAngle(a: number): number {
    let idx = Math.round(a * RZ_STEPS_PER_RAD) % RZ_STEPS;
    if (idx < 0) idx += RZ_STEPS;
    return idx / RZ_STEPS_PER_RAD;
  }

  // Pointer position to angle from the dial center. 0 rad is 3 o'clock,
  // increasing counterclockwise; SVG y points down, so negate the y delta.
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
    // Coalesce moves to one state update per frame; pointer events can
    // outpace the refresh rate and each setRzAngle re-renders.
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
    // Flush any queued frame so the final position isn't dropped.
    if (dialFrameRef.current !== null) {
      cancelAnimationFrame(dialFrameRef.current);
      dialFrameRef.current = null;
    }
    setRzAngle(angleFromPointer(e.clientX, e.clientY));
  }

  // The Rz readout doubles as a text field: users can type an angle in
  // radians and the dial + decomposition snap to the nearest grid value
  // the lookup table can produce. Parse, snap, and drop the draft so the
  // field reverts to showing the live (snapped) angle.
  function commitRzInput() {
    if (rzInputDraft === null) return;
    const parsed = Number.parseFloat(rzInputDraft);
    if (Number.isFinite(parsed)) setRzAngle(snapAngle(parsed));
    setRzInputDraft(null);
  }

  function rzInputKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      commitRzInput();
      (e.currentTarget as HTMLInputElement).blur();
    } else if (e.key === "Escape") {
      e.preventDefault();
      // Abandon the edit and restore the live angle.
      setRzInputDraft(null);
      (e.currentTarget as HTMLInputElement).blur();
    }
  }

  // Map the current dial angle to its precomputed Rz Clifford+T
  // decomposition (empty at angle 0 = identity), then adapt it to the
  // selected axis via Clifford conjugation:
  //   Rx = H Rz H ,   Ry = S H Rz H S\u2020
  const rzAngleIdx = Math.round(rzAngle * 200) % rzOps.length;
  const rzDecompString = rzOps[rzAngleIdx] ?? "";
  const rotationGate: Gate = {
    kind: AXIS_TO_ROTATION[rotationAxis],
    // Round to the displayed precision so the committed angle matches what
    // the field shows (e.g. field reads 1.400 -> stored 1.4, not the raw
    // grid value that might carry extra digits).
    angle: Number(rzAngle.toFixed(RZ_DISPLAY_DECIMALS)),
  };
  const decompositionGates = useMemo<Gate[]>(() => {
    if (rzDecompString.length === 0) return [];
    const d = decompStringToGates(rzDecompString);
    switch (rotationAxis) {
      case "X":
        return [{ kind: "H" }, ...d, { kind: "H" }];
      case "Y":
        return [
          { kind: "S'" },
          { kind: "H" },
          ...d,
          { kind: "H" },
          { kind: "S" },
        ];
      default:
        return d;
    }
  }, [rzDecompString, rotationAxis]);

  // Length of the sequence a new action would branch from: the inspected
  // prefix when mid-inspect, otherwise the whole sequence. Used to disable
  // the add controls once the gate cap is reached (the append handlers
  // enforce the same limit defensively).
  const branchLength = Math.min(cursor, gates.length);
  const atGateCap = branchLength >= MAX_GATE_SEQUENCE_LENGTH;

  // Append a run of gates (a native rotation or its decomposition) as one
  // undoable action: truncate any inspected-future steps, clear redo, and
  // animate the new gates in.
  function appendGates(run: Gate[]) {
    if (!renderer.current || run.length === 0) return;
    // Branch from the inspected step if inspecting; otherwise append.
    const base = cursor < gates.length ? gates.slice(0, cursor) : gates;
    // Enforce the gate cap: skip the action if the whole run wouldn't fit
    // (a decomposition can add many gates at once, so we don't add a
    // partial run).
    if (base.length + run.length > MAX_GATE_SEQUENCE_LENGTH) return;
    stopPlayback(false);
    cancelDraft();
    // The whole run is appended as one undoable action.
    pushHistory(gates);
    const next = [...base, ...run];
    setGates(next);
    // Leave the dial angle as-is so the user can add the rotation again.
    props.onGatesChanged?.(formatGateSequence(next));
    animateTailFrom(next, base.length);
  }

  // Append the native rotation gate for the current axis + angle.
  function applyRotation() {
    appendGates([rotationGate]);
  }

  // Append the Clifford+T decomposition for the current axis + angle.
  function applyDecomposition() {
    appendGates(decompositionGates);
  }

  // Memoized trace rows. Keying on the values they depend on (entries +
  // cursor) lets preact reuse the vnodes when only the Rz angle changed,
  // keeping the dial drag cheap as the sequence grows.
  const traceRows = useMemo(() => {
    return traceEntries.slice(0, renderLimit).map((str, i) => {
      const stepIndex = i + 1;
      const classes = ["qs-bloch-trace-item"];
      if (stepIndex === cursor) classes.push("qs-bloch-trace-item-current");
      if (stepIndex > cursor) classes.push("qs-bloch-trace-item-future");
      // Pin the last row so the latest step stays visible while scrolling
      // (see `.qs-bloch-trace-item-latest` in the CSS).
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
  }, [traceEntries, cursor, renderLimit]);

  return (
    <div
      class={"qs-bloch" + (traceCollapsed ? " qs-bloch-trace-collapsed" : "")}
      style={
        // Drive the trace column width from the measured content width
        // (--qs-trace-width). Unset until first measurement.
        traceContentWidth !== null
          ? ({
              "--qs-trace-width": `${traceContentWidth}px`,
            } as Record<string, string>)
          : undefined
      }
    >
      {/*
        Docked top toolbar: gate palette, rotation control, free-text
        program editor, and edit history. Wraps to multiple rows on narrow
        hosts so nothing clips.
      */}
      <div class="qs-bloch-toolbar">
        {/* Gate palette: single-qubit gates as one segmented control. */}
        <div class="qs-gate-buttons">
          <div
            class="qs-bloch-gate-group qs-bloch-gate-group-palette"
            role="group"
            aria-label="Apply gate"
          >
            {FIXED_GATE_KINDS.map((kind) => (
              <button
                key={kind}
                type="button"
                onClick={() => applyGate({ kind })}
                disabled={isPlaying || atGateCap}
                title={
                  atGateCap
                    ? `Sequence is at the ${MAX_GATE_SEQUENCE_LENGTH}-gate cap`
                    : undefined
                }
              >
                {resolveGate({ kind }).label}
              </button>
            ))}
          </div>
        </div>

        <div class="qs-bloch-toolbar-divider" aria-hidden="true" />

        {/*
          Rotation control: a compact "Rz( θ rad)" field with an Add button.
          Focusing the angle field opens a popover holding the draggable dial
          and the live Clifford+T decomposition; the axis dropdown selects the
          rotation axis. Both dismiss on an outside click.
        */}
        <div class="qs-bloch-rot" ref={rotRef}>
          <div class="qs-bloch-rot-field">
            <button
              type="button"
              class="qs-bloch-rot-axis"
              onClick={() => setAxisMenuOpen((v) => !v)}
              disabled={isPlaying}
              aria-haspopup="listbox"
              aria-expanded={axisMenuOpen}
              title="Rotation axis"
            >
              {AXIS_TO_ROTATION[rotationAxis]}
              <span class="qs-bloch-rot-axis-caret" aria-hidden="true">
                {"\u25BE"}
              </span>
            </button>
            {axisMenuOpen && (
              <div class="qs-bloch-rot-menu" role="listbox">
                {(["X", "Y", "Z"] as const).map((ax) => (
                  <button
                    key={ax}
                    type="button"
                    role="option"
                    aria-selected={rotationAxis === ax}
                    class={
                      "qs-bloch-rot-menu-item" +
                      (rotationAxis === ax
                        ? " qs-bloch-rot-menu-item-active"
                        : "")
                    }
                    onClick={() => {
                      setRotationAxis(ax);
                      setAxisMenuOpen(false);
                    }}
                    title={`Target the Bloch ${ax} axis (R${ax.toLowerCase()})`}
                  >
                    {AXIS_TO_ROTATION[ax]}
                  </button>
                ))}
              </div>
            )}
            <span class="qs-bloch-rot-paren" aria-hidden="true">
              (
            </span>
            <input
              class="qs-bloch-rot-arg qs-bloch-rz-input"
              type="text"
              inputMode="decimal"
              aria-label="Rotation angle in radians"
              value={
                rzInputDraft !== null
                  ? rzInputDraft
                  : rzAngle.toFixed(RZ_DISPLAY_DECIMALS)
              }
              disabled={isPlaying}
              onFocus={(e) => {
                setRzInputDraft(rzAngle.toFixed(RZ_DISPLAY_DECIMALS));
                setRotDialOpen(true);
                (e.currentTarget as HTMLInputElement).select();
              }}
              onInput={(e) =>
                setRzInputDraft((e.currentTarget as HTMLInputElement).value)
              }
              onKeyDown={rzInputKeyDown}
              onBlur={commitRzInput}
            />
            <span class="qs-bloch-rot-paren" aria-hidden="true">
              rad)
            </span>
          </div>
          <button
            type="button"
            class="qs-bloch-rz-apply qs-bloch-rot-add"
            onClick={applyRotation}
            disabled={isPlaying || rzAngle === 0 || atGateCap}
            title={
              atGateCap
                ? `Sequence is at the ${MAX_GATE_SEQUENCE_LENGTH}-gate cap`
                : rzAngle === 0
                  ? "Set a non-zero angle to add a rotation"
                  : `Append a native ${AXIS_TO_ROTATION[rotationAxis]} gate to the sequence`
            }
          >
            Add
          </button>
          {rotDialOpen && (
            <div class="qs-bloch-rot-dial">
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
                    aria-label="Rz angle in radians"
                    aria-valuemin={0}
                    aria-valuemax={(RZ_STEPS - 1) * RZ_STEP}
                    aria-valuenow={rzAngle}
                    aria-valuetext={`${rzAngle.toFixed(3)} radians`}
                    onPointerDown={dialPointerDown}
                    onPointerMove={dialPointerMove}
                    onPointerUp={dialPointerUp}
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
                    <circle
                      class="qs-bloch-rz-dial-center"
                      cx="60"
                      cy="60"
                      r="3"
                    />
                    <circle
                      class="qs-bloch-rz-dial-knob"
                      cx={knobX}
                      cy={knobY}
                      r="8"
                    />
                  </svg>
                );
              })()}
              <span class="qs-bloch-rot-dial-hint" aria-hidden="true">
                drag to set angle
              </span>
              <div class="qs-bloch-rot-decomp" aria-live="polite">
                <span class="qs-bloch-rot-decomp-label">
                  Clifford+T decomposition
                </span>
                <span class="qs-bloch-rot-decomp-str">
                  {decompositionGates.length > 0
                    ? formatGateSequence(decompositionGates)
                    : "identity (no gates)"}
                </span>
                <button
                  type="button"
                  class="qs-bloch-rz-apply"
                  onClick={applyDecomposition}
                  disabled={
                    isPlaying ||
                    decompositionGates.length === 0 ||
                    branchLength + decompositionGates.length >
                      MAX_GATE_SEQUENCE_LENGTH
                  }
                  title={
                    branchLength + decompositionGates.length >
                    MAX_GATE_SEQUENCE_LENGTH
                      ? `Decomposition would exceed the ${MAX_GATE_SEQUENCE_LENGTH}-gate cap`
                      : decompositionGates.length === 0
                        ? "Set a non-zero angle to add a decomposition"
                        : "Append the Clifford+T decomposition to the sequence"
                  }
                >
                  Add decomposition
                </button>
              </div>
            </div>
          )}
        </div>

        <div class="qs-bloch-toolbar-divider" aria-hidden="true" />

        {/* Free-text program editor with a live gate-count breakdown. */}
        <div class="qs-bloch-gate-editor">
          <div class="qs-bloch-gate-editor-row">
            <input
              class="qs-bloch-gate-editor-input"
              value={displayValue}
              onInput={gateTextInput}
              onBlur={gateTextBlur}
              spellcheck={false}
              autocomplete="off"
              autocorrect="off"
              autocapitalize="off"
              aria-label="Gate program"
              placeholder="Type a gate sequence, e.g. H Rx(1.57) SX X S'"
            />
            {props.actionSlot}
          </div>
          {/*
            Gate-count breakdown plus a T-count callout. T-count (T and T†
            gates) is the key cost metric for fault-tolerant implementations,
            so surfacing it live is useful after the rotation dial expands a
            rotation into many gates.
          */}
          <div class="qs-bloch-gate-editor-feedback" aria-hidden="true">
            <span class="qs-bloch-gate-editor-breakdown">
              {(() => {
                // Group typed gates by kind (rotations aggregate across
                // angles into a single Rx/Ry/Rz chip).
                const counts = {} as Record<GateKind, number>;
                for (const g of typedGates) {
                  counts[g.kind] = (counts[g.kind] ?? 0) + 1;
                }
                const order: GateKind[] = [
                  ...FIXED_GATE_KINDS,
                  "Rx",
                  "Ry",
                  "Rz",
                ];
                const chips = [];
                for (const kind of order) {
                  const n = counts[kind] ?? 0;
                  if (n === 0) continue;
                  const label =
                    kind === "Rx" || kind === "Ry" || kind === "Rz"
                      ? kind
                      : resolveGate({ kind }).label;
                  chips.push(
                    <span
                      key={kind}
                      class="qs-bloch-gate-editor-chip"
                      title={`${n}× ${label}`}
                    >
                      <span class="qs-bloch-gate-editor-chip-name">
                        {label}
                      </span>
                      <span class="qs-bloch-gate-editor-chip-count">{n}</span>
                    </span>,
                  );
                }
                const tCount = (counts["T"] ?? 0) + (counts["T'"] ?? 0);
                if (chips.length === 0) {
                  return (
                    <span class="qs-bloch-gate-editor-empty">no gates</span>
                  );
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
              <span
                class={
                  typedGates.length >= MAX_GATE_SEQUENCE_LENGTH
                    ? "qs-bloch-gate-editor-count qs-bloch-gate-editor-count-warn"
                    : "qs-bloch-gate-editor-count"
                }
                title={
                  typedGates.length >= MAX_GATE_SEQUENCE_LENGTH
                    ? `Sequence is at the ${MAX_GATE_SEQUENCE_LENGTH}-gate cap; remove gates to add more`
                    : ""
                }
              >
                {typedGates.length} / {MAX_GATE_SEQUENCE_LENGTH}
              </span>
            </span>
          </div>
        </div>

        <div class="qs-bloch-toolbar-spacer" />

        {/* Edit history: undo/redo and clear, as segmented controls. */}
        <div class="qs-gate-buttons">
          <div
            class="qs-bloch-gate-group"
            role="group"
            aria-label="Edit history"
          >
            <button
              type="button"
              onClick={undo}
              disabled={!canUndo}
              title="Undo last gate"
            >
              Undo
            </button>
            <button
              type="button"
              onClick={redo}
              disabled={!canRedo}
              title="Redo last undone gate"
            >
              Redo
            </button>
          </div>
          <div class="qs-bloch-gate-group" role="group">
            <button
              type="button"
              onClick={clear}
              title="Clear the entire trace"
            >
              Clear
            </button>
          </div>
        </div>
      </div>

      <div class="qs-bloch-body">
        <div class="qs-bloch-main">
          <div class="qs-bloch-stage" ref={stageRef}>
            <canvas ref={canvasRef}></canvas>
            {/*
              Top-left readout: current state ket above the θ/φ angles.
            */}
            <div class="qs-bloch-readout" aria-hidden="true">
              <div class="qs-bloch-state">
                <Markdown markdown={currentStateLatex}></Markdown>
              </div>
              <div class="qs-bloch-coords">
                <span>
                  <span class="qs-bloch-coords-greek">θ</span>
                  {" = "}
                  {blochAngles.theta.toFixed(2)} rad
                </span>
                <span>
                  <span class="qs-bloch-coords-greek">φ</span>
                  {" = "}
                  {blochAngles.polar
                    ? "n/a"
                    : `${blochAngles.phi.toFixed(2)} rad`}
                </span>
              </div>
            </div>
            {/*
            When the trace pane is collapsed, surface a small toggle to
            reopen it, plus (when there are gates) a compact playback
            transport so the animation can still be driven without the pane.
            Mirrors the main transport minus the speed slider.
          */}
            {traceCollapsed && (
              <div class="qs-bloch-trace-toggle-overlay">
                {gates.length > 0 && (
                  <div
                    class="qs-bloch-mini-transport"
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
                      <TransportIcon name="jump-to-start" />
                    </button>
                    <button
                      type="button"
                      onClick={stepBack}
                      disabled={!canStepBack}
                      title="Step back"
                      aria-label="Step back"
                    >
                      <TransportIcon name="step-back" />
                    </button>
                    {isPlaying ? (
                      <button
                        type="button"
                        onClick={pause}
                        title="Pause"
                        aria-label="Pause"
                      >
                        <TransportIcon name="pause" />
                      </button>
                    ) : (
                      <button
                        type="button"
                        onClick={play}
                        disabled={!canPlay}
                        title={atEnd ? "Replay from start" : "Play from here"}
                        aria-label={atEnd ? "Replay from start" : "Play"}
                      >
                        <TransportIcon name={atEnd ? "replay" : "play"} />
                      </button>
                    )}
                    <button
                      type="button"
                      onClick={stepForward}
                      disabled={!canStepForward}
                      title="Step forward"
                      aria-label="Step forward"
                    >
                      <TransportIcon name="step-forward" />
                    </button>
                    <button
                      type="button"
                      onClick={jumpToEnd}
                      disabled={!canStepForward}
                      title="Jump to end"
                      aria-label="Jump to end"
                    >
                      <TransportIcon name="jump-to-end" />
                    </button>
                  </div>
                )}
                <button
                  type="button"
                  class="qs-bloch-trace-toggle"
                  onClick={() => setTraceCollapsed(false)}
                  title="Show trace panel"
                  aria-label="Show trace panel"
                  aria-expanded={false}
                >
                  <span class="qs-bloch-trace-toggle-icon" aria-hidden="true">
                    {"\u00AB"}
                  </span>
                  <span class="qs-bloch-trace-toggle-label">Trace</span>
                </button>
              </div>
            )}
          </div>
        </div>
        <div class="qs-bloch-trace" style="font-size: 0.9em;">
          <div class="qs-bloch-trace-inner" aria-hidden={traceCollapsed}>
            <div class="qs-bloch-trace-title">
              <span>Trace</span>
              <span class="qs-bloch-trace-title-right">
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
                <button
                  type="button"
                  class="qs-bloch-trace-toggle"
                  onClick={() => setTraceCollapsed(true)}
                  title="Hide trace panel"
                  aria-label="Hide trace panel"
                  aria-expanded={true}
                >
                  <span class="qs-bloch-trace-toggle-label">Hide</span>
                  <span class="qs-bloch-trace-toggle-icon" aria-hidden="true">
                    {"\u00BB"}
                  </span>
                </button>
              </span>
            </div>
            {/*
          Media transport controls: jump-to-start, step-back,
          play/pause/replay, step-forward, jump-to-end. Step/jump are
          seek-only; the centre button is the only animated path.
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
                <TransportIcon name="jump-to-start" />
              </button>
              <button
                type="button"
                onClick={stepBack}
                disabled={!canStepBack}
                title="Step back"
                aria-label="Step back"
              >
                <TransportIcon name="step-back" />
              </button>
              {isPlaying ? (
                <button
                  type="button"
                  onClick={pause}
                  title="Pause"
                  aria-label="Pause"
                >
                  <TransportIcon name="pause" />
                </button>
              ) : (
                <button
                  type="button"
                  onClick={play}
                  disabled={!canPlay}
                  title={atEnd ? "Replay from start" : "Play from here"}
                  aria-label={atEnd ? "Replay from start" : "Play"}
                >
                  {/* Replay icon when the cursor is at the end (clicking
                  rewinds first); play triangle otherwise. */}
                  <TransportIcon name={atEnd ? "replay" : "play"} />
                </button>
              )}
              <button
                type="button"
                onClick={stepForward}
                disabled={!canStepForward}
                title="Step forward"
                aria-label="Step forward"
              >
                <TransportIcon name="step-forward" />
              </button>
              <button
                type="button"
                onClick={jumpToEnd}
                disabled={!canStepForward}
                title="Jump to end"
                aria-label="Jump to end"
              >
                <TransportIcon name="jump-to-end" />
              </button>
            </div>
            {/*
          Speed slider. The value is the speed multiplier (higher =
          faster); the renderer translates it back to milliseconds.
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
              style="overflow-y: auto; overflow-x: auto; flex: 1; display: flex; flex-direction: column; align-items: stretch; min-height: 0;"
            >
              <div
                class={
                  "qs-bloch-trace-item" +
                  (cursor === 0 ? " qs-bloch-trace-item-current" : "") +
                  (traceEntries.length === 0
                    ? " qs-bloch-trace-item-latest"
                    : "")
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
      </div>
    </div>
  );
}
