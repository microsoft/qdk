## Plan: Optimize Classical Wire Rendering in Circuit SVG

Change how classical wires from measurement operations render: unused measurement results show only a short vertical stub (no horizontal wire), used results (those feeding classically-controlled gates) extend horizontally only to the controlled operation — not to the end of the circuit. Reduce vertical spacing between qubits by only allocating classical wire slots for results that have horizontal wires.

---

### Phase 1: Classical Wire Usage Analysis (New Module)

**New file**: `source/npm/qsharp/ux/circuit-vis/classicalWireAnalysis.ts`

**Step 1.1**: Create `analyzeClassicalWireUsage(componentGrid)` — scans the circuit to determine for each measurement result `{qubit, result}`:
- Whether it feeds a `ClassicalControlled` gate (`isUsedAsControl`)
- Which column(s) the consuming gate(s) appear in (`controlColumns`)
- Which column the measurement itself is in (`measurementColumn`)

Walks the grid collecting `Measurement` ops and their `results`, then matching them against operations with `isConditional === true` and classical `controls`. Recurses into `op.children` for nested groups.

**Step 1.2**: Create `assignClassicalWireSlots(qubitId, wireInfos)` — greedy interval slot assignment:
- Results NOT used as controls → no slot (they get a shared "stub" y-position)
- Results used as controls → assigned to the first slot whose `[measurementCol, maxControlCol]` range doesn't overlap with existing assignments
- Returns `Map<resultIndex, slotIndex>` + `maxSlots`

**Step 1.3**: Create `computeClassicalWireLayout(componentGrid, qubits)` — orchestrating function producing a `ClassicalWireLayout` with:
- `slotAssignment`: which slot each used result maps to
- `maxSlots`: per-qubit slot counts
- `wireRanges`: per-wire `{startCol, endCol}` for horizontal extents

---

### Phase 2: Modify Vertical Layout

**File**: `source/npm/qsharp/ux/circuit-vis/formatters/inputFormatter.ts`

**Step 2.1**: `formatInputs()` accepts `ClassicalWireLayout` as a new parameter.

**Step 2.2**: Change children allocation:
- Still creates `numResults` children entries (so `_getRegY()` indexing works unchanged)
- **Used results**: y = dedicated slot position below qubit wire
- **Unused results**: y = `qubitY + gateHeight/2 + stubLength` — shared position just below the measurement box; no extra vertical space consumed
- `currY` advances by `maxSlots` count instead of `numResults`

---

### Phase 3: Modify Register Rendering

**File**: `source/npm/qsharp/ux/circuit-vis/formatters/registerFormatter.ts`

**Step 3.1**: `formatRegisters()` accepts `ClassicalWireLayout`.

**Step 3.2**: Per-wire rendering logic:
- **Unused wire**: short vertical double-line stub (~12px below measurement box bottom). No horizontal lines.
- **Used wire**: vertical connector from qubit y to wire slot y, then horizontal double-line from measurement x to the `ClassicalControlled` gate's control circle x (found by scanning `allGates` for `ClassicalControlled` gates whose `controlsY` includes the wire's y).

**Step 3.3**: Add `classicalStubLength` constant (~12px) to `source/npm/qsharp/ux/circuit-vis/constants.ts`.

---

### Phase 4: Update Gate Splitting

**File**: `source/npm/qsharp/ux/circuit-vis/process.ts`

**Step 4.1**: `_getClassicalRegStarts()` → `_getActiveClassicalRegRanges()`, returning `[startCol, endCol, Register][]` triples. Only includes results with horizontal wires.

**Step 4.2**: Filtering changes from `regCol <= colIndex` to `startCol <= colIndex && endCol >= colIndex`, so gates only split around *active* horizontal classical wires.

**Step 4.3**: Receives `ClassicalWireLayout` to access wire ranges.

---

### Phase 5: Wire Data Through Pipeline

**File**: `source/npm/qsharp/ux/circuit-vis/sqore.ts`

**Step 5.1**: In `compose()`, call `computeClassicalWireLayout()` before `formatInputs()` / `processOperations()`.

**Step 5.2**: Pass `ClassicalWireLayout` through to `formatInputs()`, `processOperations()`, and `formatRegisters()`.

---

### Relevant Files
- **NEW** `source/npm/qsharp/ux/circuit-vis/classicalWireAnalysis.ts` — analysis + slot assignment
- `source/npm/qsharp/ux/circuit-vis/sqore.ts` — orchestrate analysis, wire through `compose()`
- `source/npm/qsharp/ux/circuit-vis/formatters/inputFormatter.ts` — slot-based y-allocation
- `source/npm/qsharp/ux/circuit-vis/formatters/registerFormatter.ts` — stub vs full wire rendering
- `source/npm/qsharp/ux/circuit-vis/process.ts` — gate splitting with active wire ranges
- `source/npm/qsharp/ux/circuit-vis/constants.ts` — add `classicalStubLength`

### Verification
1. Circuit with multiple measurements, none used as controls → short stubs only, minimal qubit spacing
2. Circuit with measurement feeding a classically-controlled gate → horizontal wire stops at the control circle
3. Circuit with same result used in multiple controls → wire extends to rightmost control
4. Circuit with expanded groups containing measurements → wires render correctly inside groups
5. Circuit with NO measurements → renders identically to current behavior
6. Editing mode: adding/removing measurements via drag-and-drop → wires update correctly
7. Build: `npm run build` from `source/npm/qsharp` passes
8. VS Code integration tests: `npm test` from `source/vscode/` passes

### Decisions
- **Stub style**: short vertical double-line (~12px) below measurement box. Clean stop, no arrowhead.
- **Used wire endpoint**: stops at consuming gate's control circle x (rightmost if multiple consumers).
- **Slot reuse**: greedy interval assignment so non-overlapping used wires share y-slots.
- **Data model unchanged**: `Qubit.numResults`, `Register`, etc. untouched — only rendering changes.
- **Out of scope**: text/box-art rendering — only SVG rendering changes.

### Further Considerations
1. **Measurements inside expanded groups whose *controls* are outside the group**: the recursive scan must correlate across nesting boundaries. The use of global `{qubit, result}` matching handles this since registers are globally identified.
2. **Edge case — measurement with no `results` array**: guard against `op.results` being empty or undefined in the analysis function.
