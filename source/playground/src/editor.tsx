// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// <reference types="../../../node_modules/monaco-editor/monaco.d.ts"/>

import { useEffect, useRef, useState } from "preact/hooks";
import {
  CompilerState,
  ICompilerWorker,
  ILanguageServiceWorker,
  QscEventTarget,
  VSDiagnostic,
  log,
  ProgramConfig,
  LanguageServiceDiagnosticEvent,
  getTargetProfileFromEntryPoint,
} from "qsharp-lang";
import { Exercise, getExerciseSources } from "qsharp-lang/katas-md";
import { codeToCompressedBase64, lsRangeToMonacoRange } from "./utils.js";
import { ActiveTab } from "./main.js";

import type { KataSection } from "qsharp-lang/katas";

type ErrCollection = {
  checkDiags: VSDiagnostic[];
  shotDiags: VSDiagnostic[];
};

function VSDiagsToMarkers(errors: VSDiagnostic[]): monaco.editor.IMarkerData[] {
  return errors.map((err) => {
    let severity = monaco.MarkerSeverity.Error;
    switch (err.severity) {
      case "error":
        severity = monaco.MarkerSeverity.Error;
        break;
      case "warning":
        severity = monaco.MarkerSeverity.Warning;
        break;
      case "info":
        severity = monaco.MarkerSeverity.Info;
        break;
    }

    const marker: monaco.editor.IMarkerData = {
      ...lsRangeToMonacoRange(err.range),
      severity,
      message: err.message,
      relatedInformation: err.related?.map((r) => {
        const range = lsRangeToMonacoRange(r.location.span);
        return {
          resource: monaco.Uri.parse(r.location.source),
          message: r.message,
          ...range,
        };
      }),
    };

    if (err.uri && err.code) {
      marker.code = {
        value: err.code,
        target: monaco.Uri.parse(err.uri),
      };
    } else if (err.code) {
      marker.code = err.code;
    }

    return marker;
  });
}

