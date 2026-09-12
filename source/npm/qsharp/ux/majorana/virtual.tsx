// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { VIRTUAL_GEOMETRY, type VirtualGeometry } from "./virtual-geometry.js";
import {
  selectDisplayedVirtualOperations,
  type ContributingPhysicalEvent,
  type VirtualGlyph,
  type VirtualOperationProjection,
} from "./virtual-operations.js";
import type { VirtualTopology } from "./virtual-topology.js";

export type VirtualProps = {
  topology: VirtualTopology;
  geometry: VirtualGeometry;
  operations: readonly VirtualOperationRenderState[];
  showQubitLabels: boolean;
};

export type VirtualOperationRenderState = {
  projection: VirtualOperationProjection;
  transitionPhase: "stable" | "out" | "in";
};

export function Virtual({
  topology,
  geometry,
  operations,
  showQubitLabels,
}: VirtualProps) {
  const displayedKeys = new Set(
    selectDisplayedVirtualOperations(
      operations.map(({ projection }) => projection),
    ).map(({ identity }) => identity.key),
  );
  const displayedOperations = operations.filter(({ projection }) =>
    displayedKeys.has(projection.identity.key),
  );
  const activeEdges = new Set(
    displayedOperations.flatMap(({ projection }) =>
      projection.activeAdjacencyEdgeId === undefined
        ? []
        : [projection.activeAdjacencyEdgeId],
    ),
  );
  const activeQubits = new Set(
    displayedOperations.flatMap(({ projection }) =>
      projection.glyphs.map((glyph) => glyph.virtualQubitId),
    ),
  );

  return (
    <g class="qs-majorana-virtual" data-view="Virtual">
      <g
        class="qs-majorana-virtual-adjacencies"
        aria-label="Supported virtual qubit adjacency"
      >
        {topology.adjacencyEdges.map((edge) => {
          const connector = geometry.adjacencyEdges.get(edge.id);
          if (connector === undefined) {
            throw new Error(
              `Missing virtual geometry for adjacency ${edge.id}`,
            );
          }
          const active = activeEdges.has(edge.id);
          return (
            <line
              key={edge.id}
              class={`qs-majorana-virtual-adjacency qs-majorana-virtual-adjacency-${edge.orientation}`}
              data-majorana-id={edge.id}
              data-orientation={edge.orientation}
              data-scope={edge.scope}
              data-supported-operation={edge.supportedOperation}
              data-active={active ? "true" : "false"}
              aria-label={`${active ? "Active" : "Supported"} ${
                edge.orientation
              } adjacency between V${edge.firstVirtualQubitId} and V${
                edge.secondVirtualQubitId
              }, supports ${edge.supportedOperation}`}
              role="group"
              x1={connector.start.x}
              y1={connector.start.y}
              x2={connector.end.x}
              y2={connector.end.y}
            />
          );
        })}
      </g>
      <g class="qs-majorana-virtual-qubits" aria-label="Virtual qubits">
        {topology.qubits.map((qubit) => {
          const qubitGeometry = geometry.qubits.get(qubit.id);
          if (qubitGeometry === undefined) {
            throw new Error(`Missing geometry for virtual qubit V${qubit.id}`);
          }
          const active = activeQubits.has(qubit.id);
          return (
            <g
              key={qubit.id}
              data-majorana-id={`v${qubit.id}`}
              data-active={active ? "true" : "false"}
              aria-label={`Virtual qubit V${qubit.id}, associated physical qubits Q${qubit.physicalQubitIds[0]} and Q${qubit.physicalQubitIds[1]}${active ? ", active" : ""}`}
              role="group"
            >
              <circle
                class="qs-majorana-virtual-qubit"
                cx={qubitGeometry.center.x}
                cy={qubitGeometry.center.y}
                r={qubitGeometry.radius}
              />
            </g>
          );
        })}
      </g>
      <g class="qs-majorana-virtual-operation-layer">
        {displayedOperations.map(({ projection, transitionPhase }) => (
          <g
            key={projection.identity.key}
            class={`qs-majorana-virtual-operation qs-majorana-virtual-operation-${transitionPhase}`}
            data-operation={projection.operation.name}
            data-operation-identity={projection.identity.key}
            data-transition={transitionPhase}
            aria-label={operationLabel(projection)}
            role="group"
          >
            {projection.activeAdjacencyEdgeId !== undefined && (
              <ActiveConnector
                edgeId={projection.activeAdjacencyEdgeId}
                geometry={geometry}
              />
            )}
            {projection.glyphs.map((glyph) => {
              const qubit = geometry.qubits.get(glyph.virtualQubitId);
              if (qubit === undefined) {
                throw new Error(
                  `Missing geometry for virtual qubit V${glyph.virtualQubitId}`,
                );
              }
              return (
                <g key={`${glyph.virtualQubitId}-${glyphRole(glyph)}`}>
                  <circle
                    class="qs-majorana-virtual-qubit-active"
                    aria-hidden="true"
                    cx={qubit.center.x}
                    cy={qubit.center.y}
                    r={qubit.radius}
                  />
                  <text
                    class="qs-majorana-virtual-operation-glyph"
                    data-virtual-qubit-id={glyph.virtualQubitId}
                    data-role={glyphRole(glyph)}
                    aria-label={glyphLabel(projection, glyph)}
                    role="group"
                    x={qubit.center.x}
                    y={
                      qubit.center.y + VIRTUAL_GEOMETRY.operationBaselineOffset
                    }
                  >
                    {glyph.text}
                  </text>
                </g>
              );
            })}
          </g>
        ))}
      </g>
      {showQubitLabels && (
        <g class="qs-majorana-virtual-qubit-label-layer" aria-hidden="true">
          {topology.qubits.map((qubit) => {
            const qubitGeometry = geometry.qubits.get(qubit.id);
            if (qubitGeometry === undefined) {
              throw new Error(
                `Missing geometry for virtual qubit V${qubit.id}`,
              );
            }
            return (
              <text
                key={qubit.id}
                class="qs-majorana-virtual-qubit-label"
                x={qubitGeometry.center.x}
                y={
                  qubitGeometry.center.y + VIRTUAL_GEOMETRY.labelBaselineOffset
                }
              >
                V{qubit.id}
              </text>
            );
          })}
        </g>
      )}
    </g>
  );
}

