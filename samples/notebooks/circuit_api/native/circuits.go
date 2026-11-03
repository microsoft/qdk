// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

package main

import (
	"errors"
	"fmt"
	"log"
	"os"

	circuit "github.com/microsoft/qdk/source/circuit_capi/bindings/go"
)

func save(c *circuit.Circuit, prefix string, profile circuit.Profile) error {
	binary, err := c.MarshalBinary()
	if err != nil {
		return err
	}
	text, err := c.MarshalText()
	if err != nil {
		return err
	}
	qir, err := c.QIR(profile)
	if err != nil {
		return err
	}
	if err := os.WriteFile(prefix+".qdk", binary, 0600); err != nil {
		return err
	}
	if err := os.WriteFile(prefix+".sexpr", text, 0600); err != nil {
		return err
	}
	if err := os.WriteFile(prefix+".ll", []byte(qir), 0600); err != nil {
		return err
	}
	fmt.Printf("Wrote %s.qdk (%d bytes), %s.sexpr and %s.ll\n", prefix, len(binary), prefix, prefix)
	return nil
}

func buildBase(c *circuit.Circuit) error {
	if err := c.CX(0, 1); err != nil {
		return err
	}
	if err := c.Measure(0, 0); err != nil {
		return err
	}
	if err := c.Measure(1, 1); err != nil {
		return err
	}
	return save(c, "base", circuit.Base)
}

func buildAdaptive(c *circuit.Circuit) error {
	if err := c.Measure(0, 0); err != nil {
		return err
	}
	if err := c.IfOne(0, func(body *circuit.Body) error {
		return body.X(1)
	}); err != nil {
		return err
	}
	if err := c.Measure(1, 1); err != nil {
		return err
	}
	return save(c, "adaptive", circuit.Adaptive)
}

func run() (err error) {
	prefix, err := circuit.New(2, 2)
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, prefix.Close()) }()
	if err := prefix.H(0); err != nil {
		return err
	}

	base, err := prefix.Clone()
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, base.Close()) }()
	adaptive, err := prefix.Clone()
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, adaptive.Close()) }()

	// Clones remain usable after the preparation circuit is closed.
	if err := prefix.Close(); err != nil {
		return err
	}
	if err := buildBase(base); err != nil {
		return fmt.Errorf("base circuit: %w", err)
	}
	if err := buildAdaptive(adaptive); err != nil {
		return fmt.Errorf("adaptive circuit: %w", err)
	}
	return nil
}

func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}
