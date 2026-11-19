// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the part above may do after a refusal: [`RetryClass`].

/// Whether the part above may send a refused `execute` again, as
/// [`ExecuteRefusal::retry_class`](crate::ExecuteRefusal::retry_class)
/// gives it, so that a part that retries, such as one above a device that
/// may be busy, asks the refusal rather than matching its variants. The
/// runner sends nothing again: a refusal that reaches it fails the run,
/// whatever its class.
///
/// A failure ([`ExecuteError::Failed`](crate::ExecuteError::Failed)) has no
/// class: work started, so sending the same `execute` again could run a
/// shot's instructions twice.
///
/// Non-exhaustive: a class may be added, such as one for a refusal that
/// may be sent to another executor; a caller treats a class it does not
/// know as [`Never`](Self::Never).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RetryClass {
    /// The same `execute` is refused again: do not send it again.
    Never,
    /// The receiver could not take it now: the same `execute`, sent again
    /// later, may run, and the shots' states are as before the refusal.
    /// The protocol does not check this; it holds where every adapter
    /// below keeps the rule [`Adapt::adapt`](crate::Adapt::adapt) states:
    /// one that has had a call below run for an `execute` does not answer
    /// that `execute` with a refusal of this class, since sending it again
    /// would run that call's work twice.
    Later,
}
