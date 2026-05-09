// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { type ICompilerWorker, log } from "qsharp-lang";
import * as vscode from "vscode";
import { loadCompilerWorker } from "./common";

/**
 * URI scheme used for read-only Q# previews of circuit files.
 *
 * Documents under this scheme are served by `CircuitPreviewProvider` and
 * appear to VS Code as regular Q# documents, so they automatically pick up
 * Q# syntax highlighting, the language service, the editor theme, etc.
 *
 * The corresponding circuit document's URI is carried in the preview URI's
 * `query`, which keeps the preview URI stable across edits without baking
 * the (possibly long, possibly platform-specific) source path into the
 * preview's display path.
 */
export const qsharpCircuitPreviewScheme = "qsharp-circuit-preview";

/**
 * Build the deterministic preview URI for a given circuit document.
 *
 * The path is purely cosmetic (it controls the editor tab label and is what
 * the language service sees as the document name). The original circuit
 * URI is **not** encoded in the URI itself — it's stashed in a side-channel
 * map keyed by preview-URI-as-string. We used to put the source URI in the
 * preview URI's `query`, but `URI.toString()` includes the query verbatim,
 * which means downstream consumers that treat the URI string as a path —
 * notably `Path::extension()` in `qsc::packages::convert_circuit_sources`
 * — would see the `.qsc` extension at the tail of the query and try to
 * parse the preview Q# text as circuit JSON, surfacing a spurious
 * `Error: expected value at line 1 column 1` diagnostic on every preview.
 *
 * Keying the side-channel map by the *encoded* basename (the same
 * basename that ends up in the URI path) keeps the mapping deterministic
 * across re-opens of the same document while still allowing two documents
 * with the same basename in different folders to coexist (we disambiguate
 * by appending the source URI's hash to the path when there's a
 * collision).
 */
export function circuitPreviewUriFor(circuitUri: vscode.Uri): vscode.Uri {
  // The preview path becomes the implicit Q# namespace when the language
  // service compiles the preview as a single-file project, and the stem
  // becomes the operation name in the generated Q#. Both must be valid
  // Q# identifiers — see `sanitizeQsharpIdentifier` for the rules and the
  // motivating bug (a file named `GroupSplittingTest.Main.qsc` would
  // otherwise produce `operation GroupSplittingTest.Main(...)` and a
  // namespace name with an embedded `.`, both syntax errors).
  //
  // Strip the source extension first so we don't sanitize the trailing
  // `.qsc` into `_qsc` — a `Foo.qsc` source should preview as `Foo.qs`,
  // not `Foo_qsc.qs`.
  const rawBasename = circuitUri.path.split(/[\\/]/).pop() ?? "circuit";
  const rawStem = rawBasename.replace(/\.[^.]+$/, "") || rawBasename;
  const stem = sanitizeQsharpIdentifier(rawStem);
  // If two open circuits share a sanitized stem (either because their
  // basenames are identical or because sanitization mapped two distinct
  // inputs to the same identifier), give them distinct preview paths so
  // VS Code treats them as separate documents — otherwise the first one
  // to register would win and the second would appear blank. The hash
  // is short, deterministic per source URI, and identifier-safe.
  const existing = _sourceLookup.get(stem);
  let path: string;
  if (!existing || existing.toString() === circuitUri.toString()) {
    path = `/${stem}.qs`;
    _sourceLookup.set(stem, circuitUri);
  } else {
    const suffix = shortHash(circuitUri.toString());
    const disambiguated = `${stem}_${suffix}`;
    path = `/${disambiguated}.qs`;
    _sourceLookup.set(disambiguated, circuitUri);
  }
  return vscode.Uri.from({
    scheme: qsharpCircuitPreviewScheme,
    path,
  });
}

/**
 * Coerce an arbitrary string (typically a `.qsc` file basename) into a
 * valid Q# identifier suitable for use as an implicit-namespace component.
 *
 * Mirrors `sanitize_qsharp_identifier` in
 * `compiler/qsc_circuit/src/circuit_to_qsharp.rs`; both layers must agree
 * so the preview URI's namespace and the generated `operation` name line
 * up. Rules:
 *
 * * Keep each char if it is ASCII alphanumeric or `_`; otherwise replace
 *   it with `_`.
 * * If the result is empty or starts with a digit, prefix with `_`.
 */
function sanitizeQsharpIdentifier(raw: string): string {
  let out = "";
  for (const ch of raw) {
    if (/[A-Za-z0-9_]/.test(ch)) {
      out += ch;
    } else {
      out += "_";
    }
  }
  if (out.length === 0 || /^[0-9]/.test(out)) {
    out = `_${out}`;
  }
  return out;
}

