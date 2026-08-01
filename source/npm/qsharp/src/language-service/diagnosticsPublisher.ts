// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { VSDiagnostic } from "../../lib/web/qsc_wasm.js";

export interface DiagnosticsPublisherOptions {
  publish: (uri: string, diagnostics: VSDiagnostic[]) => void;
  /** Returns a function that cancels the scheduled callback. */
  schedule: (callback: () => void, delayMs: number) => () => void;
  /** How long after the last keystroke to wait before publishing. */
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
 * Only that one document is ever held, so there is at most one pending entry. Everything
 * else - bulk reloads, background opens, other files in the same project - publishes as it
 * arrives. The caller decides which document is "hot"; this type has no opinion on how
 * that's determined, and takes its timer from the caller so it can be tested without one.
 */
export class DiagnosticsPublisher {
  private hotUri: string | undefined;
  private pending: Pending | undefined;
  private cancelIdle: (() => void) | undefined;
  private cancelCap: (() => void) | undefined;

  constructor(private readonly options: DiagnosticsPublisherOptions) {}

  /** Identifies the document being typed in. Anything else publishes immediately. */
  setHotUri(uri: string | undefined) {
    if (this.hotUri === uri) {
      return;
    }
    this.hotUri = uri;

    // A pending entry belongs to the document the user just left, which will get no further
    // keystrokes to end its burst.
    if (this.pending && this.pending.uri !== uri) {
      this.flush();
    }
  }

  receive(uri: string, diagnostics: VSDiagnostic[]) {
    if (uri !== this.hotUri) {
      this.options.publish(uri, diagnostics);
      return;
    }

    // Clearing the last error is the one result worth showing mid-token.
    if (diagnostics.length === 0) {
      this.reset();
      this.options.publish(uri, diagnostics);
      return;
    }

    this.pending = { uri, diagnostics };

    this.cancelIdle?.();
    this.cancelIdle = this.options.schedule(() => {
      this.cancelIdle = undefined;
      this.flush();
    }, this.options.delayMs);

    // Deliberately not restarted per keystroke - that is what makes it a cap rather than a
    // second debounce, so a long typing run still refreshes.
    if (!this.cancelCap) {
      this.cancelCap = this.options.schedule(() => {
        this.cancelCap = undefined;
        this.flush();
      }, this.options.maxDelayMs);
    }
  }

  flush() {
    const pending = this.pending;
    this.reset();
    if (pending) {
      this.options.publish(pending.uri, pending.diagnostics);
    }
  }

  dispose() {
    this.reset();
  }

  private reset() {
    this.pending = undefined;
    this.cancelIdle?.();
    this.cancelIdle = undefined;
    this.cancelCap?.();
    this.cancelCap = undefined;
  }
}
