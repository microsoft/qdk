// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a layer is, and how layers and backends stack. Nothing in this module
//! knows about qodecs: it is the plumbing, and [`protocol`](crate::protocol)
//! says what flows through it.
//!
//! Data flows down as arguments and up as return values: a layer answers an
//! input by calling the backend below it, as often as it needs, and returning.
//!
//! ```text
//!  ┌─────────────────┐                  ┌─────────────────┐
//!  │ Input           │                  │ Output          │
//!  └─┬───────────────┘                  └─▲───────────────┘
//!    │ execute(input, below)              │ return
//!  ┌─▼────────────────────────────────────┴───────────────┐
//!  │                         Layer                        │
//!  └─┬────────────────────────────────────▲───────────────┘
//!    │ below.execute(..), 0..n times      │ return
//!  ┌─▼───────────────┐                  ┌─┴───────────────┐
//!  │ LowerInput      │                  │ LowerOutput     │
//!  └─────────────────┘                  └─────────────────┘
//! ```
//!
//! [`Stack`] puts a layer on a [`Backend`], which makes a backend, or on
//! another layer, which makes a layer. [`Chain`] stacks any number of layers
//! that speak the same types above and below. Composites compose further.

use super::backend::Backend;

/// A component that answers each input by calling the backend below it as
/// often as it needs.
pub trait Layer {
    /// What it is given.
    type Input;
    /// What it answers.
    type Output;
    /// What it gives the backend below.
    type LowerInput;
    /// What the backend below answers.
    type LowerOutput;

    /// Answers `input`, calling `below` as often as it needs.
    fn execute(
        &mut self,
        input: Self::Input,
        below: &mut dyn Backend<Self::LowerInput, Self::LowerOutput>,
    ) -> Self::Output;
}

impl<L: Layer + ?Sized> Layer for Box<L> {
    type Input = L::Input;
    type Output = L::Output;
    type LowerInput = L::LowerInput;
    type LowerOutput = L::LowerOutput;

    fn execute(
        &mut self,
        input: Self::Input,
        below: &mut dyn Backend<Self::LowerInput, Self::LowerOutput>,
    ) -> Self::Output {
        (**self).execute(input, below)
    }
}

// ── Stack ───────────────────────────────────────────────────────────────────

/// `upper` on top of `lower`. The types must agree at the seam, so a
/// mismatched pair does not compile.
///
/// ```text
///   │ A::Input             ▲ A::Output
///  ┌▼──────────────────────┴┐
///  │ upper: A (a Layer)     │
///  └┬──────────────────────▲┘
///   │ A::LowerInput        │ A::LowerOutput
///  ┌▼──────────────────────┴┐
///  │ lower: B               │   a Backend: the stack is a Backend
///  └────────────────────────┘   a Layer:   the stack is a Layer
/// ```
///
/// A stack of [`Blueprint`]s is a blueprint of the stack of their instances.
#[derive(Clone, Debug, Default)]
pub struct Stack<A, B> {
    /// The layer on top.
    pub upper: A,
    /// The backend or layer below it.
    pub lower: B,
}

impl<A, B> Stack<A, B> {
    /// `upper` on top of `lower`.
    pub fn new(upper: A, lower: B) -> Self {
        Self { upper, lower }
    }
}

/// A layer on a backend is a backend.
impl<A, B> Backend<A::Input, A::Output> for Stack<A, B>
where
    A: Layer,
    B: Backend<A::LowerInput, A::LowerOutput>,
{
    fn execute(&mut self, input: A::Input) -> A::Output {
        self.upper.execute(input, &mut self.lower)
    }
}

/// A layer on a layer is a layer.
impl<A, B> Layer for Stack<A, B>
where
    A: Layer,
    B: Layer<Input = A::LowerInput, Output = A::LowerOutput>,
{
    type Input = A::Input;
    type Output = A::Output;
    type LowerInput = B::LowerInput;
    type LowerOutput = B::LowerOutput;

    fn execute(
        &mut self,
        input: Self::Input,
        below: &mut dyn Backend<Self::LowerInput, Self::LowerOutput>,
    ) -> Self::Output {
        let mut lower = Over {
            layer: &mut self.lower,
            below,
        };
        self.upper.execute(input, &mut lower)
    }
}

/// `layer` on `below`, seen as one backend.
struct Over<'a, L: Layer + ?Sized> {
    layer: &'a mut L,
    below: &'a mut dyn Backend<L::LowerInput, L::LowerOutput>,
}

impl<L: Layer + ?Sized> Backend<L::Input, L::Output> for Over<'_, L> {
    fn execute(&mut self, input: L::Input) -> L::Output {
        self.layer.execute(input, &mut *self.below)
    }
}

// ── Chain ───────────────────────────────────────────────────────────────────