/**
 * Side-channel mapping from preview URI key (the basename used as the
 * preview path) to the original circuit URI. Populated by
 * `circuitPreviewUriFor` and consumed by `sourceUriFromPreviewUri`.
 *
 * Lives at module scope so any caller of `circuitPreviewUriFor` and any
 * subsequent `provideTextDocumentContent` see the same mapping. Entries
 * are never removed — the basenames are short and the worst-case memory
 * impact is bounded by the number of distinct circuit files the user
 * opens in a session.
 */
const _sourceLookup = new Map<string, vscode.Uri>();

/**
 * Compact, stable hash of the source URI used to disambiguate same-basename
 * collisions. Not a security primitive — just needs to be deterministic and
 * collision-resistant enough for "more than one Foo.qsc open at once".
 */
function shortHash(input: string): string {
  // FNV-1a 32-bit; tiny, deterministic, no deps.
  let h = 0x811c9dc5;
  for (let i = 0; i < input.length; i++) {
    h ^= input.charCodeAt(i);
    h = (h + ((h << 1) + (h << 4) + (h << 7) + (h << 8) + (h << 24))) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

/**
 * Recover the source `.qsc` URI for a preview URI from the side-channel
 * map. Returns `undefined` if no entry is registered yet — most commonly
 * when VS Code restored a preview tab from a previous session and the
 * corresponding `.qsc` custom editor hasn't activated yet to populate the
 * map. The lazy-regen path treats `undefined` as "wait for the editor",
 * which is correct: when the `.qsc` editor activates it will call
 * `setContent` and fire `onDidChange`, refreshing the placeholder.
 */
function sourceUriFromPreviewUri(
  previewUri: vscode.Uri,
): vscode.Uri | undefined {
  // Strip the leading `/` and the trailing `.qs` to recover the lookup key.
  const key = previewUri.path.replace(/^\//, "").replace(/\.qs$/, "");
  return _sourceLookup.get(key);
}

/**
 * Read-only content provider that serves Q# code generated from a circuit.
 *
 * The fast path: the circuit editor calls `setContent` whenever the user
 * edits the circuit, and `provideTextDocumentContent` returns whatever was
 * pushed last. This keeps the preview live during editing without the
 * provider having to know anything about wasm or the compiler.
 *
 * The slow path: when VS Code restores a preview tab from a previous
 * session, the corresponding `.qsc` custom editor may not have activated
 * yet (custom editors are lazy-loaded by VS Code). In that case nothing
 * has called `setContent`, so the provider would normally show its
 * placeholder forever. To avoid that, the provider can also asynchronously
 * generate content directly from the source `.qsc` file: on a cache miss
 * it returns the placeholder synchronously and kicks off a regeneration
 * that calls `setContent` when done, which fires `onDidChange` and makes
 * VS Code refresh the open tab. This recovers the preview without
 * depending on the custom editor running.
 *
 * Content is keyed by the preview URI (as a string), not the source circuit
 * URI, so two circuits with the same basename in different folders never
 * collide even though their tab labels are identical.
 */
export class CircuitPreviewProvider
  implements vscode.TextDocumentContentProvider, vscode.Disposable
{
  private readonly _onDidChange = new vscode.EventEmitter<vscode.Uri>();
  private readonly _content = new Map<string, string>();
  /**
   * URIs we've already kicked off a lazy regeneration for. Prevents
   * stampedes when VS Code re-fetches the same restored tab multiple times
   * before the first regeneration finishes (which it does, especially
   * during initial activation).
   */
  private readonly _regenerating = new Set<string>();
  private _worker: ICompilerWorker | undefined;

  readonly onDidChange = this._onDidChange.event;

  constructor(private readonly extensionUri: vscode.Uri) {}

  /**
   * Update the cached Q# content for a preview URI.
   *
   * Fires `onDidChange` so any open editor showing the preview re-fetches
   * via `provideTextDocumentContent`.
   */
  setContent(uri: vscode.Uri, content: string): void {
    this._content.set(uri.toString(), content);
    this._onDidChange.fire(uri);
  }

  /**
   * Drop cached content for a preview URI. Subsequent fetches will fall
   * back to the placeholder text (and may trigger a lazy regeneration).
   */
  clearContent(uri: vscode.Uri): void {
    if (this._content.delete(uri.toString())) {
      this._onDidChange.fire(uri);
    }
  }

  provideTextDocumentContent(uri: vscode.Uri): string {
    const cached = this._content.get(uri.toString());
    if (cached !== undefined) return cached;

    // Cache miss. Most likely VS Code restored this preview tab before the
    // custom editor for the source .qsc file activated. Kick off a lazy
    // regeneration directly from the source file so the user doesn't see
    // the placeholder indefinitely. Fire-and-forget; if the source file is
    // unreadable or the generator fails, we surface that as an error
    // comment by writing it back through `setContent`.
    void this.regenerateFromSource(uri);

    // Placeholder shown while regeneration is in flight (and as the final
    // value if the source file no longer exists). Q# comments so syntax
    // highlighting still renders sensibly.
    return "// Q# preview will appear here as you edit the circuit.\n";
  }

  /**
   * Read the source `.qsc` file from disk and regenerate the preview Q#.
   * Idempotent per-URI: a regeneration already in flight is not duplicated.
   *
   * On success, the regenerated content is pushed via `setContent`, which
   * fires `onDidChange` so VS Code refreshes the open editor for `uri`.
   */
  private async regenerateFromSource(uri: vscode.Uri): Promise<void> {
    const key = uri.toString();
    if (this._regenerating.has(key)) return;
    this._regenerating.add(key);
    try {
      const sourceUri = sourceUriFromPreviewUri(uri);
      if (!sourceUri) return;

      let text: string;
      try {
        const bytes = await vscode.workspace.fs.readFile(sourceUri);
        text = new TextDecoder().decode(bytes);
      } catch (err) {
        log.debug(
          "circuit preview regen: failed to read source",
          sourceUri.toString(),
          err,
        );
        return;
      }

      const operationName = previewOperationNameFor(sourceUri);

      if (text.trim().length === 0) {
        this.setContent(
          uri,
          `// Q# preview \u2014 empty circuit\n// Add gates to ${operationName} to generate Q#.\n`,
        );
        return;
      }

      // Validate JSON on the host first so a corrupt file produces a
      // friendly comment instead of a wasm panic.
      let parsed: unknown;
      try {
        parsed = JSON.parse(text);
      } catch (err: any) {
        this.setContent(
          uri,
          previewErrorComment(
            "invalid JSON",
            `Circuit file is not valid JSON: ${err?.message ?? err}`,
          ),
        );
        return;
      }

      let qsharp: string;
      try {
        const worker = this.getWorker();
        qsharp = await worker.circuitsToQsharp(operationName, parsed as any);
      } catch (err: any) {
        this.setContent(
          uri,
          previewErrorComment(
            "generation failed",
            `Could not generate Q#: ${err?.message ?? err}`,
          ),
        );
        return;
      }

      this.setContent(uri, qsharp);
    } finally {
      this._regenerating.delete(key);
    }
  }

  private getWorker(): ICompilerWorker {
    if (!this._worker) {
      this._worker = loadCompilerWorker(this.extensionUri);
    }
    return this._worker;
  }

  dispose(): void {
    this._content.clear();
    this._regenerating.clear();
    if (this._worker) {
      this._worker.terminate();
      this._worker = undefined;
    }
    this._onDidChange.dispose();
  }
}

/**
 * Singleton instance of the preview provider.
 *
 * Created and registered with VS Code from `extension.ts` activation.
 * Held here so other modules (notably `CircuitEditorProvider`) can push
 * generated Q# into it without having to thread the provider through
 * many layers of constructors.
 */
let _provider: CircuitPreviewProvider | undefined;

/**
 * Register the circuit preview content provider with VS Code.
 *
 * Returns a Disposable suitable for `context.subscriptions.push(...)`.
 * Calling this more than once is a programming error.
 */
export function registerCircuitPreviewProvider(
  extensionUri: vscode.Uri,
): vscode.Disposable {
  if (_provider !== undefined) {
    throw new Error("Circuit preview provider has already been registered.");
  }
  _provider = new CircuitPreviewProvider(extensionUri);
  const registration = vscode.workspace.registerTextDocumentContentProvider(
    qsharpCircuitPreviewScheme,
    _provider,
  );
  return vscode.Disposable.from(registration, _provider, {
    dispose: () => {
      _provider = undefined;
    },
  });
}

/**
 * Get the registered preview provider, if any.
 *
 * Returns `undefined` before activation has registered it (or after
 * deactivation), in which case callers should silently skip preview
 * updates rather than fail.
 */
export function getCircuitPreviewProvider():
  | CircuitPreviewProvider
  | undefined {
  return _provider;
}

/**
 * Derive the Q# operation name shown in the preview from the source URI.
 *
 * Mirrors the convention used by the circuit editor: basename minus
 * extension. Falls back to a safe default for URIs without a recognizable
 * basename.
 */
function previewOperationNameFor(circuitUri: vscode.Uri): string {
  const basename = circuitUri.path.split(/[\\/]/).pop() ?? "";
  const name = basename.replace(/\.[^/.]+$/, "");
  return name.length > 0 ? name : "Circuit";
}

/**
 * Format an error message as a Q# comment block so the preview tab keeps
 * rendering as valid Q# even when generation fails. The `kind` shows up in
 * the header so users can tell at a glance whether the issue is a malformed
 * file vs. a compiler problem.
 */
function previewErrorComment(kind: string, message: string): string {
  const lines = String(message).split(/\r?\n/);
  return [
    `// Q# preview unavailable \u2014 ${kind}`,
    ...lines.map((line) => `// ${line}`),
    "",
  ].join("\n");
}
