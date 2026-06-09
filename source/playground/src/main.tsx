// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Use esbuild to bundle and copy the CSS files to the output directory.
import "modern-normalize/modern-normalize.css";
import "./main.css";

import { render } from "preact";

import type {
  CompilerState,
  VSDiagnostic,
  LogLevel,
  ILanguageService,
} from "qsharp-lang";

import {
  QscEventTarget,
  getCompilerWorker,
  loadWasmModule,
  log,
  samples,
  openqasm_samples,
  getLanguageServiceWorker,
} from "qsharp-lang";

// The playground Katas viewer uses the Markdown version of the katas
import { Kata, getAllKatas } from "qsharp-lang/katas-md";

import { Nav } from "./nav.js";
import { Editor } from "./editor.js";
import { registerOpenQasmLanguage } from "./openqasm-language.js";
import { OutputTabs } from "./tabs.js";
import { useEffect, useState } from "preact/hooks";
import { Kata as Katas } from "./kata.js";
import {
  DocumentationDisplay,
  getNamespaces,
  processDocumentFiles,
} from "./docs.js";
import {
  compressedBase64ToCode,
  lsRangeToMonacoRange,
  lsToMonacoWorkspaceEdit,
  monacoPositionToLsPosition,
  monacoRangetoLsRange,
} from "./utils.js";

// Set up the Markdown renderer with KaTeX support
import mk from "@vscode/markdown-it-katex";
import markdownIt from "markdown-it";
import { setRenderer } from "qsharp-lang/ux";

const md = markdownIt("commonmark");
md.use((mk as any).default, {
  enableMathBlockInHtml: true,
  enableMathInlineInHtml: true,
}); // Not sure why it's not using the default export automatically :-/
setRenderer((input: string) => md.render(input));

export type ActiveTab =
  | "results-tab"
  | "ast-tab"
  | "hir-tab"
  | "rir-tab"
  | "qir-tab";

const basePath = (window as any).qscBasePath || "";
const monacoPath = basePath + "libs/monaco/vs";
const modulePath = basePath + "libs/qsharp/qsc_wasm_bg.wasm";
const compilerWorkerPath = basePath + "libs/compiler-worker.js";
const languageServiceWorkerPath = basePath + "libs/language-service-worker.js";

function telemetryHandler({ id, data }: { id: string; data?: any }) {
  // NOTE: This is for demo purposes. Wire up to the real telemetry library.
  console.log(`Received telemetry event: "%s" with payload: %o`, id, data);
}

function createCompiler(onStateChange: (val: CompilerState) => void) {
  log.info("In createCompiler");
  const compiler = getCompilerWorker(compilerWorkerPath);
  compiler.onstatechange = onStateChange;
  return compiler;
}

