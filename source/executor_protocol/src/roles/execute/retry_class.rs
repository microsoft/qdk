// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the part above may do after a refusal: [`RetryClass`].

/// Whether the part above may send a refused `execute` again, as
/// [`ExecuteRefusal::retry_class`](crate::ExecuteRefusal::retry_class)
/// gives it, so that a part that retries, such as one above a device that
/// may be busy, asks the refusal rather than matching its variants. The
/// protocol sends nothing again, whatever the class: a refusal that
/// reaches the runner fails the run. A part that retries a refusal does so
/// itself.
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
    /// later, may run, and the shots' states are as before the refusal,
    /// since a refusal says that nothing of the `execute` ran. The protocol
    /// does not check this: it holds where every adapter below keeps the
    /// rule [`Adapt::adapt`](crate::Adapt::adapt) states, that once a call
    /// below has run for an `execute`, the adapter answers no refusal.
    Later,
}
