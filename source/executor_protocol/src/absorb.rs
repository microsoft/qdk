// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Keeping a panic of code the crate runs from meeting another, which
//! would abort the process, and from going on where it must not.
//!
//! - Absorbed, where the panic must not go on: a waker woken on a
//!   placement's thread, or as `drop` cancels its call; the drop of an
//!   instruction a placement copied, or a per-shot adapter copied to merge
//!   its shots' sends; a placed executor's call or drop run
//!   on its placement's thread (its calls fail instead), and each release
//!   of its call queue (by a job, or as it is dropped), which, the last,
//!   frees the executor it places, whatever that executor's drop does;
//!   and, as an executor of the crate is dropped, the states it holds
//!   ([`HeldStates`]), then each of its other parts, the implementer's
//!   closures (with what they capture) and a placement ([`HeldPart`]);
//!   and each reply an adapter's call below gives, once that call ran, as
//!   it is dropped, ended or not ([`HeldPart`]).
//!   So no executor of the crate panics in `drop` (C-DTOR-FAIL): `close`
//!   raises a state's drop panic. The panic hook reported the panic
//!   already, so its payload is dropped.
//! - Raised again once every value is dropped: the states an executor
//!   frees together in a call or at `close`, whose drops are the
//!   implementer's code, so that the first one's panic reaches the caller,
//!   as one value's would. They are the states made for a call that is
//!   refused (the state closure refused a later shot) or whose logic
//!   failed, and all of them at close. Freed while the thread unwinds
//!   another panic (the logic's, or a report closure's), they raise
//!   none: that panic goes on. An adapter's close raises it, or its
//!   report closure's, only once it closed below.
//!
//! Each value is dropped in a catch of its own, since a collection's drop
//! goes on dropping the others during the first one's unwind, where a
//! second panic aborts the process.

use std::any::Any;
use std::ops::{Deref, DerefMut};
use std::panic::{self, AssertUnwindSafe};
use std::task::Waker;
use std::{mem, thread};

use crate::reply::{Reply, ReplyStatus};

/// A caught panic's payload.
pub(crate) type Panic = Box<dyn Any + Send>;

/// Runs `code`, absorbing its panic; returns whether `code` returned.
pub(crate) fn absorb(code: impl FnOnce()) -> bool {
    // Known issue, open (it needs two bugs at once; blocks neither this
    // commit nor a later one): a payload whose own drop panics
    // (`panic_any`) is dropped here, outside the catch, and its panic
    // escapes. So it is wherever this module drops a payload.
    panic::catch_unwind(AssertUnwindSafe(code)).is_ok()
}

/// Runs `code`, catching its panic, for the caller to raise once it did
/// what must follow ([`raise`]).
pub(crate) fn caught(code: impl FnOnce()) -> Option<Panic> {
    panic::catch_unwind(AssertUnwindSafe(code)).err()
}

/// Raises `panic` again, if any, unless the thread unwinds another panic,
/// which goes on: raising a second one would abort the process.
pub(crate) fn raise(panic: Option<Panic>) {
    if let Some(panic) = panic
        && !thread::panicking()
    {
        panic::resume_unwind(panic);
    }
}

/// Runs `end` on each of `values`, each in a catch of its own, and returns
/// the first panic; a later one's payload is dropped, as in [`absorb`].
fn end_each<T>(values: impl IntoIterator<Item = T>, mut end: impl FnMut(T)) -> Option<Panic> {
    let mut first = None;
    for value in values {
        if let Err(panic) = panic::catch_unwind(AssertUnwindSafe(|| end(value))) {
            first.get_or_insert(panic);
        }
    }
    first
}

/// Runs `end` on each of `values`, absorbing each one's panic in a catch
/// of its own.
pub(crate) fn absorb_each<T>(values: impl IntoIterator<Item = T>, end: impl FnMut(T)) {
    drop(end_each(values, end));
}

/// Runs `end` on each of `values`, each in a catch of its own, then raises
/// again the first panic, once every value ended: `earlier`, when given (a
/// panic caught before the values were freed), else the first `end`'s; the
/// others are left to the panic hook, which reported them. While the
/// thread unwinds another panic, it raises none, which would abort the
/// process: that panic goes on.
pub(crate) fn end_each_raising_first<T>(
    earlier: Option<Panic>,
    values: impl IntoIterator<Item = T>,
    end: impl FnMut(T),
) {
    let later = end_each(values, end);
    raise(earlier.or(later));
}

/// Values that are freed one by one by `free` (`drop`, unless given), each
/// in a catch of its own, the first one's panic raised again once all are
/// freed ([`end_each_raising_first`]): the states a call frees (made for
/// it, and not kept), or `close`. A `Vec` otherwise.
#[derive(Debug)]
pub(crate) struct DroppedOneByOne<T, F: FnMut(T) = fn(T)> {
    values: Vec<T>,
    free: F,
}

impl<T> Default for DroppedOneByOne<T> {
    fn default() -> Self {
        Self::from(Vec::new())
    }
}

impl<T> From<Vec<T>> for DroppedOneByOne<T> {
    fn from(values: Vec<T>) -> Self {
        Self { values, free: drop }
    }
}

impl<T, F: FnMut(T)> DroppedOneByOne<T, F> {
    /// None yet, each to be freed by `free`: for states that a plain drop
    /// would not free where the caller's rule says (a placed state, whose
    /// drop absorbs the panic of its own).
    pub(crate) fn freed_by(free: F) -> Self {
        Self {
            values: Vec::new(),
            free,
        }
    }

