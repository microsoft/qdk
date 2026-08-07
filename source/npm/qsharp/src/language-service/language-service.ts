// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type {
  ICodeAction,
  ICodeLens,
  ICompletionList,
  IHover,
  ILocation,
  INotebookMetadata,
  IPosition,
  IRange,
  ISignatureHelp,
  ITextEdit,
  IWorkspaceConfiguration,
  IWorkspaceEdit,
  LanguageService,
  VSDiagnostic,
  ITestDescriptor,
} from "../../lib/web/qsc_wasm.js";
import { IProjectHost } from "../main.js";
import { log } from "../log.js";
import type {
  IServiceEventTarget,
  IServiceProxy,
  ServiceProtocol,
} from "../workers/types.js";
type QscWasm = typeof import("../../lib/web/qsc_wasm.js");

export type LanguageServiceDiagnosticEvent = {
  type: "diagnostics";
  detail: {
    uri: string;
    version: number;
    diagnostics: VSDiagnostic[];
  };
};

export type LanguageServiceTestCallablesEvent = {
  type: "testCallables";
  detail: {
    callables: ITestDescriptor[];
  };
};

export type LanguageServiceEvent =
  | LanguageServiceDiagnosticEvent
  | LanguageServiceTestCallablesEvent;

/**
 * A completion list, plus whether the caller should ask again as the user keeps typing.
 *
 * VS Code filters a complete list client-side and won't re-request it, so this has to be
 * set whenever the list isn't the real answer for the requested document version.
 */
export type CompletionListResult = ICompletionList & {
  isIncomplete?: boolean;
};

/**
 * How long to wait for a completion request's document version to be compiled.
 *
 * This is a liveness backstop, not a tuning knob. Expiring doesn't fail the request: the
 * list is computed against whatever version is current and flagged incomplete, so the
 * cost of waiting too little is a provisional answer rather than no answer.
 */
const completionWaitTimeoutMs = 2000;

// These need to be async/promise results for when communicating across a WebWorker, however
// for running the compiler in the same thread the result will be synchronous (a resolved promise).
export interface ILanguageService {
  updateConfiguration(config: IWorkspaceConfiguration): Promise<void>;
  updateDocument(
    uri: string,
    version: number,
    code: string,
    languageId?: string,
  ): Promise<void>;
  updateNotebookDocument(
    notebookUri: string,
    version: number,
    metadata: INotebookMetadata,
    cells: {
      uri: string;
      version: number;
      code: string;
    }[],
  ): Promise<void>;
  closeDocument(uri: string, languageId?: string): Promise<void>;
  closeNotebookDocument(notebookUri: string): Promise<void>;
  getCodeActions(documentUri: string, range: IRange): Promise<ICodeAction[]>;
  getCompletions(
    documentUri: string,
    version: number,
    position: IPosition,
  ): Promise<CompletionListResult>;
  getFormatChanges(documentUri: string): Promise<ITextEdit[]>;
  getHover(
    documentUri: string,
    position: IPosition,
  ): Promise<IHover | undefined>;
  getDefinition(
    documentUri: string,
    position: IPosition,
  ): Promise<ILocation | undefined>;
  getReferences(
    documentUri: string,
    position: IPosition,
    includeDeclaration: boolean,
  ): Promise<ILocation[]>;
  getSignatureHelp(
    documentUri: string,
    position: IPosition,
  ): Promise<ISignatureHelp | undefined>;
  getRename(
    documentUri: string,
    position: IPosition,
    newName: string,
  ): Promise<IWorkspaceEdit | undefined>;
  prepareRename(
    documentUri: string,
    position: IPosition,
  ): Promise<ITextEdit | undefined>;
  getCodeLenses(documentUri: string): Promise<ICodeLens[]>;

  dispose(): Promise<void>;

  addEventListener<T extends LanguageServiceEvent["type"]>(
    type: T,
    listener: (event: Extract<LanguageServiceEvent, { type: T }>) => void,
  ): void;

  removeEventListener<T extends LanguageServiceEvent["type"]>(
    type: T,
    listener: (event: Extract<LanguageServiceEvent, { type: T }>) => void,
  ): void;
}

