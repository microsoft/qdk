// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { VSDiagnostic } from "../../lib/web/qsc_wasm.js";

export interface DiagnosticsPublisherOptions {
  publish: (uri: string, diagnostics: VSDiagnostic[]) => void;
  /** Returns a function that cancels the scheduled callback. */
  schedule: (callback: () => void, delayMs: number) => () => void;
  /** How long after the last edit to wait before publishing. */
  delayMs: number;
  /** Upper bound on how long sustained typing can withhold a publish. */
  maxDelayMs: number;
}

interface Pending {
  uri: string;
  diagnostics: VSDiagnostic[];
}

/**
 * Withholds diagnostics for the document the user is currently typing in, so squiggles
 * don't churn on every character of a half-written token.
 *
 * The wait runs from the last edit rather than the last result, so a pause the user has
 * already taken counts against it and a slow compilation adds no delay of its own.
 *
 * Only the document being edited is ever held, so there is at most one pending entry.
 * Everything else - bulk reloads, background opens, other files in the same project -
 * publishes as it arrives. The caller reports the edits; this type has no opinion on how
 * they're detected, and takes its timer from the caller so it can be tested without one.
 */
export class DiagnosticsPublisher {
  private hotUri: string | undefined;
  private pending: Pending | undefined;
  private cancelIdle: (() => void) | undefined;
  private cancelCap: (() => void) | undefined;

  constructor(private readonly options: DiagnosticsPublisherOptions) {}

  /** Reports an edit. `uri` is undefined for documents we don't publish diagnostics for. */
  noteEdit(uri: string | undefined) {
    if (uri !== this.hotUri) {
      // A pending entry belongs to the document the user just left, which will get no
      // further edits to end its burst.
      this.flush();
      this.endBurst();
      this.hotUri = uri;
    }

    if (uri === undefined) {
      return;
    }

    this.cancelIdle?.();
    this.cancelIdle = this.options.schedule(() => {
      this.cancelIdle = undefined;
      this.endBurst();
      this.flush();
    }, this.options.delayMs);

    // Deliberately not restarted per edit - that is what makes it a cap rather than a
    // second debounce, so a long typing run still refreshes.
    if (!this.cancelCap) {
      this.cancelCap = this.options.schedule(() => {
        this.cancelCap = undefined;
        this.flush();
      }, this.options.maxDelayMs);
    }
  }

  receive(uri: string, diagnostics: VSDiagnostic[]) {
    // A result that arrives once the typing has stopped is the one being waited on.
    if (uri !== this.hotUri || !this.isBursting) {
      this.options.publish(uri, diagnostics);
      return;
    }

    // Clearing the last error is the one result worth showing mid-token.
    if (diagnostics.length === 0) {
      this.pending = undefined;
      this.options.publish(uri, diagnostics);
      return;
    }

    this.pending = { uri, diagnostics };
  }

  flush() {
    const pending = this.pending;
    this.pending = undefined;
    if (pending) {
      this.options.publish(pending.uri, pending.diagnostics);
    }
  }

  dispose() {
    this.pending = undefined;
    this.endBurst();
  }

  /** A burst runs from an edit until `delayMs` of quiet. */
  private get isBursting() {
    return this.cancelIdle !== undefined;
  }

  private endBurst() {
    this.cancelIdle?.();
    this.cancelIdle = undefined;
    this.cancelCap?.();
    this.cancelCap = undefined;
  }
}