/// A boxed layer that speaks `I` and `O` both above and below, so any number of
/// them can be chained.
pub type Link<I, O> = Box<dyn Layer<Input = I, Output = O, LowerInput = I, LowerOutput = O>>;

/// Any number of layers stacked in order, the first on top. It is a layer
/// itself. Use it when the number of layers is only known at run time, as with
/// the layers of a qodec; use [`Stack`] otherwise. An empty chain passes inputs
/// straight to the backend below.
pub struct Chain<I, O> {
    links: Vec<Link<I, O>>,
}

impl<I, O> Chain<I, O> {
    /// Chains `links`, the first on top.
    #[must_use]
    pub fn new(links: Vec<Link<I, O>>) -> Self {
        Self { links }
    }

    /// The number of layers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.links.len()
    }

    /// Whether the chain has no layers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }
}

impl<I, O> Layer for Chain<I, O> {
    type Input = I;
    type Output = O;
    type LowerInput = I;
    type LowerOutput = O;

    fn execute(&mut self, input: I, below: &mut dyn Backend<I, O>) -> O {
        Rest {
            links: &mut self.links,
            below,
        }
        .execute(input)
    }
}

/// The links of a chain from some point down, over the backend below the
/// chain, seen as one backend.
struct Rest<'a, I, O> {
    links: &'a mut [Link<I, O>],
    below: &'a mut dyn Backend<I, O>,
}

impl<I, O> Backend<I, O> for Rest<'_, I, O> {
    fn execute(&mut self, input: I) -> O {
        match self.links.split_first_mut() {
            None => self.below.execute(input),
            Some((first, links)) => first.execute(
                input,
                &mut Rest {
                    links,
                    below: &mut *self.below,
                },
            ),
        }
    }
}

// ── Blueprints ──────────────────────────────────────────────────────────────

/// Shot-independent data from which a fresh component is built for every
/// shot. Planning is the expensive part, done once; instantiating is cheap.
pub trait Blueprint {
    /// The component it builds.
    type Instance;
    /// Builds a fresh component.
    fn instantiate(&self) -> Self::Instance;
}

impl<A: Blueprint, B: Blueprint> Blueprint for Stack<A, B> {
    type Instance = Stack<A::Instance, B::Instance>;

    fn instantiate(&self) -> Self::Instance {
        Stack::new(self.upper.instantiate(), self.lower.instantiate())
    }
}

#[cfg(test)]
mod tests {
    use super::{Chain, Layer, Link, Stack};
    use crate::backend::{Backend, from_fn};

    /// Adds `step` to every number going down, and subtracts it from every
    /// answer coming up. Answers 0 with -1 itself, without calling below.
    struct Shift {
        step: i64,
    }

    impl Layer for Shift {
        type Input = i64;
        type Output = i64;
        type LowerInput = i64;
        type LowerOutput = i64;

        fn execute(&mut self, m: i64, below: &mut dyn Backend<i64, i64>) -> i64 {
            if m == 0 {
                return -1;
            }
            below.execute(m + self.step) - self.step
        }
    }

    /// Calls below once per unit of its input, and sums the answers.
    struct Repeat;

    impl Layer for Repeat {
        type Input = u32;
        type Output = i64;
        type LowerInput = i64;
        type LowerOutput = i64;

        fn execute(&mut self, n: u32, below: &mut dyn Backend<i64, i64>) -> i64 {
            (0..n).map(|_| below.execute(1)).sum()
        }
    }

    fn doubler() -> impl Backend<i64, i64> {
        from_fn(|m: i64| 2 * m)
    }

    #[test]
    fn a_layer_on_a_backend_is_a_backend() {
        let mut stack = Stack::new(Shift { step: 1 }, doubler());
        // 1 → 2 → doubled to 4 → 3 above.
        assert_eq!(stack.execute(1), 3);
        assert_eq!(stack.execute(0), -1);
    }

    #[test]
    fn a_layer_on_a_layer_is_a_layer() {
        let layers = Stack::new(Shift { step: 1 }, Shift { step: 10 });
        // 1 → 2 → 12 → doubled to 24 → 14 → 13.
        assert_eq!(Stack::new(layers, doubler()).execute(1), 13);
    }

    #[test]
    fn layers_call_below_as_often_as_they_need() {
        let mut calls = 0;
        let counter = from_fn(|m: i64| {
            calls += 1;
            m
        });
        assert_eq!(Stack::new(Repeat, counter).execute(3), 3);
        assert_eq!(calls, 3);
    }

    #[test]
    fn chains_compose_like_stacks() {
        let links: Vec<Link<i64, i64>> =
            vec![Box::new(Shift { step: 1 }), Box::new(Shift { step: 10 })];
        assert_eq!(Stack::new(Chain::new(links), doubler()).execute(1), 13);
        let empty: Chain<i64, i64> = Chain::new(Vec::new());
        assert_eq!(Stack::new(empty, doubler()).execute(7), 14);
    }
}
