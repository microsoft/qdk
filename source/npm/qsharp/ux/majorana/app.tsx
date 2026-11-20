// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { MajoranaLayout } from "./layout.js";
import type { OperationProjection } from "./operations.js";
import { Qubits } from "./qubits.js";
import { Tetrons } from "./tetrons.js";
import { MajoranaControls, type MajoranaControlsProps } from "./controls.js";
import {
  type MajoranaPresentationOptions,
  type MajoranaTransitionPhase,
} from "./types.js";

export type MajoranaViewProps = {
  layout: MajoranaLayout;
  operations: readonly OperationProjection[];
  presentation: Required<MajoranaPresentationOptions>;
  transitionPhase: MajoranaTransitionPhase;
};

export type MajoranaSceneProps = {
  layout: MajoranaLayout | undefined;
  operations: readonly OperationProjection[];
  zoom: number;
  availableWidth: number | undefined;
  step: number;
  transitionPhase: MajoranaTransitionPhase;
  error: Error | undefined;
  controls: MajoranaControlsProps;
};

export function MajoranaScene({
  layout,
  operations,
  zoom,
  availableWidth,
  step,
  transitionPhase,
  error,
  controls,
}: MajoranaSceneProps) {
  if (layout === undefined) {
    return (
      <div class="qs-majorana">{error && <ErrorPanel error={error} />}</div>
    );
  }

  const { view, step: selectedStep, presentation } = controls;
  const DeviceView = view === "Tetrons" ? Tetrons : Qubits;
  const viewProps: MajoranaViewProps = {
    layout,
    operations,
    presentation,
    transitionPhase,
  };
  const componentChromeWidth = 32;
  const minimumComponentWidth = 1000;
  const naturalComponentWidth =
    getMinimumSceneWidth(layout) + componentChromeWidth;
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
      <MajoranaControls {...controls} />
      <div class="qs-majorana-scroller">
        <svg
          class="qs-majorana-scene"
          style={{ width: `${sceneWidth}px` }}
          viewBox={`${layout.viewBox.x} ${layout.viewBox.y} ${layout.viewBox.width} ${layout.viewBox.height}`}
          aria-label={`${view} view at visualization step ${step} with ${layout.device.qubitCount} qubits`}
          data-step={step}
          data-selected-step={selectedStep}
          data-view={view}
          data-zoom={zoom}
          data-transition={transitionPhase}
          role="img"
        >
          <title>Majorana qubit visualization</title>
          <DeviceView {...viewProps} />
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

export function getMinimumSceneWidth(layout: MajoranaLayout): number {
  return Math.max(520, Math.ceil(layout.viewBox.width * 0.75));
}
