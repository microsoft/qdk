// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { CircuitModel } from "../data/circuitModel.js";
import { InteractionState } from "../actions/interactionState.js";
import { LayoutMap } from "../renderer/layoutMap.js";

/**
 * `InteractionContext` — shared dependencies passed to every editor
 * controller. The single object is built once in `CircuitEvents`'s
 * constructor and handed to each controller; controllers read/write
 * the same `model` / `interaction` and dispatch to the same
 * `renderFn` so they all observe a consistent view of the editor.
 *
 * Controllers are translation-only: pointer/keyboard event listeners
 * that turn raw DOM events into `*Actions.*` calls. They hold no
 * state of their own — everything mutable lives on `model` (Data
 * layer) or `interaction` (ephemeral session state).
 *
 * Fields are mutable on purpose. `wireData` is grown/shrunk by
 * qubit-line edits; `circuitSvg` etc. are re-resolved on each
 * `enableEvents` re-run. The context object itself is meant to be
 * built once per `CircuitEvents` instance, not per event.
 */
export interface InteractionContext {
  /** The Data layer. Owns componentGrid, qubits, qubitUseCounts. */
  readonly model: CircuitModel;
  /** Ephemeral session state — selection, drag flags, etc. */
  readonly interaction: InteractionState;
  /** Geometry from the layout pass, indexed by hierarchical scope. */
  readonly layoutMap: LayoutMap;
  /** Outer host element (the editor's container). */
  readonly container: HTMLElement;
  /** The rendered `svg.qviz` root. */
  readonly circuitSvg: SVGElement;
  /**
   * The editor-only overlay group inside `svg.qviz`. Holds every
   * editor-owned DOM node — dropzones, ghost qubit row, future
   * selection rectangles / hover halos / Inspector anchors.
   * Controllers append wire dropzones (and any ad-hoc overlay
   * elements they need) to this group instead of to `circuitSvg`,
   * keeping the renderer-owned children of the SVG purely
   * presentational.
   */
  readonly overlayLayer: SVGGElement;
  /** Editor-only overlay layer for inter-column / on-column dropzones. */
  readonly dropzoneLayer: SVGGElement;
  /** Editor-only overlay layer for the ghost qubit wire. */
  readonly ghostQubitLayer: SVGGElement;
  /**
   * Wire Y positions in absolute svg coords. Mutable because
   * qubit-line removals splice an entry out (see `QubitController`).
   */
  wireData: number[];
  /** Triggers a renderer re-run; controllers call after model edits. */
  readonly renderFn: () => void;
}