function ActiveConnector({
  edgeId,
  geometry,
}: {
  edgeId: string;
  geometry: VirtualGeometry;
}) {
  const connector = geometry.adjacencyEdges.get(edgeId);
  if (connector === undefined) {
    throw new Error(`Missing virtual geometry for adjacency ${edgeId}`);
  }
  return (
    <line
      class={`qs-majorana-virtual-adjacency-active qs-majorana-virtual-adjacency-${connector.orientation}`}
      data-orientation={connector.orientation}
      aria-hidden="true"
      x1={connector.start.x}
      y1={connector.start.y}
      x2={connector.end.x}
      y2={connector.end.y}
    />
  );
}

function operationLabel(operation: VirtualOperationProjection): string {
  const targets = operation.identity.normalizedTargetIds
    .map((target) => `V${target}`)
    .join(", ");
  const contributors = operation.contributingPhysicalEvents
    .map(contributingEventLabel)
    .join("; ");
  return `${operation.operation.name} on ${targets}${
    contributors.length === 0 ? "" : `, via ${contributors}`
  }`;
}

function glyphLabel(
  operation: VirtualOperationProjection,
  glyph: VirtualGlyph,
): string {
  const role = "role" in glyph ? ` ${glyph.role}` : "";
  return `${operation.operation.name}${role} on V${glyph.virtualQubitId}: ${glyph.text}`;
}

function glyphRole(glyph: VirtualGlyph): string {
  return "role" in glyph ? glyph.role : "single";
}

function contributingEventLabel(event: ContributingPhysicalEvent): string {
  return `${event.operation} on ${event.targetQubitIds
    .map((target) => `Q${target}`)
    .join(", ")}`;
}