    /// Frees every value now, each in a catch of its own, and gives the
    /// first panic, for the caller to raise once it did what must follow
    /// ([`raise`]): an adapter's close, which closes below once its states
    /// are freed, whatever their drops did.
    pub(crate) fn free_keeping_first(mut self) -> Option<Panic> {
        end_each(mem::take(&mut self.values), &mut self.free)
    }
}

impl<T, F: FnMut(T)> Deref for DroppedOneByOne<T, F> {
    type Target = Vec<T>;

    fn deref(&self) -> &Vec<T> {
        &self.values
    }
}

/// A value taken out through it (`pop`, `drain`, `append` to another
/// guard) leaves the guard's rule: keep it, or move it into another guard.
/// Never free values through it (`clear`, `truncate`, a dropped `pop`):
/// they would drop as a `Vec` does, where two panicking drops abort.
impl<T, F: FnMut(T)> DerefMut for DroppedOneByOne<T, F> {
    fn deref_mut(&mut self) -> &mut Vec<T> {
        &mut self.values
    }
}

impl<T, F: FnMut(T)> Drop for DroppedOneByOne<T, F> {
    fn drop(&mut self) {
        if !self.values.is_empty() {
            end_each_raising_first(None, mem::take(&mut self.values), &mut self.free);
        }
    }
}

/// The states an executor holds: dropped with it one by one, each in a
/// catch of its own, every panic absorbed ([`absorb_each`]), so that its
/// `drop` never panics; `close` takes them out
/// ([`HeldStates::take_raising_first`]) to raise the first panic. A `Vec`
/// otherwise.
#[derive(Debug)]
pub(crate) struct HeldStates<T>(Vec<T>);

impl<T> Default for HeldStates<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T> From<Vec<T>> for HeldStates<T> {
    fn from(values: Vec<T>) -> Self {
        Self(values)
    }
}

impl<T> HeldStates<T> {
    /// Every state, taken out into a guard that raises the first drop's
    /// panic once all are dropped ([`DroppedOneByOne`]): for `close`.
    pub(crate) fn take_raising_first(&mut self) -> DroppedOneByOne<T> {
        DroppedOneByOne::from(mem::take(&mut self.0))
    }
}

impl<T> Deref for HeldStates<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Vec<T> {
        &self.0
    }
}

/// As for [`DroppedOneByOne`]: a state taken out through it leaves the
/// guard's rule, and none is freed through it.
impl<T> DerefMut for HeldStates<T> {
    fn deref_mut(&mut self) -> &mut Vec<T> {
        &mut self.0
    }
}

impl<T> Drop for HeldStates<T> {
    fn drop(&mut self) {
        absorb_each(mem::take(&mut self.0), drop);
    }
}

/// A part an executor holds besides its states: a closure of the
/// implementer's (the state closure, the logic, the check, a report
/// closure), with what it captures, or a placement. Dropped with the
/// executor in a catch of its own, its panic absorbed ([`absorb`]), so
/// that the executor's `drop` never panics, during an unwind too, where
/// two parts' panics in one catch would abort the process. Otherwise it
/// derefs to the part.
///
/// It also holds each reply an adapter's call below gives, from the moment
/// the call returns it, so that dropping it, ended or not, absorbs its
/// panic: two pending replies dropped with a call's reply, or as the
/// logic's panic unwinds, would abort the process otherwise. It is a
/// reply, polling the one it holds.
///
/// Declared after what holds the executor's states, so that its states
/// are dropped first, as `close` frees them first: a state dropped in
/// place is freed while the closures that made it and run it are still
/// held, so that a state whose drop reaches its device through what a
/// closure holds (a weak handle to the device's connection, say) still
/// finds it there. A placed state is dropped on its placement, which
/// `drop` does not wait for; there, its shot's executor holds the shot's
/// copies of the logic and the report closures, declared after its
/// state. The order is kept by the fields' declaration order alone, which
/// the compiler does not check; the public docs promise no order. For the
/// executors (`ExecutorFn`, `OnePerShotExecutor`,
/// `OnePerShotPlacedExecutor` and its shots' executors), no test pins it.
/// For the adapters, tests pin it
/// (`dropped_it_frees_its_state_before_its_closures_and_the_executor_below`,
/// in `src/tests/adapter/adapter_fn.rs`, and
/// `dropped_it_frees_its_states_before_its_closures_and_the_executor_below`,
/// in `src/tests/adapter/adapter_fn/one_per_shot.rs`).
pub(crate) struct HeldPart<T>(Option<T>);

impl<T> HeldPart<T> {
    pub(crate) fn new(part: T) -> Self {
        Self(Some(part))
    }
}

impl<T> Deref for HeldPart<T> {
    type Target = T;

    fn deref(&self) -> &T {
        (self.0.as_ref()).expect("a part is held until its holder is dropped")
    }
}

impl<T> DerefMut for HeldPart<T> {
    fn deref_mut(&mut self) -> &mut T {
        (self.0.as_mut()).expect("a part is held until its holder is dropped")
    }
}

impl<R: Reply> Reply for HeldPart<R> {
    type Item = R::Item;

    fn poll_events(&mut self, out: &mut Vec<R::Item>, waker: &Waker) -> ReplyStatus {
        (**self).poll_events(out, waker)
    }
}

impl<T> Drop for HeldPart<T> {
    fn drop(&mut self) {
        let part = self.0.take();
        absorb(|| drop(part));
    }
}