export function Editor(props: {
  code: string;
  compiler: ICompilerWorker;
  compiler_worker_factory: () => ICompilerWorker;
  compilerState: CompilerState;
  defaultShots: number;
  evtTarget: QscEventTarget;
  kataSection?: KataSection;
  onRestartCompiler: () => void;
  shotError?: VSDiagnostic;
  showExpr: boolean;
  showShots: boolean;
  setAst: (ast: string) => void;
  setHir: (hir: string) => void;
  setRir: (rir: string[]) => void;
  setQir: (qir: string) => void;
  activeTab: ActiveTab;
  languageService: ILanguageServiceWorker;
  language?: "qsharp" | "openqasm";
}) {
  // The editor is remounted (keyed on language in the parent) whenever the
  // language changes, so this is stable for the lifetime of the component.
  const language = props.language ?? "qsharp";
  const fileName = language === "openqasm" ? "main.qasm" : "main.qs";

  const editor = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const errMarks = useRef<ErrCollection>({ checkDiags: [], shotDiags: [] });
  const editorDiv = useRef<HTMLDivElement>(null);

  // Maintain a ref to the latest getAst/getHir functions, as it closes over a bunch of stuff
  const irRef = useRef(async () => {
    return;
  });
  const [shotCount, setShotCount] = useState(props.defaultShots);
  const [runExpr, setRunExpr] = useState("");
  const [errors, setErrors] = useState<
    { location: string; severity: monaco.MarkerSeverity; msg: string[] }[]
  >([]);
  const [hasCheckErrors, setHasCheckErrors] = useState(false);

  function markErrors() {
    const model = editor.current?.getModel();
    if (!model) return;

    const errs = [
      ...errMarks.current.checkDiags,
      ...errMarks.current.shotDiags,
    ];

    const markers = VSDiagsToMarkers(errs);
    monaco.editor.setModelMarkers(model, "qsharp", markers);

    const errList = markers.map((err) => ({
      location: `${fileName}@(${err.startLineNumber},${err.startColumn})`,
      severity: err.severity,
      msg: err.message.split("\n\n"),
    }));
    setErrors(errList);
  }

  irRef.current = async function updateIr() {
    const code = editor.current?.getValue();
    if (code == null) return;

    const config = {
      sources: [["code", code]] as [string, string][],
      languageFeatures: [],
      profile:
        (language === "openqasm"
          ? undefined
          : await getTargetProfileFromEntryPoint("main.qs", code)) ||
        "adaptive_rif", // Default to adaptive_rif for qir and rir generation
      projectType: language,
    };

    if (props.activeTab === "ast-tab") {
      props.setAst(await props.compiler.getAst(config));
      return;
    }
    if (props.activeTab === "hir-tab") {
      props.setHir(await props.compiler.getHir(config));
      return;
    }

    if (props.activeTab === "qir-tab" || props.activeTab === "rir-tab") {
      // OpenQASM codegen runs through the interpreter and is slower than Q#'s
      // direct path, so allow more time before terminating the worker.
      const codeGenTimeout = language === "openqasm" ? 10000 : 1000; // ms
      let timedOut = false;
      const compiler = props.compiler_worker_factory();
      const compilerTimeout = setTimeout(() => {
        log.info("Compiler timeout. Terminating worker.");
        timedOut = true;
        compiler.terminate();
      }, codeGenTimeout);
      try {
        if (props.activeTab === "rir-tab") {
          const ir = await compiler.getRir(config);
          clearTimeout(compilerTimeout);
          props.setRir(ir);
        } else {
          const ir = await compiler.getQir(config);
          clearTimeout(compilerTimeout);
          props.setQir(ir);
        }
      } catch (e: any) {
        if (timedOut) {
          if (props.activeTab === "rir-tab") {
            props.setRir(["timed out", "timed out"]);
          } else {
            props.setQir("timed out");
          }
        } else {
          if (props.activeTab === "rir-tab") {
            props.setRir([e.toString(), e.toString()]);
          } else {
            props.setQir(e.toString());
          }
        }
      } finally {
        compiler.terminate();
      }
    }
  };

  async function onRun() {
    const code = editor.current?.getValue();
    if (code == null) return;
    props.evtTarget.clearResults();
    const config = {
      sources: [["code", code]],
      languageFeatures: [],
      // The entry-point profile annotation is Q#-specific; skip it for OpenQASM.
      profile:
        language === "openqasm"
          ? undefined
          : await getTargetProfileFromEntryPoint("main.qs", code),
      projectType: language,
    } as ProgramConfig;

    try {
      if (props.kataSection?.type === "exercise") {
        // This is for a kata exercise. Provide the sources that implement the solution verification.
        const sources = await getExerciseSources(props.kataSection as Exercise);
        // check uses the unrestricted profile and doesn't do code gen,
        // so we just pass the sources
        await props.compiler.checkExerciseSolution(
          code,
          sources,
          props.evtTarget,
        );
      } else {
        performance.mark("compiler-run-start");
        await props.compiler.run(config, runExpr, shotCount, props.evtTarget);
        const runTimer = performance.measure(
          "compiler-run",
          "compiler-run-start",
        );
        log.logTelemetry({
          id: "compiler-run",
          data: {
            duration: runTimer.duration,
            codeSize: code.length,
            shotCount,
          },
        });
      }
    } catch (err) {
      // This could fail for several reasons, e.g. the run being cancelled.
      if (err === "terminated") {
        log.info("Run was terminated");
      } else {
        log.error("Run failed with error: %o", err);
      }
    }
  }

  useEffect(() => {
    if (!editorDiv.current) return;
    const newEditor = monaco.editor.create(editorDiv.current, {
      minimap: { enabled: false },
      lineNumbersMinChars: 3,
      automaticLayout: true,
    });

    editor.current = newEditor;
    const srcModel =
      monaco.editor.getModel(
        monaco.Uri.parse(props.kataSection?.id ?? fileName),
      ) ??
      monaco.editor.createModel(
        "",
        language,
        monaco.Uri.parse(props.kataSection?.id ?? fileName),
      );
    srcModel.setValue(props.code);
    newEditor.setModel(srcModel);
    srcModel.onDidChangeContent(() => irRef.current());

    // TODO: If the language service ever changes, this callback
    // will be invalid as it captures the *original* props.languageService
    // and not the updated one. Not a problem currently since the language
    // service is never updated, but not correct either.
    srcModel.onDidChangeContent(async () => {
      // Reset the shot errors whenever the document changes.
      // The markers will be refreshed by the onDiagnostics callback
      // when the language service finishes checking the document.
      errMarks.current.shotDiags = [];

      performance.mark("update-document-start");
      await props.languageService.updateDocument(
        srcModel.uri.toString(),
        srcModel.getVersionId(),
        srcModel.getValue(),
        language,
      );
      const measure = performance.measure(
        "update-document",
        "update-document-start",
      );
      log.info(`updateDocument took ${measure.duration}ms`);
    });

    function onResize() {
      newEditor.layout();
    }

    // If the browser window resizes, tell the editor to update it's layout
    window.addEventListener("resize", onResize);
    return () => {
      log.info("Disposing a monaco editor");
      window.removeEventListener("resize", onResize);
      props.languageService.closeDocument(srcModel.uri.toString(), language);
      newEditor.dispose();
    };
  }, []);

  useEffect(() => {
    props.languageService.updateConfiguration({
      packageType: props.kataSection ? "lib" : "exe",
      lints: props.kataSection
        ? []
        : [{ lint: "needlessOperation", level: "warn" }],
    });

    function onDiagnostics(evt: LanguageServiceDiagnosticEvent) {
      const diagnostics = evt.detail.diagnostics;
      errMarks.current.checkDiags = diagnostics;
      markErrors();
      setHasCheckErrors(
        diagnostics.filter((d) => d.severity === "error").length > 0,
      );
    }

    props.languageService.addEventListener("diagnostics", onDiagnostics);

    return () => {
      log.info("Removing diagnostics listener");
      props.languageService.removeEventListener("diagnostics", onDiagnostics);
    };
  }, [props.languageService, props.kataSection]);

  useEffect(() => {
    const theEditor = editor.current;
    if (!theEditor) return;

    theEditor.getModel()?.setValue(props.code);
    theEditor.revealLineNearTop(1);
    setShotCount(props.defaultShots);
    setRunExpr("");
  }, [props.code, props.defaultShots]);

  useEffect(() => {
    errMarks.current.shotDiags = props.shotError ? [props.shotError] : [];
    markErrors();
  }, [props.shotError]);

  useEffect(() => {
    // Whenever the active tab changes, run check again.
    irRef.current();
  }, [props.activeTab]);

  // On reset, reload the initial code
  function onReset() {
    const theEditor = editor.current;
    if (!theEditor) return;
    theEditor.getModel()?.setValue(props.code || "");
    setShotCount(props.defaultShots);
    setRunExpr("");
  }

  async function onGetLink(ev: MouseEvent) {
    const code = editor.current?.getModel()?.getValue();
    if (!code) return;

    let messageText = "Unable to create the link";
    try {
      const encodedCode = await codeToCompressedBase64(code);
      const escapedCode = encodeURIComponent(encodedCode);
      // Update or add the current URL parameter 'code'
      const newURL = new URL(window.location.href);
      newURL.searchParams.set("code", escapedCode);

      // Copy link to clipboard and update url without reloading the page
      navigator.clipboard.writeText(newURL.toString());

      window.history.pushState({}, "", newURL.toString());
      messageText = "Link was copied to the clipboard";
    } finally {
      const popup = document.getElementById("popup") as HTMLDivElement;
      popup.style.display = "block";
      popup.innerText = messageText;
      popup.style.left = `${ev.clientX - 120}px`;
      popup.style.top = `${ev.clientY - 40}px`;

      setTimeout(() => {
        popup.style.display = "none";
      }, 2000);
    }
  }

  function shotCountChanged(e: Event) {
    const target = e.target as HTMLInputElement;
    setShotCount(parseInt(target.value) || 1);
  }

  function runExprChanged(e: Event) {
    const target = e.target as HTMLInputElement;
    setRunExpr(target.value);
  }

  return (
    <div class="editor-column">
      <div style="display: flex; justify-content: space-between; align-items: center;">
        <div class="file-name">{fileName}</div>
        <div class="icon-row">
          <svg
            onClick={onGetLink}
            width="24px"
            height="24px"
            viewBox="0 0 24 24"
            fill="none"
          >
            <title>Get a link to this code</title>
            <path
              d="M14 12C14 14.2091 12.2091 16 10 16H6C3.79086 16 2 14.2091 2 12C2 9.79086 3.79086 8 6 8H8M10 12C10 9.79086 11.7909 8 14 8H18C20.2091 8 22 9.79086 22 12C22 14.2091 20.2091 16 18 16H16"
              stroke="#000000"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
          <svg
            onClick={onReset}
            width="24px"
            height="24px"
            viewBox="0 0 24 24"
            fill="none"
          >
            <title>Reset code to initial state</title>
            <path
              d="M4,13 C4,17.4183 7.58172,21 12,21 C16.4183,21 20,17.4183 20,13 C20,8.58172 16.4183,5 12,5 C10.4407,5 8.98566,5.44609 7.75543,6.21762"
              stroke="#0C0310"
              stroke-width="2"
              stroke-linecap="round"
            ></path>
            <path
              d="M9.2384,1.89795 L7.49856,5.83917 C7.27552,6.34441 7.50429,6.9348 8.00954,7.15784 L11.9508,8.89768"
              stroke="#0C0310"
              stroke-width="2"
              stroke-linecap="round"
            ></path>
          </svg>
        </div>
      </div>
      <div class="code-editor" ref={editorDiv}></div>
      <div class="button-row">
        {props.showExpr ? (
          <>
            <span>Start</span>
            <input
              style="width: 160px"
              value={runExpr}
              onChange={runExprChanged}
            />
          </>
        ) : null}
        {props.showShots ? (
          <>
            <span>Shots</span>
            <input
              style="width: 88px;"
              type="number"
              value={shotCount || 100}
              max="1000"
              min="1"
              onChange={shotCountChanged}
            />
          </>
        ) : null}
        <button
          class="main-button"
          onClick={onRun}
          disabled={hasCheckErrors || props.compilerState === "busy"}
        >
          Run
        </button>
        <button
          class="main-button"
          onClick={props.onRestartCompiler}
          disabled={props.compilerState === "idle"}
        >
          Cancel
        </button>
      </div>
      <div class="diag-list">
        {errors.map((err) => (
          <div
            className={`diag-row ${err.severity === monaco.MarkerSeverity.Error ? "error-row" : "warning-row"}`}
          >
            <span>{err.location}: </span>
            <span>{err.msg[0]}</span>
            {err.msg.length > 1 ? (
              <div class="diag-help">{err.msg[1]}</div>
            ) : null}
          </div>
        ))}
      </div>
    </div>
  );
}
