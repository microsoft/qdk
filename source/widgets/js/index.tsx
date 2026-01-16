// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { render as prender } from "preact";
import {
  ReTable,
  SpaceChart,
  Histogram,
  CreateSingleEstimateResult,
  EstimatesOverview,
  EstimatesPanel,
  ReData,
  Circuit,
  setRenderer,
} from "qsharp-lang/ux";
import markdownIt from "markdown-it";
import "./widgets.css";

// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore - there are no types for this
import mk from "@vscode/markdown-it-katex";

const md = markdownIt();
md.use(mk);
setRenderer((input: string) => md.render(input));

export function mdRenderer(input: string) {
  // Note: Need to ensure this 'fix' is still needed with the latest data JSON.
  // In early testing backslashes were being double-escaped in the results.
  return md.render(input.replace(/\\\\/g, "\\"));
}

// Param types for AnyWidget render functions
import type { AnyModel } from "@anywidget/types";

type RenderArgs = {
  model: AnyModel;
  el: HTMLElement;
};

function getWidgetHost(el: HTMLElement): HTMLElement {
  const root = el.getRootNode();
  if (root instanceof ShadowRoot && root.host instanceof HTMLElement) {
    return root.host;
  }

  return el;
}

function computeColorScheme(doc: Document): "dark" | "light" {
  const body = doc.body;

  // VS Code webviews
  const vscodeKind = body?.getAttribute("data-vscode-theme-kind");
  if (vscodeKind === "vscode-dark" || vscodeKind === "vscode-high-contrast") {
    return "dark";
  }
  if (
    vscodeKind === "vscode-light" ||
    vscodeKind === "vscode-high-contrast-light"
  ) {
    return "light";
  }

  // JupyterLab / Notebook 7
  if (body?.classList.contains("jp-mod-theme-dark")) {
    return "dark";
  }
  if (body?.classList.contains("jp-mod-theme-light")) {
    return "light";
  }

  // Some JupyterLab themes expose data-jp-theme-light="true|false".
  const jpThemeLight = body?.getAttribute("data-jp-theme-light");
  if (jpThemeLight === "false") {
    return "dark";
  }
  if (jpThemeLight === "true") {
    return "light";
  }

  // Fallback: OS/browser preference.
  return doc.defaultView?.matchMedia?.("(prefers-color-scheme: dark)")?.matches
    ? "dark"
    : "light";
}

const themeObserverByDocument = new WeakMap<Document, MutationObserver>();
const themeHostsByDocument = new WeakMap<Document, Set<HTMLElement>>();

function applyAndObserveTheme(el: HTMLElement) {
  const doc = el.ownerDocument;
  const host = getWidgetHost(el);

  let hosts = themeHostsByDocument.get(doc);
  if (!hosts) {
    hosts = new Set<HTMLElement>();
    themeHostsByDocument.set(doc, hosts);
  }
  hosts.add(host);

  const apply = () => {
    const scheme = computeColorScheme(doc);
    if (scheme === "dark") {
      host.setAttribute("data-qs-color-scheme", "dark");
    } else {
      host.removeAttribute("data-qs-color-scheme");
    }
  };

  apply();

  // Ensure we only register one observer per document.
  if (!themeObserverByDocument.has(doc)) {
    const observer = new MutationObserver(() => {
      const currentHosts = themeHostsByDocument.get(doc);
      if (!currentHosts) return;

      const scheme = computeColorScheme(doc);

      // Apply to all known hosts; drop any that are no longer connected.
      for (const h of currentHosts) {
        if (!h.isConnected) {
          currentHosts.delete(h);
          continue;
        }

        if (scheme === "dark") {
          h.setAttribute("data-qs-color-scheme", "dark");
        } else {
          h.removeAttribute("data-qs-color-scheme");
        }
      }
    });

    // Observe theme changes on the body (VS Code and JupyterLab mutate class/attrs).
    if (doc.body) {
      observer.observe(doc.body, {
        attributes: true,
        attributeFilter: [
          "class",
          "data-vscode-theme-kind",
          "data-jp-theme-light",
          "data-jp-theme-name",
        ],
      });
    }

    themeObserverByDocument.set(doc, observer);
  }
}

function render({ model, el }: RenderArgs) {
  const componentType = model.get("comp");

  // Set a stable color-scheme marker for CSS (works in Jupyter-only and VS Code).
  // This is especially important when AnyWidget uses Shadow DOM, where selectors
  // like body.jp-mod-theme-dark won't match inside the widget stylesheet.
  applyAndObserveTheme(el);

  // There is an existing issue where in VS Code it always shows the widget background as white.
  // (See https://github.com/microsoft/vscode-jupyter/issues/7161)
  // We tried to fix this in CSS by overriding the style, but there is a race condition whereby
  // depending on which style gets injected first (ours or ipywidgets), it may or may not work.

  // The solution here is to force our own override to be last in the style list if not already.
  // It's a bit of a hack, but it works, and I couldn't find something better that wouldn't be fragile.

  if (
    !el.ownerDocument.head.lastChild?.textContent?.includes("widget-css-fix")
  ) {
    const forceStyle = el.ownerDocument.createElement("style");
    forceStyle.textContent = `/* widget-css-fix */ .cell-output-ipywidget-background {background-color: transparent !important;}`;
    el.ownerDocument.head.appendChild(forceStyle);
  }

  switch (componentType) {
    case "SpaceChart":
      renderChart({ model, el });
      break;
    case "EstimatesOverview":
      renderEstimatesOverview({ model, el });
      break;
    case "EstimateDetails":
      renderTable({ model, el });
      break;
    case "Histogram":
      renderHistogram({ model, el });
      break;
    case "EstimatesPanel":
      renderEstimatesPanel({ model, el });
      break;
    case "Circuit":
      renderCircuit({ model, el });
      break;
    default:
      throw new Error(`Unknown component type ${componentType}`);
  }
}

