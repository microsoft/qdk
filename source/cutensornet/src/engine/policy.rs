// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine's fixed settings: the device, how cuTensorNet truncates and
//! factorizes, how much scratch a call may take, and how often a state
//! restarts. No caller tunes them. The SVD algorithm, the gauge and the
//! restart period were chosen by measurement on the A100 (the crate's
//! `README.md`, "The MPS engine"); the cutoff is the CPU engine's rule and
//! value, not yet run on the A100. The max bond dimension is not here: each
//! state's capacity, the `MpsShape` given to `prepare`, carries a bond for
//! every cut.
//!
//! cuTensorNet factorizes each two-site update with a singular value
//! decomposition (SVD) computed by cuSOLVER, NVIDIA's dense linear algebra
//! library, and keeps the MPS in a gauge (a choice of the factors' form
//! between updates). The engine fixes both to what H₂ ran with on the A100:
//! GESVD and the simple-update gauge. Of the others, GESVDP failed H₂ with
//! `CUTENSORNET_STATUS_INTERNAL_ERROR` at its 39th measurement and ran
//! about twice as slowly until then, and the free gauge ran a shot 47%
//! more slowly; GESVDJ and GESVDR were not measured.

/// The CUDA device every engine runs on. A process that runs on another
/// device selects it with `CUDA_VISIBLE_DEVICES`.
pub(super) const DEVICE_ORDINAL: i32 = 0;

/// The largest scratch a compute, read or sample may take, 64 GiB: more
/// than an A100 holds, so the device's free memory is the bound that acts.
pub(super) const MAXIMUM_WORKSPACE_BYTES: usize = 68_719_476_736;

/// The fraction of a cut's squared norm a truncation may discard beyond
/// the capacity's bond
/// (`CUTENSORNET_STATE_CONFIG_MPS_SVD_DISCARDED_WEIGHT_CUTOFF`): the
/// smallest singular values whose squares sum to at most this fraction of
/// their total are dropped. It is the same rule and value as the CPU
/// engine's (quimb's `rsum2`). It is relative to the cut's own weight, so
/// it cuts no deeper as the retained weight falls, which an absolute
/// cutoff on the unnormalized state would.
pub(super) const DISCARDED_WEIGHT_CUTOFF: f64 = 1e-14;

/// The computes after which a state restarts from its computed MPS on a
/// cuTensorNet handle of its own.
///
/// Needed because the scratch cuTensorNet recommends for a compute grows by
/// about 16.06 MiB with each compute done on a long-lived handle: about
/// 44 GiB at the end of an H₂ shot (two computes per measurement, 1,404
/// measurements), more than the A100 holds; the first H₂ shot failed at its
/// 1,252nd measurement, asking for 42 GB. On the A100, 300 steps of H₂'s
/// pattern with a restart every 10 computes on a fresh handle kept the
/// recommended scratch flat at 193 MiB, where every variant on the
/// engine's handle reached 4,831 MiB, for about 15 ms per restart (1% of
/// the time), with P(One) identical. A larger period raises that bound by
/// about 16 MiB per compute; a smaller one pays more restarts. Why the
/// handle grows is not known: a new state on the old handle asks for
/// 48 MiB, so the growth needs computes on a long-lived handle.
pub(super) const RESTART_PERIOD: usize = 10;
