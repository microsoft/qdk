// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The backend interface: something that answers each message it is given.
//! A qodec runtime is one, and drives one: it takes batches of top-ISA calls,
//! and sends batches of bottom-ISA calls to a backend that runs them.

/// Answers each input it is given, synchronously.
///
/// The trait assumes nothing about `I` and `O`: each user states what it needs
/// in its bounds. For example, a qodec runtime drives a
/// `Backend<Vec<Call>, Result<Vec<BasicReadout>, E>>` (see
/// [`Call`](crate::protocol::Call) and
/// [`BasicReadout`](crate::protocol::BasicReadout)), and documents what it
/// expects of it there.
pub trait Backend<I, O> {
    /// Answers `input`.
    fn execute(&mut self, input: I) -> O;
}

/// A borrowed backend, e.g. one that is reset and reused across shots.
impl<I, O, B: Backend<I, O> + ?Sized> Backend<I, O> for &mut B {
    fn execute(&mut self, input: I) -> O {
        (**self).execute(input)
    }
}

impl<I, O, B: Backend<I, O> + ?Sized> Backend<I, O> for Box<B> {
    fn execute(&mut self, input: I) -> O {
        (**self).execute(input)
    }
}

/// A backend that calls `f` (see [`from_fn`]).
#[derive(Clone, Copy, Debug)]
pub struct FromFn<F>(pub F);

impl<I, O, F: FnMut(I) -> O> Backend<I, O> for FromFn<F> {
    fn execute(&mut self, input: I) -> O {
        (self.0)(input)
    }
}

/// A backend that answers with `f`, such as a closure. (A blanket
/// implementation for closures would conflict with the one for `&mut B`.)
pub fn from_fn<I, O, F: FnMut(I) -> O>(f: F) -> FromFn<F> {
    FromFn(f)
}

#[cfg(test)]
mod tests {
    use super::{Backend, from_fn};

    #[test]
    fn closures_and_borrowed_backends_are_backends() {
        let mut calls = 0;
        let mut doubler = from_fn(|x: i32| {
            calls += 1;
            2 * x
        });
        let borrowed = &mut doubler;
        assert_eq!(borrowed.execute(3), 6);
        let mut boxed: Box<dyn Backend<i32, i32> + '_> = Box::new(borrowed);
        assert_eq!(boxed.execute(4), 8);
        drop(boxed);
        assert_eq!(calls, 2);
    }
}
