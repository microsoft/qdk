// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/* The math for converting basis coefficients (a, b) to a Bloch-sphere
   point is:
     theta = 2 * acos(magnitude(a))
     phi   = arg(b) - arg(a), normalized to [0, 2 * PI)
*/

import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { ComponentChildren } from "preact";

import { Vector3 } from "three";

import { Rotations, Ket0, Vec2 } from "../cplx.js";
import { Markdown } from "../renderers.js";
import { detectThemeChange, ensureTheme } from "../themeObserver.js";
import {
  gateInfo,
  MAX_GATE_SEQUENCE_LENGTH,
  sanitizeGateSequence,
  VALID_GATE_CODES,
} from "./blochGates.js";
import { BlochRenderer, DEFAULT_ROTATION_TIME_MS } from "./blochRenderer.js";

import rzOps from "../../rz-array.json";

// Markdown for the initial |0> state shown as the first trace row. Kept
// as a module constant so the trace list and the hidden width-probe
// (see `widthProbe`) render exactly the same source.
const INITIAL_KET_MARKDOWN =
  "$$ | \\psi \\rangle_0 = \\begin{bmatrix} 1 \\\\ 0 \\end{bmatrix} $$";

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
  //   * `past` / `future` are the undo/redo history, kept as whole
  //     sequence snapshots (see below). They are *not* per-gate: each
  //     user action records one snapshot, so undoing reverts an entire
  //     action -- including a multi-gate "Add sequence" -- in one step.
  //
  // Inspect mode (cursor < gates.length) is signalled visually by a
  // persistent banner, dimmed/italicized future rows, and disabled
  // Undo/Redo buttons. Applying a new gate while inspecting commits the
  // truncation (future rows become discarded). This mirrors how
  // browsers and most editors handle "navigate back, then act".
  const [gates, setGates] = useState<string[]>([]);
  const [cursor, setCursor] = useState(0);
  // Undo/redo history as full-sequence snapshots. `past` holds the
  // sequences that preceded the current one (oldest first, newest
  // last); `future` holds sequences that were undone away (the next one
  // to redo is first).
  const [past, setPast] = useState<string[][]>([]);
  const [future, setFuture] = useState<string[][]>([]);
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

  // Live-text editing of the gate sequence.
  //
  //   * `draftText === null` means the textbox simply mirrors the
  //     committed sequence (`gates.join("")`); buttons, undo, etc. flow
  //     straight through.
  //   * `draftText !== null` means the user is actively typing. The
  //     textbox shows their text *immediately* (so typing stays snappy),
  //     but the expensive part -- recomputing every trace row and snapping
  //     the sphere -- is deferred until they pause (`GATE_TEXT_DEBOUNCE_MS`
  //     after the last keystroke). The math itself is sub-millisecond; the
  //     debounce exists to avoid rebuilding the whole trace on every
  //     keystroke, not because the state calc is slow.
  //
  // Input is sanitized as it's typed, so the textbox only ever contains
  // valid gate codes -- there's no transient "invalid"/"unsaved" state to
  // reconcile, and the sphere (once the debounce fires) always matches
  // exactly what's shown.
  const GATE_TEXT_DEBOUNCE_MS = 150;
  const [draftText, setDraftText] = useState<string | null>(null);
  const displayValue = draftText ?? gates.join("");
  // Pending-commit timer id, the text waiting to be committed, and a
  // snapshot of `gates` taken at the start of an editing burst (so the
  // whole burst collapses into a single undoable step).
  const draftTimerRef = useRef<number | null>(null);
  const draftPendingRef = useRef<string | null>(null);
  const draftBaseRef = useRef<string[]>([]);

  // Measured natural width (px) of the widest piece of trace content,
  // used to size the trace pane so it grows horizontally to fit the
  // wide `gate . |psi> = result` equations instead of clipping them or
  // showing a horizontal scrollbar. Null until first measurement.
  const [traceContentWidth, setTraceContentWidth] = useState<number | null>(
    null,
  );

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
  const PANE_MIN_WIDTH = 480;
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
  }, [traceEntries, renderLimit]);

  // Clear any pending gate-text debounce timer on unmount so it can't
  // fire a state update after the component is gone.
  useEffect(() => {
    return () => {
      if (draftTimerRef.current !== null) clearTimeout(draftTimerRef.current);
    };
  }, []);

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

  // "Inspect mode" means the user has *deliberately* parked the cursor on
  // an earlier step to look at it (as opposed to a forward tail animation,
  // which also has cursor < gates.length -- hence !isPlaying). It gates
  // editing actions, but NOT undo/redo: those are always available when
  // there's a history state to navigate to. Triggering one stops any
  // in-flight animation and snaps to the restored sequence (see undo/redo).
  const inInspectMode = !isPlaying && cursor < gates.length;
  const canUndo = past.length > 0;
  const canRedo = future.length > 0;
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
   * Record the sequence as it was *before* the current action so Undo
   * can return to it, and clear the redo `future` (a fresh action
   * invalidates anything that was previously undone away). Call this
   * once at the start of every action that changes `gates`.
   */
  function pushHistory(prev: string[]) {
    setPast((p) => [...p, prev]);
    setFuture([]);
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

    // Drop any pending text edit so its debounced commit can't fire
    // after this gate and overwrite it.
    cancelDraft();

    // Record the pre-action sequence so Undo reverts this whole action
    // (including any inspect-mode truncation) in a single step.
    pushHistory(gates);

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
   * Step back one entry in the history, restoring the whole previous
   * sequence snapshot. Because history records one snapshot per action,
   * this reverts an entire action at once -- e.g. undoing "Add sequence"
   * removes the whole Rz decomposition. Always available when there's a
   * prior state: stops any in-flight animation and snaps to the end of the
   * restored sequence.
   */
  function undo() {
    if (!canUndo || !renderer.current) return;
    stopPlayback(false);
    cancelDraft();
    const prev = past[past.length - 1];
    setPast(past.slice(0, -1));
    // The sequence we're leaving becomes the next thing Redo restores.
    setFuture([gates, ...future]);
    renderer.current.snapTo(gatesToSteps(prev));
    // Snap navigation: mount the whole restored trace at once so the
    // active row (and its highlight/anchor) appears immediately.
    fullMountRef.current = true;
    setGates(prev);
    setCursor(prev.length);
    props.onGatesChanged?.(prev.join(""));
  }

  /**
   * Step forward one entry in the history, restoring the snapshot that
   * was most recently undone away. Like undo, it's always available when
   * there's a state to redo: stops any in-flight animation and snaps to
   * the end of the restored sequence (symmetric with undo).
   */
  function redo() {
    if (!canRedo || !renderer.current) return;
    stopPlayback(false);
    cancelDraft();
    const next = future[0];
    setFuture(future.slice(1));
    // The sequence we're leaving goes back onto the undo history.
    setPast([...past, gates]);
    renderer.current.snapTo(gatesToSteps(next));
    // Snap navigation: mount the whole restored trace at once so the
    // active row (and its highlight/anchor) appears immediately.
    fullMountRef.current = true;
    setGates(next);
    setCursor(next.length);
    props.onGatesChanged?.(next.join(""));
  }

  function clear() {
    stopPlayback(false);
    // A pending text edit would otherwise fire after the clear and
    // resurrect the old sequence; drop it first.
    cancelDraft();
    // Clear is an editing action like any other: record the cleared-from
    // sequence so an accidental Clear can be undone.
    pushHistory(gates);
    setGates([]);
    setCursor(0);
    // Also return the Rz slider to its zero position so the control
    // reflects the cleared state rather than a stale angle.
    setRzAngle(0);
    renderer.current?.reset();
    props.onGatesChanged?.("");
  }

  // ---- Live-text gate editing -------------------------------------------

  /**
   * Cancel any pending debounced commit and drop the draft so the textbox
   * falls back to mirroring `gates`. Called by the non-text actions
   * (buttons, undo/redo, clear) so a stale timer can't clobber the change
   * they just made.
   */
  function cancelDraft() {
    if (draftTimerRef.current !== null) {
      clearTimeout(draftTimerRef.current);
      draftTimerRef.current = null;
    }
    draftPendingRef.current = null;
    if (draftText !== null) setDraftText(null);
  }

  /**
   * Handle a keystroke in the gate textbox. Sanitize immediately (so the
   * field only ever holds valid codes), show the result right away for a
   * responsive feel, and schedule the expensive sphere/trace update for
   * `GATE_TEXT_DEBOUNCE_MS` after the user stops typing.
   */
  function gateTextInput(e: Event) {
    const value = (e.target as HTMLInputElement).value;
    const clean = sanitizeGateSequence(value).gates;
    // Typing is an edit, so it interrupts any in-flight animation --
    // including the tail animation a previous commit may have started.
    // We snap nowhere here; the pending commit will snap to the right
    // place. This keeps the textbox focused/enabled the whole time.
    if (isPlayingRef.current) stopPlayback(false);
    // First keystroke of a burst: snapshot the pre-edit sequence so the
    // whole burst undoes as one step.
    if (draftText === null) draftBaseRef.current = gates;
    setDraftText(clean);
    draftPendingRef.current = clean;
    if (draftTimerRef.current !== null) clearTimeout(draftTimerRef.current);
    draftTimerRef.current = window.setTimeout(
      commitDraftText,
      GATE_TEXT_DEBOUNCE_MS,
    );
  }

  /**
   * Snap the sphere to `arr[0..fromIndex]` and then animate the
   * remaining gates one at a time through to the end, advancing the
   * cursor as each completes. Shared by the actions that add a run of
   * gates and want the new tail to *animate* into place (like a gate
   * button does) rather than teleport: live-text commits and the Rz
   * "Add to sequence" button. Reads gates from the passed `arr` because
   * the caller's `setGates` hasn't flushed yet, mirroring how
   * `playFromIndex` reads `gates`. No-op tail (fromIndex at or past the
   * end) just lands the cursor.
   */
  function animateTailFrom(arr: string[], fromIndex: number) {
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
      const info = gateInfo[arr[pos]];
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
   * Commit the pending draft text to `gates`. Rather than teleporting to
   * the final state, we diff against the sequence we started editing
   * from: everything up to the first differing gate is a shared prefix,
   * so we snap instantly to that prefix and then *animate* the divergent
   * tail. The edit then reads as "continue from where the two sequences
   * still agree" -- appending "H" to "XYZ" animates just the H, and even
   * a mid-string change only re-animates from the point of divergence.
   * The whole burst is recorded as a single undo step.
   */
  function commitDraftText() {
    draftTimerRef.current = null;
    const text = draftPendingRef.current;
    draftPendingRef.current = null;
    if (text === null || !renderer.current) return;
    const arr = text.split("");
    const prev = draftBaseRef.current;
    // Nothing actually changed during this burst (e.g. pasting the same
    // text, or a stray input event). Drop the draft without recording a
    // history step -- otherwise undo gets a no-op entry that appears to
    // only reset the trace position.
    if (prev.join("") === text) {
      if (draftText !== null) setDraftText(null);
      return;
    }
    stopPlayback(false);
    // Record the pre-burst sequence as a single undoable step.
    setPast((p) => [...p, prev]);
    setFuture([]);
    setGates(arr);
    // Back to canonical mirroring -- gates.join("") now equals the text.
    setDraftText(null);
    props.onGatesChanged?.(text);

    // Length of the shared leading run between old and new sequences.
    const maxPrefix = Math.min(prev.length, arr.length);
    let prefix = 0;
    while (prefix < maxPrefix && prev[prefix] === arr[prefix]) prefix++;

    // Snap to the shared prefix, then animate everything past it.
    animateTailFrom(arr, prefix);
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
  // (inspected-past) steps, add the new gates, and clear the redo stack.
  // Like a gate button, the newly-appended decomposition *animates* into
  // place gate by gate (via `animateTailFrom`) rather than teleporting;
  // the user can Pause partway or scrub the trace to inspect each step.
  function applyRzDecomposition() {
    if (!renderer.current || rzDecomposition.length === 0) return;
    stopPlayback(false);
    cancelDraft();
    // The whole decomposition is appended as one undoable action.
    pushHistory(gates);
    // If the user is inspecting an earlier step, branch from there by
    // truncating the later gates; otherwise append at the end.
    const base = cursor < gates.length ? gates.slice(0, cursor) : gates;
    const next = [...base, ...rzDecomposition.split("")];
    setGates(next);
    // Leave the dial at its current angle so the user can add the same
    // rotation again without re-dialing it.
    props.onGatesChanged?.(next.join(""));
    // Snap to the prefix, then animate the appended gates into place.
    animateTailFrom(next, base.length);
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
    // Mount only up to the progressive render limit (see above).
    return traceEntries.slice(0, renderLimit).map((str, i) => {
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
  }, [traceEntries, cursor, renderLimit]);

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
              {"\u23EE\uFE0E"}
            </button>
            <button
              type="button"
              onClick={stepBack}
              disabled={!canStepBack}
              title="Step back"
              aria-label="Step back"
            >
              {"\u23EA\uFE0E"}
            </button>
            {isPlaying ? (
              <button
                type="button"
                onClick={pause}
                title="Pause"
                aria-label="Pause"
              >
                {"\u23F8\uFE0E"}
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
                  standard play triangle. The trailing U+FE0E forces
                  text-style (non-emoji) presentation so these match the
                  step/jump glyphs. */}
                {atEnd ? "\u21BB" : "\u23F5\uFE0E"}
              </button>
            )}
            <button
              type="button"
              onClick={stepForward}
              disabled={!canStepForward}
              title="Step forward"
              aria-label="Step forward"
            >
              {"\u23E9\uFE0E"}
            </button>
            <button
              type="button"
              onClick={jumpToEnd}
              disabled={!canStepForward}
              title="Jump to end"
              aria-label="Jump to end"
            >
              {"\u23ED\uFE0E"}
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
        {/* Gate palette: the single-qubit gates that can be applied to
            the current state. Rendered as one segmented control so the
            eight gates read as a single related set rather than a row of
            disconnected buttons. */}
        <div
          class="qs-bloch-gate-group qs-bloch-gate-group-palette"
          role="group"
          aria-label="Apply gate"
        >
          {(
            [
              ["X", "X"],
              ["Y", "Y"],
              ["Z", "Z"],
              ["H", "H"],
              ["S", "S"],
              ["s", "S†"],
              ["T", "T"],
              ["t", "T†"],
            ] as const
          ).map(([code, label]) => (
            <button
              key={code}
              type="button"
              onClick={() => applyGate(code)}
              disabled={isPlaying}
            >
              {label}
            </button>
          ))}
        </div>

        {/* Edit history: undo/redo of the gate sequence, grouped as a
            second segmented control. */}
        <div class="qs-bloch-gate-group" role="group" aria-label="Edit history">
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
            disabled={isPlaying}
            title={isPlaying ? "Pause to clear" : "Clear the entire trace"}
          >
            Clear
          </button>
        </div>
      </div>
      <div class="qs-bloch-gate-editor">
        <div class="qs-bloch-gate-editor-row">
          <input
            class="qs-bloch-gate-editor-input"
            value={displayValue}
            onInput={gateTextInput}
            spellcheck={false}
            autocomplete="off"
            autocorrect="off"
            autocapitalize="off"
            aria-label="Gate program"
            placeholder="Type a gate sequence (X Y Z H S s T t)"
          />
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
        <div class="qs-bloch-gate-editor-feedback" aria-hidden="true">
          <span class="qs-bloch-gate-editor-breakdown">
            {(() => {
              const counts: Record<string, number> = {};
              for (const ch of displayValue) {
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
            <span
              class={
                displayValue.length > MAX_GATE_SEQUENCE_LENGTH
                  ? "qs-bloch-gate-editor-count qs-bloch-gate-editor-count-warn"
                  : "qs-bloch-gate-editor-count"
              }
              title={
                displayValue.length > MAX_GATE_SEQUENCE_LENGTH
                  ? `Sequence exceeds the ${MAX_GATE_SEQUENCE_LENGTH}-gate cap`
                  : ""
              }
            >
              {displayValue.length} / {MAX_GATE_SEQUENCE_LENGTH}
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
