// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

package capi

import (
	"bytes"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func check(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
}

func circuitForTest(t *testing.T) *circuit {
	t.Helper()
	value, err := newCircuit()
	check(t, err)
	t.Cleanup(func() { check(t, value.close()) })
	return value
}

func graphForTest(t *testing.T) *graph {
	t.Helper()
	value := circuitForTest(t)
	check(t, value.h())
	graph, err := value.lower()
	check(t, err)
	t.Cleanup(func() { check(t, graph.close()) })
	return graph
}

func TestLibraryObjectLifetime(t *testing.T) {
	value := circuitForTest(t)
	check(t, value.h())
	copy, err := value.clone()
	check(t, err)
	defer func() { check(t, copy.close()) }()
	check(t, value.close())
	size, err := copy.size()
	check(t, err)
	if size != 1 {
		t.Fatalf("clone lost its instruction: %d", size)
	}
}

func TestOwnedDataCopies(t *testing.T) {
	value := circuitForTest(t)
	check(t, value.h())
	snapshot, err := value.snapshot()
	check(t, err)
	copy, err := value.clone()
	check(t, err)
	expected, err := copy.snapshot()
	check(t, err)
	check(t, copy.close())
	check(t, value.close())
	runtime.GC()
	if !bytes.HasPrefix(snapshot, []byte("QDKCIRCT\x01\n")) || !bytes.Equal(snapshot, expected) || len(snapshot) <= 10 {
		t.Fatal("owned snapshot copy is not a complete binary document")
	}
}

func TestDiagnosticsAndRejectedContext(t *testing.T) {
	state := &callbackState{}
	pass, err := register(state, false)
	if err == nil || !strings.Contains(err.Error(), "unknown graph pass kind") || pass != nil {
		t.Fatalf("unexpected failed registration: %v %v", pass, err)
	}
	if state.nativeDeletes != 0 || state.callerDeletes != 1 {
		t.Fatalf("failed registration transferred context: %+v", state)
	}
}

func TestSymbolicDynamicConstruction(t *testing.T) {
	value := circuitForTest(t)
	check(t, value.symbolicConditional())
	snapshot, err := value.snapshotText()
	check(t, err)
	if !bytes.Contains(snapshot, []byte("theta")) || !bytes.Contains(snapshot, []byte("(ifElse ")) {
		t.Fatalf("symbolic conditional did not survive C construction: %s", snapshot)
	}
}

func TestGraphRecordsAndOutputs(t *testing.T) {
	graph := graphForTest(t)
	qir, err := graph.qir()
	check(t, err)
	if !bytes.Contains(qir, []byte("__quantum__qis__h__body")) {
		t.Fatalf("graph QIR did not include H: %s", qir)
	}
	operation, err := graph.copiedOperationAfterClose()
	check(t, err)
	if operation == "" {
		t.Fatal("copied graph record lost its operation")
	}
}

func TestRetainedCallbackSurvivesGC(t *testing.T) {
	graph := graphForTest(t)
	pass, err := register(&callbackState{payload: bytes.Repeat([]byte{7}, 4096)}, true)
	check(t, err)
	defer func() { check(t, pass.close()) }()
	for i := 0; i < 3; i++ {
		runtime.GC()
		output, count, err := pass.run(graph)
		check(t, err)
		if count != 1 {
			t.Fatalf("callbacks per match: %d", count)
		}
		qir, err := output.qir()
		check(t, err)
		check(t, output.close())
		if !bytes.Contains(qir, []byte("__quantum__qis__x__body")) {
			t.Fatal("Go callback replacement did not reach QIR")
		}
	}
	state := pass.state()
	if state.calls != 3 || state.operation != "quantum.h" || len(state.payload) != 4096 || state.payload[0] != 7 {
		t.Fatalf("retained context did not survive GC: %+v", state)
	}
}

func TestCallbackPanicBecomesDiagnostic(t *testing.T) {
	graph := graphForTest(t)
	before, err := graph.snapshot()
	check(t, err)
	pass, err := register(&callbackState{mode: "panic"}, true)
	check(t, err)
	defer func() { check(t, pass.close()) }()
	output, _, err := pass.run(graph)
	if err == nil || !strings.Contains(err.Error(), "Go callback panic: requested failure") || output != nil {
		t.Fatalf("panic did not become a non-publishing diagnostic: %v %v", output, err)
	}
	after, err := graph.snapshot()
	check(t, err)
	if !bytes.Equal(before, after) {
		t.Fatal("failed callback changed the input graph")
	}
}

func TestContextDestroyedExactlyOnce(t *testing.T) {
	state := &callbackState{}
	pass, err := register(state, true)
	check(t, err)
	if state.nativeDeletes != 0 {
		t.Fatal("context destroyed during registration")
	}

	check(t, pass.close())
	check(t, pass.close())
	if state.nativeDeletes != 1 || state.callerDeletes != 0 {
		t.Fatalf("incorrect context cleanup: %+v", state)
	}
}

func feedbackForTest(t *testing.T) *circuit {
	t.Helper()
	value, err := feedbackCircuit()
	check(t, err)
	t.Cleanup(func() { check(t, value.close()) })
	return value
}

func assertQIRSnapshot(t *testing.T, profile uint32, filename string) {
	t.Helper()
	value := feedbackForTest(t)
	actual, err := value.qir(profile)
	check(t, err)
	expected, err := os.ReadFile(filepath.Join("..", "fixtures", filename))
	check(t, err)
	if !bytes.Equal(actual, expected) {
		t.Fatalf("%s QIR mismatch\nexpected (%d bytes):\n%s\nactual (%d bytes):\n%s",
			filename, len(expected), expected, len(actual), actual)
	}
}

func TestAdaptiveQIRSnapshot(t *testing.T) {
	assertQIRSnapshot(t, profileAdaptive, "adaptive.ll")
}

func TestAdaptiveRIQIRSnapshot(t *testing.T) {
	assertQIRSnapshot(t, profileAdaptiveRI, "adaptive_ri.ll")
}

func TestFeedbackRejectsBaseProfile(t *testing.T) {
	value := feedbackForTest(t)
	output, err := value.qir(profileBase)
	if err == nil || !strings.Contains(err.Error(), "Adaptive") || output != nil {
		t.Fatalf("expected Adaptive capability refusal and no output, got %q: %v", output, err)
	}
}