export const qsharpLibraryUriScheme = "qsharp-library-source";
export const qsharpGithubUriScheme = "qsharp-github-source";

export type ILanguageServiceWorker = ILanguageService & IServiceProxy;

/**
 * Builds a function that yields to the host's macrotask queue.
 *
 * The update loop calls this to let the host deliver input events that piled up while
 * a compilation was blocking the event loop, so they can be coalesced instead of
 * processed one at a time.
 *
 * The primitive matters. `setImmediate` runs in Node's check phase, after the poll
 * phase where the extension host reads queued IPC messages. `MessageChannel` posts an
 * unclamped task that queues behind already-posted message events. A plain
 * `setTimeout(0)` is neither: it runs in the timers phase, which can precede poll, and
 * browsers clamp it to 4ms once nested. It is only a last resort.
 */
export function createHostYield(): () => Promise<void> {
  if (typeof setImmediate === "function") {
    return () => new Promise<void>((resolve) => setImmediate(resolve));
  }

  if (typeof MessageChannel === "function") {
    const channel = new MessageChannel();
    const pending: (() => void)[] = [];
    channel.port1.onmessage = () => pending.shift()?.();
    return () =>
      new Promise<void>((resolve) => {
        pending.push(resolve);
        channel.port2.postMessage(null);
      });
  }

  return () => new Promise<void>((resolve) => setTimeout(resolve, 0));
}

export class QSharpLanguageService implements ILanguageService {
  private languageService: LanguageService;
  private eventHandler =
    new EventTarget() as IServiceEventTarget<LanguageServiceEvent>;

  private updateLoop: Promise<void>;

  constructor(
    private wasm: QscWasm,
    host: IProjectHost = {
      readFile: async () => null,
      listDirectory: async () => [],
      resolvePath: async () => null,
      fetchGithub: async () => "",
      findManifestDirectory: async () => null,
    },
  ) {
    log.info("Constructing a QSharpLanguageService instance");
    this.languageService = new wasm.LanguageService();

    this.updateLoop = this.languageService.start_update_loop(
      this.onDiagnostics.bind(this),
      this.onTestCallables.bind(this),
      host,
      createHostYield(),
    );
  }

  async updateConfiguration(config: IWorkspaceConfiguration): Promise<void> {
    this.languageService.update_configuration(config);
  }

  async updateDocument(
    documentUri: string,
    version: number,
    code: string,
    languageId?: string,
  ): Promise<void> {
    this.languageService.update_document(
      documentUri,
      version,
      code,
      languageId || "qsharp",
    );
  }

  async updateNotebookDocument(
    notebookUri: string,
    version: number,
    metadata: INotebookMetadata,
    cells: { uri: string; version: number; code: string }[],
  ): Promise<void> {
    this.languageService.update_notebook_document(notebookUri, metadata, cells);
  }

  async closeDocument(documentUri: string, languageId?: string): Promise<void> {
    this.languageService.close_document(documentUri, languageId || "qsharp");
  }

  async closeNotebookDocument(documentUri: string): Promise<void> {
    this.languageService.close_notebook_document(documentUri);
  }

  async getCodeActions(
    documentUri: string,
    range: IRange,
  ): Promise<ICodeAction[]> {
    return this.languageService.get_code_actions(documentUri, range);
  }

  async getCompletions(
    documentUri: string,
    version: number,
    position: IPosition,
  ): Promise<CompletionListResult> {
    // The position was computed against this version, so answering against an older one
    // gets the wrong list: the last character typed is often what determines the answer,
    // as in `Foo.`. Waiting avoids that. A newer version is a lesser problem, since the
    // position only drifts if the intervening edits moved it, so it's tolerated below.
    const status = await this.languageService.wait_for_document_version(
      documentUri,
      version,
      completionWaitTimeoutMs,
    );

    const completions: CompletionListResult =
      this.languageService.get_completions(documentUri, position);

    if (status !== "ready") {
      log.info(
        `Providing completions for ${documentUri} from a ${status === "timeout" ? "older" : "newer"} version than requested`,
      );
      // Attempt to signal to the editor that the list is provisional and a fresh request should
      // be made on the next keystroke (vs just filtering).
      completions.isIncomplete = true;
    }

    return completions;
  }

