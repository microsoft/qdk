// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { h, render } from "preact";
import { MajoranaScene, type MajoranaSceneProps } from "./app.js";
import { createMajoranaGeometry, type MajoranaGeometry } from "./geometry.js";
import { projectTraceStep } from "./operations.js";
import { createMajoranaTopology, type MajoranaTopology } from "./topology.js";
import {
  createVirtualGeometry,
  type VirtualGeometry,
} from "./virtual-geometry.js";
import {
  projectVirtualTraceStep,
  type VirtualOperationProjection,
} from "./virtual-operations.js";
import type { VirtualOperationRenderState } from "./virtual.js";
import {
  createVirtualTopology,
  type VirtualTopology,
} from "./virtual-topology.js";
import {
  MAJORANA_VIEWS,
  type MajoranaController,
  type MajoranaInput,
  type MajoranaOptions,
  type MajoranaPresentationOptions,
  type MajoranaTransitionPhase,
  type MajoranaView,
  type ResolvedMajoranaOptions,
  type ValidatedMajoranaInput,
} from "./types.js";
import {
  MajoranaValidationError,
  validateMajoranaInput,
  validateMajoranaOptions,
  validateMajoranaPresentationOptions,
} from "./validation.js";

const playbackSpeeds = [0.25, 0.5, 1, 2, 3, 5] as const;
type PlaybackSpeed = (typeof playbackSpeeds)[number];

type MajoranaSceneModel = {
  validatedInput: ValidatedMajoranaInput;
  topology: MajoranaTopology;
  geometry: MajoranaGeometry;
  virtualTopology: VirtualTopology | undefined;
  virtualGeometry: VirtualGeometry | undefined;
};

function createSceneModel(
  input: MajoranaInput,
  enableVirtualView: boolean,
): MajoranaSceneModel {
  const validatedInput = validateMajoranaInput(input, { enableVirtualView });
  const topology = createMajoranaTopology(validatedInput.device);
  const geometry = createMajoranaGeometry(topology);
  const virtualTopology = enableVirtualView
    ? createVirtualTopology(validatedInput.device)
    : undefined;
  const virtualGeometry =
    virtualTopology === undefined
      ? undefined
      : createVirtualGeometry(virtualTopology, geometry);
  return {
    validatedInput,
    topology,
    geometry,
    virtualTopology,
    virtualGeometry,
  };
}

