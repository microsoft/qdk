// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { MajoranaGeometry } from "./geometry.js";
import type { OperationProjection } from "./operations.js";
import { Qubits } from "./qubits.js";
import { Tetrons } from "./tetrons.js";
import { Virtual, type VirtualOperationRenderState } from "./virtual.js";
import { MajoranaControls, type MajoranaControlsProps } from "./controls.js";
import {
  MAJORANA_VIEWS,
  type MajoranaPresentationOptions,
  type MajoranaTransitionPhase,
  type MajoranaView,
} from "./types.js";
import type { MajoranaTopology } from "./topology.js";
import type { VirtualGeometry } from "./virtual-geometry.js";
import type { VirtualOperationProjection } from "./virtual-operations.js";
import type { VirtualTopology } from "./virtual-topology.js";

export type MajoranaSceneProps = {
  topology: MajoranaTopology | undefined;
  geometry: MajoranaGeometry | undefined;
  virtualTopology: VirtualTopology | undefined;
  virtualGeometry: VirtualGeometry | undefined;
  virtualOperations: readonly VirtualOperationRenderState[];
  operations: readonly OperationProjection[];
  presentation: Required<MajoranaPresentationOptions>;
  view: MajoranaView;
  zoom: number;
  availableWidth: number | undefined;
  step: number;
  selectedStep: number;
  transitionPhase: MajoranaTransitionPhase;
  error: Error | undefined;
  controls: Omit<
    MajoranaControlsProps,
    "view" | "availableViews" | "step" | "showQubitLabels" | "showMzmLabels"
  >;
};

export function MajoranaScene({
  topology,
  geometry,
  virtualTopology,
  virtualGeometry,
  virtualOperations,
  operations,
  presentation,
  view,
  zoom,
  availableWidth,
  step,
  selectedStep,
  transitionPhase,
  error,
  controls,
}: MajoranaSceneProps) {
  if (topology === undefined || geometry === undefined) {
    return (
      <div class="qs-majorana">{error && <ErrorPanel error={error} />}</div>
    );
  }

  const availableViews =
    virtualTopology === undefined
      ? MAJORANA_VIEWS.filter((option) => option !== "Virtual")
      : MAJORANA_VIEWS;
  const componentChromeWidth = 32;
  const minimumComponentWidth = 1000;
  const naturalComponentWidth =
    getMinimumSceneWidth(geometry) + componentChromeWidth;
  const fittedComponentWidth =
    availableWidth === undefined
      ? Math.max(minimumComponentWidth, naturalComponentWidth)
      : Math.min(
          Math.max(minimumComponentWidth, naturalComponentWidth),
          availableWidth,
        );
  const fittedSceneWidth = Math.max(
    0,
    fittedComponentWidth - componentChromeWidth,
  );
  const sceneWidth = fittedSceneWidth * zoom;
  const desiredComponentWidth = Math.max(
    fittedComponentWidth,
    sceneWidth + componentChromeWidth,
  );
  const componentWidth =
    availableWidth === undefined
      ? desiredComponentWidth
      : Math.min(desiredComponentWidth, availableWidth);
  return (
    <div class="qs-majorana" style={{ width: `${componentWidth}px` }}>
      <MajoranaControls
        {...controls}
        view={view}
        availableViews={availableViews}
        step={selectedStep}
        maximumStep={controls.maximumStep}
        showQubitLabels={presentation.showQubitLabels}
        showMzmLabels={presentation.showMzmLabels}
      />
      <div class="qs-majorana-scroller">
        <svg
          class="qs-majorana-scene"
          style={{ width: `${sceneWidth}px` }}
          viewBox={`${geometry.viewBox.x} ${geometry.viewBox.y} ${geometry.viewBox.width} ${geometry.viewBox.height}`}
          aria-label={
            view === "Virtual" && virtualTopology !== undefined
              ? `Virtual view at visualization step ${step} with ${virtualTopology.qubits.length} virtual qubits`
              : `${view} view at visualization step ${step} with ${topology.device.qubitCount} qubits`
          }
          data-step={step}
          data-selected-step={selectedStep}
          data-view={view}
          data-zoom={zoom}
          data-transition={transitionPhase}
          role="img"
        >
          <title>Majorana qubit visualization</title>
          {view === "Tetrons" ? (
            <Tetrons
              topology={topology}
              geometry={geometry}
              operations={operations}
              showQubitLabels={presentation.showQubitLabels}
              showMzmLabels={presentation.showMzmLabels}
              transitionPhase={transitionPhase}
            />
          ) : view === "Qubits" ? (
            <Qubits
              topology={topology}
              geometry={geometry}
              operations={operations}
              showQubitLabels={presentation.showQubitLabels}
              transitionPhase={transitionPhase}
            />
          ) : virtualTopology !== undefined && virtualGeometry !== undefined ? (
            <Virtual
              topology={virtualTopology}
              geometry={virtualGeometry}
              operations={virtualOperations}
              showQubitLabels={presentation.showQubitLabels}
            />
          ) : (
            <title>Virtual view is unavailable</title>
          )}
        </svg>
      </div>
      {error && <ErrorPanel error={error} />}
    </div>
  );
}

function ErrorPanel({ error }: { error: Error }) {
  return (
    <div class="qs-majorana-error" role="alert">
      <strong>Unable to update the Majorana visualization.</strong>
      <span>{error.message}</span>
    </div>
  );
}

export function getMinimumSceneWidth(geometry: MajoranaGeometry): number {
  return Math.max(520, Math.ceil(geometry.viewBox.width * 0.75));
}
