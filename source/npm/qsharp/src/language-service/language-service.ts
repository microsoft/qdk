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
import {
  IServiceEventTarget,
  IServiceProxy,
  ServiceProtocol,
} from "../workers/common.js";
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
    position: IPosition,
  ): Promise<ICompletionList>;
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

export class QSharpLanguageService implements ILanguageService {
  private languageService: LanguageService;
  private eventHandler =
    new EventTarget() as IServiceEventTarget<LanguageServiceEvent>;

  private backgroundWork: Promise<void>;

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

    this.backgroundWork = this.languageService.start_background_work(
      this.onDiagnostics.bind(this),
      this.onTestCallables.bind(this),
      host,
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
    position: IPosition,
  ): Promise<ICompletionList> {
    // Tiny delay to let the compilation catch up before we invoke
    // the completion provider.
    // This becomes important when the completion list is triggered
    // during typing. If the last character typed is significant to
    // the completion (e.g. in `Foo.` completions)
    // it's critical that the completion provider "sees" this character.
    await new Promise((resolve) => setTimeout(resolve, 50));
    return this.languageService.get_completions(documentUri, position);
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
    this.languageService.stop_background_work();
    await this.backgroundWork;
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