export function Majorana(
  container: HTMLElement,
  input: MajoranaInput,
  options?: MajoranaOptions,
): MajoranaController {
  const transitionDuration = 75;
  const zoomStep = 0.15;
  const resolvedOptions = validateMajoranaOptions(options);
  let disposed = false;
  let scene: MajoranaSceneModel | undefined;
  let view = resolvedOptions.initialView;
  let selectedStep = 0;
  let renderedStep = 0;
  let transitionPhase: MajoranaTransitionPhase = "stable";
  let renderedVirtualOperations: readonly VirtualOperationRenderState[] = [];
  let playing = false;
  let speed: PlaybackSpeed = 1;
  let zoom = 1;
  let availableWidth = getAvailableWidth(container);
  let error: MajoranaValidationError | undefined;
  let presentation = presentationFromOptions(resolvedOptions);
  let playbackTimer: ReturnType<typeof setTimeout> | undefined;
  let fadeOutTimer: ReturnType<typeof setTimeout> | undefined;
  let fadeInTimer: ReturnType<typeof setTimeout> | undefined;
  const motionQuery = window.matchMedia?.("(prefers-reduced-motion: reduce)");
  let reducedMotion = motionQuery?.matches ?? false;

  try {
    scene = createSceneModel(input, resolvedOptions.enableVirtualView);
  } catch (caught) {
    if (!(caught instanceof MajoranaValidationError)) {
      throw caught;
    }
    error = caught;
  }

  const root = document.createElement("div");
  root.className = "qs-majorana-root";
  root.tabIndex = 0;
  root.addEventListener("keydown", handleKeyDown);
  container.appendChild(root);

  function renderScene(): void {
    const operations =
      scene === undefined || renderedStep === 0
        ? []
        : projectTraceStep(
            scene.topology,
            scene.validatedInput.trace[renderedStep - 1],
          );
    const props: MajoranaSceneProps = {
      topology: scene?.topology,
      geometry: scene?.geometry,
      virtualTopology: scene?.virtualTopology,
      virtualGeometry: scene?.virtualGeometry,
      virtualOperations: renderedVirtualOperations,
      operations,
      presentation,
      view,
      zoom,
      availableWidth,
      step: renderedStep,
      selectedStep,
      transitionPhase,
      error,
      controls: {
        maximumStep: scene?.validatedInput.trace.length ?? 0,
        playing,
        onViewChange: setView,
        onStepChange: setStep,
        onPlayPause: togglePlayback,
        onZoomIn: zoomIn,
        onZoomOut: zoomOut,
        onShowQubitLabelsChange: (show) =>
          updatePresentation({ showQubitLabels: show }),
        onShowMzmLabelsChange: (show) =>
          updatePresentation({ showMzmLabels: show }),
      },
    };
    render(h(MajoranaScene, props), root);
  }

  function assertActive(): void {
    if (disposed) {
      throw new Error("Majorana controller has been disposed");
    }
  }

  function clearPlaybackTimer(): void {
    if (playbackTimer !== undefined) {
      clearTimeout(playbackTimer);
      playbackTimer = undefined;
    }
  }

  function cancelTransition(settle: boolean): void {
    if (fadeOutTimer !== undefined) {
      clearTimeout(fadeOutTimer);
      fadeOutTimer = undefined;
    }
    if (fadeInTimer !== undefined) {
      clearTimeout(fadeInTimer);
      fadeInTimer = undefined;
    }
    if (settle) {
      renderedStep = selectedStep;
    }
    renderedVirtualOperations = stableVirtualOperationsForStep(
      settle ? selectedStep : renderedStep,
    );
    transitionPhase = "stable";
  }

  function stopPlayback(): void {
    clearPlaybackTimer();
    playing = false;
  }

  function transitionTo(
    nextStep: number,
    source: "manual" | "automatic",
  ): void {
    cancelTransition(false);
    selectedStep = nextStep;
    const nextVirtualOperations = virtualOperationsForStep(nextStep);
    const skipFade = reducedMotion || (source === "automatic" && speed >= 2);
    if (renderedStep === nextStep || skipFade) {
      renderedStep = nextStep;
      renderedVirtualOperations = stableRenderStates(nextVirtualOperations);
      transitionPhase = "stable";
      renderScene();
      return;
    }

    const currentByKey = new Map(
      renderedVirtualOperations.map(({ projection }) => [
        projection.identity.key,
        projection,
      ]),
    );
    const nextByKey = new Map(
      nextVirtualOperations.map((projection) => [
        projection.identity.key,
        projection,
      ]),
    );
    const sharedKeys = new Set(
      [...currentByKey.keys()].filter((key) => nextByKey.has(key)),
    );
    renderedVirtualOperations = [...currentByKey.values()].map(
      (projection) => ({
        projection: nextByKey.get(projection.identity.key) ?? projection,
        transitionPhase: sharedKeys.has(projection.identity.key)
          ? "stable"
          : "out",
      }),
    );
    transitionPhase = "out";
    renderScene();
    fadeOutTimer = setTimeout(() => {
      fadeOutTimer = undefined;
      renderedStep = selectedStep;
      renderedVirtualOperations = nextVirtualOperations.map((projection) => ({
        projection,
        transitionPhase: sharedKeys.has(projection.identity.key)
          ? "stable"
          : "in",
      }));
      transitionPhase = "in";
      renderScene();
      fadeInTimer = setTimeout(() => {
        fadeInTimer = undefined;
        renderedVirtualOperations = stableRenderStates(nextVirtualOperations);
        transitionPhase = "stable";
        renderScene();
      }, transitionDuration);
    }, transitionDuration);
  }

  function setStep(nextStep: number): void {
    assertActive();
    const maximumStep = scene?.validatedInput.trace.length ?? 0;
    if (!Number.isInteger(nextStep) || nextStep < 0 || nextStep > maximumStep) {
      throw new RangeError(
        `Majorana visualization step must be an integer from 0 through ${maximumStep}`,
      );
    }
    stopPlayback();
    transitionTo(nextStep, "manual");
  }

  function setView(nextView: MajoranaView): void {
    assertActive();
    view = validateView(nextView, resolvedOptions.enableVirtualView);
    renderScene();
  }

  function updatePresentation(nextOptions: MajoranaPresentationOptions): void {
    assertActive();
    const validatedOptions = validateMajoranaPresentationOptions(nextOptions);
    presentation = { ...presentation, ...validatedOptions };
    renderScene();
  }

  function schedulePlayback(): void {
    clearPlaybackTimer();
    playbackTimer = setTimeout(() => {
      playbackTimer = undefined;
      const maximumStep = scene?.validatedInput.trace.length ?? 0;
      const nextStep = selectedStep + 1;
      transitionTo(nextStep, "automatic");
      if (nextStep >= maximumStep) {
        playing = false;
        renderScene();
      } else {
        schedulePlayback();
      }
    }, 1000 / speed);
  }

  function togglePlayback(): void {
    assertActive();
    if (playing) {
      stopPlayback();
      renderScene();
      return;
    }

    const maximumStep = scene?.validatedInput.trace.length ?? 0;
    if (selectedStep === maximumStep) {
      transitionTo(0, "automatic");
    }
    playing = true;
    schedulePlayback();
    renderScene();
  }

  function setSpeed(nextSpeed: PlaybackSpeed): void {
    assertActive();
    speed = nextSpeed;
    if (playing) {
      schedulePlayback();
    }
  }

  function adjustSpeed(direction: -1 | 1): void {
    const speedIndex = playbackSpeeds.indexOf(speed);
    const nextIndex = Math.min(
      playbackSpeeds.length - 1,
      Math.max(0, speedIndex + direction),
    );
    const nextSpeed = playbackSpeeds[nextIndex];
    if (nextSpeed !== speed) {
      setSpeed(nextSpeed);
    }
  }

  function zoomIn(): void {
    assertActive();
    zoom += zoomStep * zoom;
    renderScene();
  }

  function handleKeyDown(event: KeyboardEvent): void {
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
    const maximumStep = scene?.validatedInput.trace.length ?? 0;
    switch (key) {
      case "ArrowRight":
        if (selectedStep < maximumStep) {
          setStep(selectedStep + 1);
        }
        break;
      case "ArrowLeft":
        if (selectedStep > 0) {
          setStep(selectedStep - 1);
        }
        break;
      case "ArrowUp":
        zoomIn();
        break;
      case "ArrowDown":
        zoomOut();
        break;
      case "p":
        togglePlayback();
        break;
      case "f":
        adjustSpeed(1);
        break;
      case "s":
        adjustSpeed(-1);
        break;
      case "t":
        cycleView();
        break;
      default:
        return;
    }
    event.preventDefault();
    event.stopPropagation();
  }

  function zoomOut(): void {
    assertActive();
    zoom -= zoomStep * zoom;
    renderScene();
  }

  function cycleView(): void {
    const availableViews: readonly MajoranaView[] =
      resolvedOptions.enableVirtualView
        ? MAJORANA_VIEWS
        : MAJORANA_VIEWS.filter((option) => option !== "Virtual");
    const currentIndex = availableViews.indexOf(view);
    setView(availableViews[(currentIndex + 1) % availableViews.length]);
  }

  function virtualOperationsForStep(
    step: number,
  ): readonly VirtualOperationProjection[] {
    if (scene?.virtualTopology === undefined || step === 0) {
      return [];
    }
    return projectVirtualTraceStep(
      scene.virtualTopology,
      scene.validatedInput.trace[step - 1],
    );
  }

  function stableVirtualOperationsForStep(
    step: number,
  ): readonly VirtualOperationRenderState[] {
    return stableRenderStates(virtualOperationsForStep(step));
  }

  function handleMotionChange(event: MediaQueryListEvent): void {
    reducedMotion = event.matches;
    if (reducedMotion && transitionPhase !== "stable") {
      cancelTransition(true);
      renderScene();
    }
  }

  const resizeObserver =
    typeof ResizeObserver === "undefined"
      ? undefined
      : new ResizeObserver((entries) => {
          const nextWidth = entries[0]?.contentRect.width;
          if (
            nextWidth !== undefined &&
            nextWidth > 0 &&
            nextWidth !== availableWidth
          ) {
            availableWidth = nextWidth;
            renderScene();
          }
        });
  resizeObserver?.observe(container);
  motionQuery?.addEventListener("change", handleMotionChange);
  renderScene();

  return {
    updateInput(nextInput): void {
      assertActive();
      stopPlayback();
      cancelTransition(true);
      try {
        scene = createSceneModel(nextInput, resolvedOptions.enableVirtualView);
        selectedStep = Math.min(
          selectedStep,
          scene.validatedInput.trace.length,
        );
        renderedStep = selectedStep;
        renderedVirtualOperations =
          stableVirtualOperationsForStep(selectedStep);
        error = undefined;
      } catch (caught) {
        if (!(caught instanceof MajoranaValidationError)) {
          throw caught;
        }
        error = caught;
      }
      renderScene();
    },
    updatePresentation,
    setView,
    setStep,
    dispose(): void {
      assertActive();
      stopPlayback();
      cancelTransition(false);
      resizeObserver?.disconnect();
      motionQuery?.removeEventListener("change", handleMotionChange);
      root.removeEventListener("keydown", handleKeyDown);
      disposed = true;
      render(null, root);
      root.remove();
    },
  };
}