function App(props: {
  katas: Kata[];
  linkedCode?: string;
  linkedLanguage?: "qsharp" | "openqasm";
}) {
  const [compilerState, setCompilerState] = useState<CompilerState>("idle");
  const [compiler, setCompiler] = useState(() =>
    createCompiler(setCompilerState),
  );

  const [compiler_worker_factory] = useState(() => {
    const compiler_worker_factory = () => getCompilerWorker(compilerWorkerPath);
    return compiler_worker_factory;
  });

  const [evtTarget] = useState(() => new QscEventTarget(true));

  const [languageService] = useState(() => {
    const languageService = getLanguageServiceWorker(languageServiceWorkerPath);
    registerMonacoLanguageServiceProviders(languageService);
    return languageService;
  });

  const [currentNavItem, setCurrentNavItem] = useState(
    props.linkedCode ? "linked" : "sample-Minimal",
  );
  const [shotError, setShotError] = useState<VSDiagnostic | undefined>(
    undefined,
  );

  const [ast, setAst] = useState<string>("");
  const [hir, setHir] = useState<string>("");
  const [rir, setRir] = useState<string[]>(["", ""]);
  const [qir, setQir] = useState<string>("");
  const [activeTab, setActiveTab] = useState<ActiveTab>("results-tab");

  const onRestartCompiler = () => {
    compiler.terminate();
    const newCompiler = createCompiler(setCompilerState);
    setCompiler(newCompiler);
    setCompilerState("idle");
  };

  const kataTitles = props.katas.map((elem) => elem.title);
  const sampleTitles = samples.map((sample) => sample.title);
  const openqasmSampleTitles = openqasm_samples.map((sample) => sample.title);

  const [documentation, setDocumentation] = useState<
    Map<string, string> | undefined
  >(undefined);
  useEffect(() => {
    createDocumentation();
  }, []);
  async function createDocumentation() {
    const docFiles = await compiler.getDocumentation();
    setDocumentation(processDocumentFiles(docFiles));
  }

  // OpenQASM samples are namespaced with their own nav prefix because some
  // sample titles (e.g. "Random Number Generator") exist in both languages.
  const isOpenQasmSample = currentNavItem.startsWith("openqasm-sample-");
  // For shared links the language comes from the URL (defaulting to Q#); for
  // samples it's derived from which sample set the nav item belongs to.
  const currentLanguage: "qsharp" | "openqasm" =
    currentNavItem === "linked"
      ? (props.linkedLanguage ?? "qsharp")
      : isOpenQasmSample
        ? "openqasm"
        : "qsharp";

  const selectedSample = isOpenQasmSample
    ? openqasm_samples.find(
        (sample) => "openqasm-sample-" + sample.title === currentNavItem,
      )
    : samples.find((sample) => "sample-" + sample.title === currentNavItem);

  const sampleCode = selectedSample?.code || props.linkedCode;

  const defaultShots = selectedSample?.shots || 100;

  const activeKata = kataTitles.includes(currentNavItem)
    ? props.katas.find((kata) => kata.title === currentNavItem)
    : undefined;

  function onNavItemSelected(name: string) {
    // If there was a ?code link on the URL before, clear it out
    const newURL = new URL(window.location.href);
    if (newURL.searchParams.get("code")) {
      newURL.searchParams.delete("code");
      newURL.searchParams.delete("profile");
      newURL.searchParams.delete("lang");
      window.history.pushState({}, "", newURL.toString());
      props.linkedCode = undefined;
    }
    setCurrentNavItem(name);
  }

  return (
    <>
      <header class="page-header">Q# playground</header>
      <Nav
        selected={currentNavItem}
        navSelected={onNavItemSelected}
        katas={kataTitles}
        samples={sampleTitles}
        openqasmSamples={openqasmSampleTitles}
        namespaces={getNamespaces(documentation)}
      ></Nav>
      {sampleCode ? (
        <>
          <Editor
            key={currentLanguage}
            language={currentLanguage}
            code={sampleCode}
            compiler={compiler}
            compiler_worker_factory={compiler_worker_factory}
            compilerState={compilerState}
            onRestartCompiler={onRestartCompiler}
            evtTarget={evtTarget}
            defaultShots={defaultShots}
            showShots={true}
            showExpr={true}
            shotError={shotError}
            setAst={setAst}
            setHir={setHir}
            setRir={setRir}
            setQir={setQir}
            activeTab={activeTab}
            languageService={languageService}
          ></Editor>
          <OutputTabs
            evtTarget={evtTarget}
            showPanel={true}
            onShotError={(diag?: VSDiagnostic) => setShotError(diag)}
            ast={ast}
            hir={hir}
            rir={rir}
            qir={qir}
            activeTab={activeTab}
            setActiveTab={setActiveTab}
          ></OutputTabs>
        </>
      ) : activeKata ? (
        <Katas
          kata={activeKata!}
          compiler={compiler}
          compiler_worker_factory={compiler_worker_factory}
          compilerState={compilerState}
          onRestartCompiler={onRestartCompiler}
          languageService={languageService}
        ></Katas>
      ) : (
        <DocumentationDisplay
          currentNamespace={currentNavItem}
          documentation={documentation}
        ></DocumentationDisplay>
      )}
      <div id="popup"></div>
    </>
  );
}

// Called once Monaco is ready
async function loaded() {
  // Configure any logging as early as possible
  const logLevelUri = new URLSearchParams(window.location.search).get(
    "logLevel",
  );
  if (logLevelUri) {
    log.setLogLevel(logLevelUri as LogLevel);
  } else {
    log.setLogLevel("error");
  }
  log.setTelemetryCollector(telemetryHandler);

  await loadWasmModule(modulePath);

  const katas = await getAllKatas({ includeUnpublished: true });

  // If URL is a sharing link, populate the editor with the code from the link.
  // Otherwise, populate with sample code.
  let linkedCode: string | undefined;
  const params = new URLSearchParams(window.location.search);
  const paramCode = params.get("code");
  if (paramCode) {
    try {
      const base64code = decodeURIComponent(paramCode);
      linkedCode = await compressedBase64ToCode(base64code);
    } catch {
      linkedCode = "// Unable to decode the code in the URL\n";
    }
  }
  // Language of a shared link, defaulting to Q# so existing links keep working.
  const linkedLanguage = params.get("lang") === "openqasm" ? "openqasm" : "qsharp";

  render(
    <App
      katas={katas}
      linkedCode={linkedCode}
      linkedLanguage={linkedLanguage}
    ></App>,
    document.body,
  );
}