  async getFormatChanges(documentUri: string): Promise<ITextEdit[]> {
    return this.languageService.get_format_changes(documentUri);
  }

  async getHover(
    documentUri: string,
    position: IPosition,
  ): Promise<IHover | undefined> {
    return this.languageService.get_hover(documentUri, position);
  }

  async getDefinition(
    documentUri: string,
    position: IPosition,
  ): Promise<ILocation | undefined> {
    return this.languageService.get_definition(documentUri, position);
  }

  async getReferences(
    documentUri: string,
    position: IPosition,
    includeDeclaration: boolean,
  ): Promise<ILocation[]> {
    return this.languageService.get_references(
      documentUri,
      position,
      includeDeclaration,
    );
  }

  async getSignatureHelp(
    documentUri: string,
    position: IPosition,
  ): Promise<ISignatureHelp | undefined> {
    return this.languageService.get_signature_help(documentUri, position);
  }

  async getRename(
    documentUri: string,
    position: IPosition,
    newName: string,
  ): Promise<IWorkspaceEdit | undefined> {
    return this.languageService.get_rename(documentUri, position, newName);
  }

  async prepareRename(
    documentUri: string,
    position: IPosition,
  ): Promise<ITextEdit | undefined> {
    return this.languageService.prepare_rename(documentUri, position);
  }

  async getCodeLenses(documentUri: string): Promise<ICodeLens[]> {
    return this.languageService.get_code_lenses(documentUri);
  }

  async dispose() {
    this.languageService.stop_update_loop();
    await this.updateLoop;
    this.languageService.free();
  }

  addEventListener<T extends LanguageServiceEvent["type"]>(
    type: T,
    listener: (event: Extract<LanguageServiceEvent, { type: T }>) => void,
  ) {
    this.eventHandler.addEventListener(type, listener);
  }

  removeEventListener<T extends LanguageServiceEvent["type"]>(
    type: T,
    listener: (event: Extract<LanguageServiceEvent, { type: T }>) => void,
  ) {
    this.eventHandler.removeEventListener(type, listener);
  }

  async onDiagnostics(
    uri: string,
    version: number | undefined,
    diagnostics: VSDiagnostic[],
  ) {
    try {
      const event = new Event("diagnostics") as LanguageServiceDiagnosticEvent &
        Event;
      event.detail = {
        uri,
        version: version ?? 0,
        diagnostics,
      };
      this.eventHandler.dispatchEvent(event);
    } catch (e) {
      log.error("Error in onDiagnostics", e);
    }
  }

  async onTestCallables(callables: ITestDescriptor[]) {
    try {
      const event = new Event(
        "testCallables",
      ) as LanguageServiceTestCallablesEvent & Event;
      event.detail = {
        callables,
      };
      this.eventHandler.dispatchEvent(event);
    } catch (e) {
      log.error("Error in onTestCallables", e);
    }
  }
}

/**
 * The protocol definition to allow running the language service in a worker.
 *
 * Not to be confused with "the" LSP (Language Server Protocol).
 */
export const languageServiceProtocol: ServiceProtocol<
  ILanguageService,
  LanguageServiceDiagnosticEvent
> = {
  class: QSharpLanguageService,
  methods: {
    updateConfiguration: "request",
    updateDocument: "request",
    updateNotebookDocument: "request",
    closeDocument: "request",
    closeNotebookDocument: "request",
    getCodeActions: "request",
    getCompletions: "request",
    getFormatChanges: "request",
    getHover: "request",
    getDefinition: "request",
    getReferences: "request",
    getSignatureHelp: "request",
    getRename: "request",
    prepareRename: "request",
    getCodeLenses: "request",
    dispose: "request",
    addEventListener: "addEventListener",
    removeEventListener: "removeEventListener",
  },
  eventNames: ["diagnostics"],
};
