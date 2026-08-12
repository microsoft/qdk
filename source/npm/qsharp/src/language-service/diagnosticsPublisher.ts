// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { VSDiagnostic } from "../../lib/web/qsc_wasm.js";

/**
 * Both configuration for DiagnosticsPublisher and hooks so that different editors
 * (and tests) can implement key operations differently.
 */
export interface DiagnosticsPublisherImpl {
  /** Called when diagnostics are ready to be displayed to the user. */
  publish: (uri: string, diagnostics: VSDiagnostic[]) => void;
  /** Called to schedule deferred work. */
  schedule: (callback: () => void, delayMs: number) => () => void;
  /** How long after the last edit to wait before publishing. */
  delayMs: number;
  /** Upper bound on how long sustained typing can withhold a publish. */
  maxDelayMs: number;
}

interface PendingDiagnostics {
  uri: string;
  diagnostics: VSDiagnostic[];
}

/**
 * Withholds diagnostics for the document the user is currently typing in, so squiggles
 * don't churn on every character of a half-written token.
 *
 * The goal is to have a small delay when the user is idle but a cap on the maximum delay
 * before squiggles are drawn.
 *
 * Only the active file is affected - everything else (e.g. closed files) still publishes
 * ASAP.
 */
export class DiagnosticsPublisher {
  // Active file (i.e. subject to delays)
  private activeUri: string | undefined;
  // Deferred diagnostics for `activeUri`
  private pending: PendingDiagnostics | undefined;
  private cancelIdle: (() => void) | undefined;
  private cancelCap: (() => void) | undefined;

  constructor(private readonly impl: DiagnosticsPublisherImpl) {}

  /// Callback for when a document is edited
  onEdit(uri: string | undefined) {
    if (uri !== this.activeUri) {
      // A pending entry belongs to the document the user just left, which will get no
      // further edits to end its burst.
      this.publishPending();
      this.endBurst();
      this.activeUri = uri;
    }

    // Indicates a non-QDK document (which is still interesting, since it changes the activeUri)
    if (uri === undefined) {
      return;
    }

    this.cancelIdle?.();
    this.cancelIdle = this.impl.schedule(() => {
      this.cancelIdle = undefined;
      this.endBurst();
      this.publishPending();
    }, this.impl.delayMs);

    // Deliberately not restarted per edit - that is what makes it a cap rather than a
    // second debounce, so a long typing run still refreshes.
    if (!this.cancelCap) {
      this.cancelCap = this.impl.schedule(() => {
        this.cancelCap = undefined;
        this.publishPending();
      }, this.impl.maxDelayMs);
    }
  }

  onDiagnosticsUpdate(uri: string, diagnostics: VSDiagnostic[]) {
    // Nothing is waiting on this result: either it's for another document, or the
    // typing already stopped and the burst ended.
    if (uri !== this.activeUri || !this.isBursting) {
      this.impl.publish(uri, diagnostics);
      return;
    }

    // Clearing the last error is the one result worth showing mid-token.
    if (diagnostics.length === 0) {
      this.pending = undefined;
      this.impl.publish(uri, diagnostics);
      return;
    }

    this.pending = { uri, diagnostics };
  }

  publishPending() {
    const pending = this.pending;
    this.pending = undefined;
    if (pending) {
      this.impl.publish(pending.uri, pending.diagnostics);
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