export default {
  render,
};

function renderTable({ model, el }: RenderArgs) {
  const onChange = () => {
    const estimates = model.get("estimates");
    const index = model.get("index");
    const singleEstimateResult = CreateSingleEstimateResult(estimates, index);
    prender(
      <ReTable
        estimatesData={singleEstimateResult}
        mdRenderer={mdRenderer}
      ></ReTable>,
      el,
    );
  };

  onChange();
  model.on("change:estimates", onChange);
  model.on("change:index", onChange);
}

function renderChart({ model, el }: RenderArgs) {
  const onChange = () => {
    const estimates = model.get("estimates");
    const index = model.get("index");
    const singleEstimateResult = CreateSingleEstimateResult(estimates, index);
    prender(<SpaceChart estimatesData={singleEstimateResult}></SpaceChart>, el);
  };

  onChange();
  model.on("change:estimates", onChange);
  model.on("change:index", onChange);
}

function renderEstimatesOverview({ model, el }: RenderArgs) {
  const onChange = () => {
    const results = model.get("estimates");
    const colors = model.get("colors");
    const runNames = model.get("runNames");

    let estimates = [];
    if (results[0] == null) {
      estimates.push(results);
    } else {
      for (const estimate of Object.values(results)) {
        estimates.push(estimate);
      }
    }

    const onRowDeleted = createOnRowDeleted(estimates, (newEstimates) => {
      estimates = newEstimates;
      model.set("estimates", estimates);
    });

    prender(
      <EstimatesOverview
        estimatesData={estimates}
        runNames={runNames}
        colors={colors}
        isSimplifiedView={true}
        onRowDeleted={onRowDeleted}
        setEstimate={() => undefined}
        allowSaveImage={true}
      ></EstimatesOverview>,
      el,
    );
  };

  onChange();
  model.on("change:estimates", onChange);
  model.on("change:colors", onChange);
  model.on("change:runNames", onChange);
}

function renderEstimatesPanel({ model, el }: RenderArgs) {
  const onChange = () => {
    const results = model.get("estimates");
    const colors = model.get("colors");
    const runNames = model.get("runNames");

    let estimates: ReData[] = [];
    if (results[0] == null) {
      estimates.push(results);
    } else {
      for (const estimate of Object.values(results)) {
        estimates.push(estimate as ReData);
      }
    }

    const onRowDeleted = createOnRowDeleted(estimates, (newEstimates) => {
      estimates = newEstimates;
      model.set("estimates", estimates);
    });

    prender(
      <EstimatesPanel
        estimatesData={estimates}
        runNames={runNames}
        colors={colors}
        renderer={mdRenderer}
        calculating={false}
        onRowDeleted={onRowDeleted}
        allowSaveImage={true}
      ></EstimatesPanel>,
      el,
    );
  };

  onChange();
  model.on("change:estimates", onChange);
  model.on("change:colors", onChange);
  model.on("change:runNames", onChange);
}

function createOnRowDeleted(
  estimates: ReData[],
  setEstimates: (estimates: ReData[]) => void,
) {
  return (rowId: string) => {
    // Clone estimates into a new object
    const newEstimates = JSON.parse(JSON.stringify(estimates)) as ReData[];

    // Splice out the estimate that was deleted
    const index = newEstimates.findIndex(
      (estimate) => estimate.jobParams.runName === rowId,
    );
    if (index >= 0) {
      newEstimates.splice(index, 1);
    }

    setEstimates(newEstimates);
  };
}

function renderHistogram({ model, el }: RenderArgs) {
  const onChange = () => {
    const buckets = model.get("buckets") as { [key: string]: number };
    const bucketMap = new Map(Object.entries(buckets));
    const shot_count = model.get("shot_count") as number;
    const shot_header = model.get("shot_header") as boolean;
    const labels = model.get("labels") as "raw" | "kets" | "none";
    const items = model.get("items") as "all" | "top-10" | "top-25";
    const sort = model.get("sort") as "a-to-z" | "high-to-low" | "low-to-high";

    prender(
      <Histogram
        data={bucketMap}
        shotCount={shot_count}
        filter={""}
        onFilter={() => undefined}
        shotsHeader={shot_header}
        labels={labels}
        items={items}
        sort={sort}
      ></Histogram>,
      el,
    );
  };

  onChange();
  model.on("change:buckets", onChange);
  model.on("change:shot_count", onChange);
  model.on("change:shot_header", onChange);
  model.on("change:labels", onChange);
  model.on("change:items", onChange);
  model.on("change:sort", onChange);
}

function renderCircuit({ model, el }: RenderArgs) {
  const onChange = () => {
    const circuitJson = model.get("circuit_json") as string;
    prender(
      <Circuit
        circuit={JSON.parse(circuitJson)}
        isEditable={false}
        renderLocations={(locations) => {
          return {
            title: locations
              .map((loc) => `${loc.file}:${loc.line}:${loc.column}`)
              .join("\n"),
            href: "#",
          };
        }}
      ></Circuit>,
      el,
    );
  };

  onChange();
  model.on("change:circuit_json", onChange);
}
