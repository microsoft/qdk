// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { MajoranaGeometry } from "./geometry.js";
import type { OperationProjection } from "./operations.js";
import { Qubits } from "./qubits.js";
import { Tetrons } from "./tetrons.js";
import { MajoranaControls, type MajoranaControlsProps } from "./controls.js";
import {
  type MajoranaPresentationOptions,
  type MajoranaTransitionPhase,
  type MajoranaView,
} from "./types.js";
import type { MajoranaTopology } from "./topology.js";

export type MajoranaSceneProps = {
  topology: MajoranaTopology | undefined;
  geometry: MajoranaGeometry | undefined;
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
    "view" | "step" | "showQubitLabels" | "showMzmLabels"
  >;
};

export function MajoranaScene({
  topology,
  geometry,
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
          aria-label={`${view} view at visualization step ${step} with ${topology.device.qubitCount} qubits`}
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
          ) : (
            <Qubits
              topology={topology}
              geometry={geometry}
              operations={operations}
              showQubitLabels={presentation.showQubitLabels}
              transitionPhase={transitionPhase}
            />
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
