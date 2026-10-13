// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod backends;

use super::QirBackend;

/// Exercises every gate operation supported by a backend.
fn apply_all_gates(backend: &mut dyn QirBackend) -> Result<(), String> {
    backend.ccx(0, 1, 2)?;
    backend.cx(0, 1)?;
    backend.cy(0, 1)?;
    backend.cz(0, 1)?;
    backend.rx(0.5, 0)?;
    backend.rxx(0.5, 0, 1)?;
    backend.ry(0.5, 0)?;
    backend.ryy(0.5, 0, 1)?;
    backend.rz(0.5, 0)?;
    backend.rzz(0.5, 0, 1)?;
    backend.h(0)?;
    backend.s(0)?;
    backend.sadj(0)?;
    backend.sx(0)?;
    backend.t(0)?;
    backend.tadj(0)?;
    backend.x(0)?;
    backend.y(0)?;
    backend.z(0)?;
    backend.swap(0, 1)?;
    backend.reset(0)
}