// Languages that share the language-service-backed providers below. OpenQASM
// supports completions, go-to-definition, find-references and rename in the
// language service, matching the set registered by the VS Code extension.
const qdkLanguages = ["qsharp", "openqasm"];

function registerMonacoLanguageServiceProviders(
  languageService: ILanguageService,
) {
  registerOpenQasmLanguage();

  monaco.languages.registerCompletionItemProvider(qdkLanguages, {
    // @ts-expect-error - Monaco's types expect range to be defined,
    // but it's actually optional and the default behavior is better
    provideCompletionItems: async (
      model: monaco.editor.ITextModel,
      position: monaco.Position,
    ) => {
      const completions = await languageService.getCompletions(
        model.uri.toString(),
        monacoPositionToLsPosition(position),
      );
      return {
        suggestions: completions.items.map((i) => {
          let kind;
          switch (i.kind) {
            case "function":
              kind = monaco.languages.CompletionItemKind.Function;
              break;
            case "interface":
              kind = monaco.languages.CompletionItemKind.Interface;
              break;
            case "keyword":
              kind = monaco.languages.CompletionItemKind.Keyword;
              break;
            case "variable":
              kind = monaco.languages.CompletionItemKind.Variable;
              break;
            case "typeParameter":
              kind = monaco.languages.CompletionItemKind.TypeParameter;
              break;
            case "module":
              kind = monaco.languages.CompletionItemKind.Module;
              break;
            case "property":
              kind = monaco.languages.CompletionItemKind.Property;
              break;
            case "field":
              kind = monaco.languages.CompletionItemKind.Field;
              break;
            case "class":
              kind = monaco.languages.CompletionItemKind.Class;
              break;
          }
          return {
            label: i.label,
            kind: kind,
            insertText: i.label,
            sortText: i.sortText,
            detail: i.detail,
            additionalTextEdits: i.additionalTextEdits?.map((edit) => {
              const range = edit.range;
              const textEdit: monaco.languages.TextEdit = {
                range: lsRangeToMonacoRange(range),
                text: edit.newText,
              };
              return textEdit;
            }),
            range: undefined,
          };
        }),
      };
    },
    // Trigger characters should be kept in sync with the ones in `vscode/src/extension.ts`
    triggerCharacters: ["@", "."],
  });

  monaco.languages.registerHoverProvider("qsharp", {
    provideHover: async (
      model: monaco.editor.ITextModel,
      position: monaco.Position,
    ) => {
      const hover = await languageService.getHover(
        model.uri.toString(),
        monacoPositionToLsPosition(position),
      );

      if (hover) {
        return {
          contents: [{ value: hover.contents }],
          range: lsRangeToMonacoRange(hover.span),
        };
      }
      return null;
    },
  });

  monaco.languages.registerDefinitionProvider(qdkLanguages, {
    provideDefinition: async (
      model: monaco.editor.ITextModel,
      position: monaco.Position,
    ) => {
      const definition = await languageService.getDefinition(
        model.uri.toString(),
        monacoPositionToLsPosition(position),
      );
      if (!definition) return null;
      const uri = monaco.Uri.parse(definition.source);
      if (uri.toString() !== model.uri.toString()) return null;
      return {
        uri,
        range: lsRangeToMonacoRange(definition.span),
      };
    },
  });

  monaco.languages.registerReferenceProvider(qdkLanguages, {
    provideReferences: async (
      model: monaco.editor.ITextModel,
      position: monaco.Position,
      context: monaco.languages.ReferenceContext,
    ) => {
      const lsReferences = await languageService.getReferences(
        model.uri.toString(),
        monacoPositionToLsPosition(position),
        context.includeDeclaration,
      );
      if (!lsReferences) return [];
      const references: monaco.languages.Location[] = [];
      for (const reference of lsReferences) {
        const uri = monaco.Uri.parse(reference.source);
        // the playground doesn't support sources other than the current source
        if (uri.toString() == model.uri.toString()) {
          references.push({
            uri,
            range: lsRangeToMonacoRange(reference.span),
          });
        }
      }
      return references;
    },
  });

  monaco.languages.registerSignatureHelpProvider("qsharp", {
    signatureHelpTriggerCharacters: ["(", ","],
    provideSignatureHelp: async (
      model: monaco.editor.ITextModel,
      position: monaco.Position,
    ) => {
      const sigHelpLs = await languageService.getSignatureHelp(
        model.uri.toString(),
        monacoPositionToLsPosition(position),
      );
      if (!sigHelpLs) return null;
      return {
        dispose: () => {},
        value: {
          activeParameter: sigHelpLs.activeParameter,
          activeSignature: sigHelpLs.activeSignature,
          signatures: sigHelpLs.signatures.map((sig) => {
            return {
              label: sig.label,
              documentation: {
                value: sig.documentation,
              } as monaco.IMarkdownString,
              parameters: sig.parameters.map((param) => {
                return {
                  label: param.label,
                  documentation: {
                    value: param.documentation,
                  } as monaco.IMarkdownString,
                };
              }),
            };
          }),
        },
      };
    },
  });

  monaco.languages.registerRenameProvider(qdkLanguages, {
    provideRenameEdits: async (
      model: monaco.editor.ITextModel,
      position: monaco.Position,
      newName: string,
    ) => {
      const rename = await languageService.getRename(
        model.uri.toString(),
        monacoPositionToLsPosition(position),
        newName,
      );
      if (!rename) return null;
      return lsToMonacoWorkspaceEdit(rename);
    },
    resolveRenameLocation: async (
      model: monaco.editor.ITextModel,
      position: monaco.Position,
    ) => {
      const prepareRename = await languageService.prepareRename(
        model.uri.toString(),
        monacoPositionToLsPosition(position),
      );
      if (prepareRename) {
        return {
          range: lsRangeToMonacoRange(prepareRename.range),
          text: prepareRename.newText,
        } as monaco.languages.RenameLocation;
      } else {
        return {
          rejectReason: "Rename is unavailable at this location.",
        } as monaco.languages.RenameLocation & monaco.languages.Rejection;
      }
    },
  });

  async function getFormatChanges(
    model: monaco.editor.ITextModel,
    range?: monaco.Range,
  ) {
    const lsEdits = await languageService.getFormatChanges(
      model.uri.toString(),
    );
    if (!lsEdits) {
      return [];
    }
    let edits = lsEdits.map((edit) => {
      return {
        range: lsRangeToMonacoRange(edit.range),
        text: edit.newText,
      } as monaco.languages.TextEdit;
    });
    if (range) {
      edits = edits.filter((e) => monaco.Range.areIntersecting(range, e.range));
    }
    return edits;
  }

  monaco.languages.registerDocumentFormattingEditProvider("qsharp", {
    provideDocumentFormattingEdits: async (model: monaco.editor.ITextModel) => {
      return getFormatChanges(model);
    },
  });

  monaco.languages.registerDocumentRangeFormattingEditProvider("qsharp", {
    provideDocumentRangeFormattingEdits: async (
      model: monaco.editor.ITextModel,
      range: monaco.Range,
    ) => {
      return getFormatChanges(model, range);
    },
  });

  monaco.languages.registerCodeActionProvider("qsharp", {
    provideCodeActions: async (
      model: monaco.editor.ITextModel,
      range: monaco.Range,
    ) => {
      const lsCodeActions = await languageService.getCodeActions(
        model.uri.toString(),
        monacoRangetoLsRange(range),
      );

      const codeActions = lsCodeActions.map((lsCodeAction) => {
        let edit;
        if (lsCodeAction.edit) {
          edit = lsToMonacoWorkspaceEdit(lsCodeAction.edit);
        }

        return {
          title: lsCodeAction.title,
          edit: edit,
          kind: lsCodeAction.kind,
          isPreferred: lsCodeAction.isPreferred,
        } as monaco.languages.CodeAction;
      });

      return {
        actions: codeActions,
        dispose: () => {},
      } as monaco.languages.CodeActionList;
    },
  });
}

// Monaco provides the 'require' global for loading modules.
declare const require: {
  config: (settings: object) => void;
  (base: string[], onready: () => void): void;
};
require.config({ paths: { vs: monacoPath } });
require(["vs/editor/editor.main"], loaded);