function getAvailableWidth(container: HTMLElement): number | undefined {
  return container.clientWidth > 0 ? container.clientWidth : undefined;
}

function presentationFromOptions(
  options: ResolvedMajoranaOptions,
): Required<MajoranaPresentationOptions> {
  return {
    showQubitLabels: options.showQubitLabels,
    showMzmLabels: options.showMzmLabels,
  };
}

function validateView(
  view: MajoranaView,
  enableVirtualView: boolean,
): MajoranaView {
  return validateMajoranaOptions({
    enableVirtualView,
    initialView: view,
  }).initialView;
}

function stableRenderStates(
  operations: readonly VirtualOperationProjection[],
): readonly VirtualOperationRenderState[] {
  return operations.map((projection) => ({
    projection,
    transitionPhase: "stable",
  }));
}

export {
  MajoranaValidationError,
  validateMajoranaInput,
  validateMajoranaOptions,
  validateMajoranaPresentationOptions,
} from "./validation.js";

export type {
  JointQubitOperation,
  JointQubitOperationName,
  JointVirtualOperationTraceEntry,
  MajoranaController,
  MajoranaDeviceSize,
  MajoranaInput,
  MajoranaInputValidationOptions,
  MajoranaOperation,
  MajoranaOperationName,
  MajoranaOptions,
  MajoranaPresentationOptions,
  MajoranaTrace,
  MajoranaTraceStep,
  MajoranaValidationErrorCode,
  MajoranaView,
  ResolvedMajoranaOptions,
  SingleVirtualOperationTraceEntry,
  SingleQubitOperation,
  SingleQubitOperationName,
  ValidatedMajoranaInput,
  VirtualOperationName,
  VirtualOperationTraceEntry,
} from "./types.js";
