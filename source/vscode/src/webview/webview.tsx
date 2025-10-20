// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// <reference types="@types/vscode-webview"/>

const vscodeApi = acquireVsCodeApi();

import { render } from "preact";
import {
  CircuitPanel,
  CircuitProps,
  EstimatesPanel,
  Histogram,
  setRenderer,
  type ReData,
} from "qsharp-lang/ux";
import { HelpPage } from "./help";
import { DocumentationView, IDocFile } from "./docview";
import "./webview.css";

// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore - there are no types for this
import mk from "@vscode/markdown-it-katex";
import markdownIt from "markdown-it";
import { setThemeStylesheet } from "./theme";
const md = markdownIt("commonmark");
md.use(mk, {
  enableMathBlockInHtml: true,
  enableMathInlineInHtml: true,
});
setRenderer((input: string) => md.render(input));

window.addEventListener("message", onMessage);
window.addEventListener("load", main);

type HistogramState = {
  viewType: "histogram";
  panelId: string;
  buckets: Array<[string, number]>;
  shotCount: number;
  suppressSettings?: boolean;
};

type EstimatesState = {
  viewType: "estimates";
  estimatesData: {
    calculating: boolean;
    estimates: ReData[];
  };
};

type CircuitState = {
  viewType: "circuit";
  panelId: string;
  props: CircuitProps;
};

type DocumentationState = {
  viewType: "documentation";
  fragmentsToRender: IDocFile[];
  projectName: string;
};

type State =
  | { viewType: "loading"; panelId: string }
  | { viewType: "help" }
  | HistogramState
  | EstimatesState
  | CircuitState
  | DocumentationState;
const loadingState: State = { viewType: "loading", panelId: "" };
const helpState: State = { viewType: "help" };
let state: State = loadingState;

function main() {
  state = (vscodeApi.getState() as any) || loadingState;
  render(<App state={state} />, document.body);
  setThemeStylesheet();
  vscodeApi.postMessage({ command: "ready" });
}

function onMessage(event: any) {
  const message = event.data;
  if (!message?.command) {
    console.error("Unknown message: ", message);
    return;
  }
  switch (message.command) {
    case "histogram": {
      if (!message.buckets || typeof message.shotCount !== "number") {
        console.error("No buckets in message: ", message);
        return;
      }
      state = {
        viewType: "histogram",
        panelId: message.panelId,
        buckets: message.buckets as Array<[string, number]>,
        shotCount: message.shotCount,
        suppressSettings: message.suppressSettings,
      };
      break;
    }
    case "estimates":
      {
        const newState: EstimatesState = {
          viewType: "estimates",
          estimatesData: {
            calculating: !!message.calculating,
            estimates: [],
          },
        };
        // Copy over any existing estimates
        if ((state as EstimatesState).estimatesData?.estimates) {
          newState.estimatesData.estimates.push(
            ...(state as EstimatesState).estimatesData.estimates,
          );
        }
        // Append any new estimates
        if (message.estimates) {
          if (Array.isArray(message.estimates)) {
            newState.estimatesData.estimates.push(...message.estimates);
          } else {
            newState.estimatesData.estimates.push(message.estimates);
          }
        }
        state = newState;
      }
      break;
    case "help":
      state = helpState;
      break;
    case "circuit":
      {
        state = {
          viewType: "circuit",
          ...message,
        };
      }
      break;
    case "showDocumentationCommand":
      {
        state = {
          viewType: "documentation",
          fragmentsToRender: message.fragmentsToRender,
          projectName: message.projectName,
        };
      }
      break;
    default:
      console.error("Unknown command: ", message.command);
      return;
  }

  vscodeApi.setState(state);
  render(<App state={state} />, document.body);
}

function onRowDeleted(rowId: string) {
  // Clone all the state to a new object
  const newState: State = JSON.parse(JSON.stringify(state));

  // Splice out the estimate that was deleted
  const estimates = (newState as EstimatesState).estimatesData.estimates;
  const index = estimates.findIndex(
    (estimate) => estimate.jobParams.runName === rowId,
  );
  if (index >= 0) {
    estimates.splice(index, 1);
  }
  state = newState;

  vscodeApi.setState(state);
  render(<App state={state} />, document.body);
}

function App({ state }: { state: State }) {
  const onFilter = () => undefined;

  switch (state.viewType) {
    case "loading":
      return <div>Loading...</div>;
    case "histogram":
      return (
        <>
          <Histogram
            data={new Map(state.buckets)}
            shotCount={state.shotCount}
            filter=""
            onFilter={onFilter}
            shotsHeader={true}
          ></Histogram>
          {state.suppressSettings ? null : (
            <p style="margin-top: 8px; font-size: 0.8em">
              Note: If a{" "}
              <a href="vscode://settings/Q%23.simulation.pauliNoise">
                noise model
              </a>{" "}
              or any{" "}
              <a href="vscode://settings/Q%23.simulation.qubitLoss">
                qubit loss
              </a>{" "}
              has been configured, this may impact results
            </p>
          )}
        </>
      );
    case "estimates":
      return (
        <EstimatesPanel
          calculating={state.estimatesData.calculating}
          estimatesData={state.estimatesData.estimates}
          onRowDeleted={onRowDeleted}
          colors={[]}
          runNames={[]}
        />
      );
    case "circuit":
      return <CircuitPanel {...state.props}></CircuitPanel>;
    case "help":
      return <HelpPage />;
    case "documentation":
      // Ideally we'd have this on all web views, but it makes the font a little
      // too large in the others right now. Something to unify later.
      document.body.classList.add("markdown-body");
      document.body.style.fontSize = "0.8em";
      return (
        <DocumentationView
          fragmentsToRender={state.fragmentsToRender}
          projectName={state.projectName}
        />
      );
    default:
      console.error("Unknown view type in state", state);
      return <div>Loading error</div>;
  }
}
