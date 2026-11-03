// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

package circuit_test

import (
	"bytes"
	"errors"
	"strconv"
	"strings"
	"testing"

	circuit "github.com/microsoft/qdk/source/circuit_capi/bindings/go"
)

func own(t *testing.T, c *circuit.Circuit) *circuit.Circuit {
	t.Helper()
	t.Cleanup(func() {
		if err := c.Close(); err != nil {
			t.Errorf("close circuit: %v", err)
		}
	})
	return c
}

func newCircuit(t *testing.T) *circuit.Circuit {
	t.Helper()
	c, err := circuit.New(2, 2)
	if err != nil {
		t.Fatal(err)
	}
	return own(t, c)
}

func snapshot(t *testing.T, c *circuit.Circuit) []byte {
	t.Helper()
	data, err := c.MarshalBinary()
	if err != nil {
		t.Fatal(err)
	}
	return data
}

func TestBinarySnapshotSurvivesOwnerClose(t *testing.T) {
	c := newCircuit(t)
	if err := c.H(0); err != nil {
		t.Fatal(err)
	}
	data := snapshot(t, c)
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
	loaded, err := circuit.FromBinary(data)
	if err != nil {
		t.Fatal(err)
	}
	own(t, loaded)
	if !bytes.Equal(snapshot(t, loaded), data) {
		t.Fatal("binary snapshot did not roundtrip")
	}
}

func TestTextSnapshotMatchesBinary(t *testing.T) {
	c := newCircuit(t)
	if err := c.X(1); err != nil {
		t.Fatal(err)
	}
	text, err := c.MarshalText()
	if err != nil {
		t.Fatal(err)
	}
	loaded, err := circuit.FromText(text)
	if err != nil {
		t.Fatal(err)
	}
	own(t, loaded)
	if !bytes.Equal(snapshot(t, loaded), snapshot(t, c)) {
		t.Fatal("text and binary snapshots disagree")
	}
}

func TestCloneSurvivesOriginalClose(t *testing.T) {
	c := newCircuit(t)
	clone, err := c.Clone()
	if err != nil {
		t.Fatal(err)
	}
	own(t, clone)
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
	if err := clone.CX(0, 1); err != nil {
		t.Fatal(err)
	}
	snapshot(t, clone)
}

func TestAdaptiveBodyRequiresAdaptiveProfile(t *testing.T) {
	c := newCircuit(t)
	if err := c.H(0); err != nil {
		t.Fatal(err)
	}
	if err := c.Measure(0, 0); err != nil {
		t.Fatal(err)
	}
	if err := c.IfOne(0, func(body *circuit.Body) error { return body.X(1) }); err != nil {
		t.Fatal(err)
	}
	if err := c.Measure(1, 1); err != nil {
		t.Fatal(err)
	}
	_, err := c.QIR(circuit.Base)
	var native *circuit.Error
	if !errors.As(err, &native) || !strings.Contains(native.Message, "Adaptive") {
		t.Fatalf("expected native profile diagnostic, got %v", err)
	}
	qir, err := c.QIR(circuit.Adaptive)
	if err != nil || qir == "" {
		t.Fatalf("Adaptive compilation: %v", err)
	}
}

func TestCallbackFailureCancelsBody(t *testing.T) {
	c := newCircuit(t)
	before, err := c.QIR(circuit.Adaptive)
	if err != nil {
		t.Fatal(err)
	}
	rejected := errors.New("callback rejected construction")
	err = c.IfOne(0, func(body *circuit.Body) error {
		if err := body.X(1); err != nil {
			return err
		}
		return rejected
	})
	if !errors.Is(err, rejected) {
		t.Fatalf("callback error was lost: %v", err)
	}
	after, err := c.QIR(circuit.Adaptive)
	if err != nil || after != before {
		t.Fatalf("cancelled body changed the program: %v", err)
	}
}

func TestAppendFailureReleasesBodyHandles(t *testing.T) {
	c := newCircuit(t)
	err := c.IfOne(0, func(body *circuit.Body) error { return body.X(99) })
	var native *circuit.Error
	if !errors.As(err, &native) {
		t.Fatalf("expected native bounds error, got %v", err)
	}
	if err := c.H(0); err != nil {
		t.Fatal(err)
	}
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestCallbackPanicReleasesBodyHandles(t *testing.T) {
	c := newCircuit(t)
	func() {
		defer func() {
			if recovered := recover(); recovered != "callback panic" {
				t.Errorf("unexpected panic: %v", recovered)
			}
		}()
		_ = c.IfOne(0, func(body *circuit.Body) error {
			panic("callback panic")
		})
		t.Error("callback did not panic")
	}()
	if err := c.H(0); err != nil {
		t.Fatal(err)
	}
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestRetainedBodyIsClosedAfterCallback(t *testing.T) {
	c := newCircuit(t)
	var retained circuit.Body
	if err := c.IfOne(0, func(body *circuit.Body) error {
		retained = *body
		return body.X(1)
	}); err != nil {
		t.Fatal(err)
	}
	if err := retained.X(1); !errors.Is(err, circuit.ErrClosed) {
		t.Fatalf("retained body remained usable: %v", err)
	}
}

func TestCloseDuringCallbackIsRefusedWithoutLosingOwner(t *testing.T) {
	c := newCircuit(t)
	if err := c.IfOne(0, func(body *circuit.Body) error {
		if err := c.Close(); err == nil {
			t.Fatal("closed a circuit with an active body")
		}
		return body.X(1)
	}); err != nil {
		t.Fatal(err)
	}
	snapshot(t, c)
}

func TestValueCopySharesClosedState(t *testing.T) {
	c := newCircuit(t)
	copied := *c
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
	if err := copied.X(0); !errors.Is(err, circuit.ErrClosed) {
		t.Fatalf("value copy retained a freed pointer: %v", err)
	}
	if err := copied.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestZeroValueRejectsOperations(t *testing.T) {
	var c circuit.Circuit
	if err := c.X(0); !errors.Is(err, circuit.ErrClosed) {
		t.Fatalf("zero value should be closed: %v", err)
	}
}

func TestNegativeDimensionIsRejected(t *testing.T) {
	c, err := circuit.New(-1, 0)
	if err == nil || c != nil {
		t.Fatalf("accepted negative dimension: %v, %v", c, err)
	}
}

func TestLargeIndexDoesNotWrap(t *testing.T) {
	if strconv.IntSize < 64 {
		t.Skip("Go int cannot hold an index exceeding uint32")
	}
	c := newCircuit(t)
	large := uint64(1) << 32
	if err := c.X(int(large)); err == nil {
		t.Fatal("oversized index wrapped to zero")
	}
}

func TestNilCallbackIsRejected(t *testing.T) {
	c := newCircuit(t)
	if err := c.IfOne(0, nil); err == nil {
		t.Fatal("accepted nil callback")
	}
}

func TestMalformedBinaryReturnsDiagnostic(t *testing.T) {
	c, err := circuit.FromBinary([]byte("not a snapshot"))
	var native *circuit.Error
	if c != nil || !errors.As(err, &native) {
		t.Fatalf("expected native decode error, got %v, %v", c, err)
	}
}
