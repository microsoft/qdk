// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the logic of an [`adapter_fn`](fn@crate::adapter_fn) reaches below
//! it with: the executor downstream, called with the shots of the call the
//! logic serves, to pass a reply up ([`Below::send`]) or to read it
//! ([`Below::run`]). The logic is lent it for each call ([`Below`]), or
//! its state owns it for its life ([`BelowHandle`]), as
//! [`owning_adapter_fn`](fn@crate::owning_adapter_fn) gives it; both reach
//! below by the same rules, which the adapter's executor holds. A call
//! below that cannot go on says why with an [`AdapterError`], the error
//! the logic returns too.

use std::any::Any;
use std::cell::RefCell;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ShotOutcome;
#[cfg(doc)]
use crate::{AdapterFnReply, OnePerShotAdapter, Outcomes, Rejected, ReplyStatus, wait};

/// The executor below an [`adapter_fn`](fn@crate::adapter_fn), lent to its
/// logic for one call: what the logic [`send`](Below::send)s goes below on
/// the shots of the call the logic serves, and the reply passes up as it
/// comes, as part of the call's reply; what it [`run`](Below::run)s goes
/// below the same way, and the logic reads the reply.
///
/// It names only the instructions it takes, `O`, not the executor below,
/// so that a logic is written once for every chain it is composed into:
/// one function, whatever is below it.
///
/// A state that keeps its way below for its whole life, such as a code's
/// runtime made over a backend it owns, is given a [`BelowHandle`]
/// instead, which reaches below by the same rules while its state's logic
/// serves a call.
//
// Known issue, held for the user (blocks neither): a refusal below after
// the logic changed its state refuses the call, though the logic ran. A
// proposed fix: the logic decides, through `AdapterError`'s planned refusal
// (its refusal returned refuses the call, any other `Err` fails it).
//
// Planned, with that refusal: a way for the logic to map the index a
// refusal below names (`Rejected::Instruction`'s `at`) to its call's, as a
// code adapter written by hand does (`tests/runtime_isa.rs`,
// `a_downstream_refusal_names_the_logical_call`): it reads the refusal
// with `AdapterError::refused()` and returns its own. A code's layer needs
// a way to refuse and this mapping too, and so does the repetition code as
// a logic, whose run below names a physical instruction
// (`tests/static_isa/repetition_fn.rs`). Until then, the index passes up
// unmapped (`a_refusal_below_names_the_instruction_as_sent`).
//
// The rules of a call's calls below live in the executor's `CallsBelow`,
// which this type reaches through a `Route` (the call's shots, or, with a
// state per shot, the shot's); a `BelowHandle` reaches the same `Route`,
// lent on the executor's thread for the length of each call
// (`lend_while`), so that it adds no second implementation of the refusal,
// failure and reply rules.
//
// The lent form stays as it was, a borrow, and costs what it did (the
// `merging` rows of `benches/instruction_calls.rs`, run on both trees).
// The handle pays to be lifetime-free: two boxes and a copy of the
// call's shots per call with one state; two boxes per shot with a state
// per shot, whose sends are compared by content, as it does not know the
// call's instructions; a mark of the state that serves, two stores per
// call or per shot, so that a refusal on another thread says so; and a
// look-up per call below. It needs the
// executor below, its reply and the instructions to be `'static`, to be
// lent as `Any`. Measured on calls of one instruction that pass
// down as they are, the worst case: 1.0 to 1.5 times the lent form's time
// with one state, 1.4 to 2.4 times with a state per shot; on calls of a
// hundred, 1.0 to 1.1 and 1.0 to 1.4. The mark adds 1 to 8 percent to the
// owning rows of calls of one instruction (fastest of two to four runs on
// each tree), as much as the rows vary from run to run. Known issue, open
// (blocks neither): with a state per shot, the route could be installed
// once per call, the state it serves switched per shot.
pub struct Below<'a, O> {
    route: &'a mut dyn Route<O>,
}

/// How a logic's calls below reach the executor below, by the rules of the
/// adapter that serves the call: on the call's shots with one state; with a
/// state per shot, kept to be merged (a send) or on the shot alone (a run).
pub(crate) trait Route<O> {
    /// [`Below::send`].
    fn send(&mut self, instructions: &[O]) -> Result<(), String>;

    /// [`Below::run`].
    fn run(&mut self, instructions: &[O]) -> Result<Vec<ShotOutcome>, String>;
}

impl<'a, O> Below<'a, O> {
    /// The executor below, reached through `route`.
    pub(crate) fn new(route: &'a mut dyn Route<O>) -> Self {
        Self { route }
    }

    /// Sends `instructions` down, on the shots of the call the logic
    /// serves: the reply passes up as it comes, after the outcomes the
    /// logic writes, and beside those of the call's other sends. A logic
    /// may send several times in one call. With one state, each send is
    /// one call below, made at once. With a state per shot
    /// ([`OnePerShotAdapter`]), a shot's sends are kept, and go below
    /// merged once every shot's logic ran, rank by rank: the shot's `k`-th
    /// send with the other shots' `k`-th sends equal to it, as one call
    /// over those shots; but those the shot kept when it runs go alone,
    /// before its run ([`run`](Below::run)).
    ///
    /// The call reports a refusal below: the first call below refused,
    /// a send or a [`run`](Below::run), refuses the call with the same
    /// [`Rejected`], whatever the logic returns; one refused after an
    /// earlier call went down fails the call ([`ReplyStatus::Failed`]),
    /// since the earlier one runs. An index the refusal names, such as
    /// [`Rejected::Instruction`]'s `at`, is that of the instruction in the
    /// call below, not in the call the logic serves: the caller of a logic
    /// that sends other instructions than it is called with gets it
    /// unmapped.
    ///
    /// A call refused below is refused though its logic ran: what the
    /// logic changed in its state stays changed. A logic should therefore
    /// change its state only after `send(..)?` or `run(..)?`. With a state
    /// per shot there is no such order for a send, since `send` returns
    /// `Ok` and a merged send is refused after every shot's logic ran: a
    /// call refused there is refused with what every shot's logic changed
    /// in its kept state (planned: the refusal an [`AdapterError`] would
    /// carry, through which the logic would decide). A run, and the sends
    /// its shot kept, go below at once and are refused at `run(..)?`: there
    /// the order holds, and a call refused is refused with what the
    /// earlier shots' logic changed in their kept states.
    ///
    /// A send's reply that fails fails the call and the adapter, and one
    /// cancelled ends the call cancelled, unless it fails, and fails the
    /// adapter: when, depends on the executor below, as
    /// [`adapter_fn`](fn@crate::adapter_fn) states. The logic writes no
    /// tag of a shot that its sends read, which the reply would give
    /// twice (unchecked).
    ///
    /// # Errors
    ///
    /// When the call cannot go on, why: an earlier call below of the call
    /// was refused, as [`Rejected`] displays it, or a run's reply failed
    /// ([`Below::run`]), or a send's reply failed; or, with one state, this
    /// send was refused below, or its reply failed within its call below.
    /// Nothing goes down after. With a state per shot, only after a run:
    /// a send goes nowhere while the logic runs.
    pub fn send(&mut self, instructions: &[O]) -> Result<(), AdapterError> {
        self.route.send(instructions).map_err(AdapterError::from)
    }

    /// Runs `instructions` below, on the shots of the call the logic
    /// serves, and gives what they read, by shot, in ascending order, once
    /// their reply ended:
    /// for a logic that decides on what comes up, as a code's decoder does.
    /// Each shot's readouts come in the order the reply below gives them,
    /// since a reply may give them in any order ([`ShotOutcome`]). That is
    /// the order the shot read them when the executor below keeps it, as a
    /// simulator does. It is not when the executor below is an
    /// [`adapter_fn`](fn@crate::adapter_fn) whose logic sends, whose reply
    /// ([`AdapterFnReply`]) gives what the logic wrote before what its
    /// sends read: a logic that sends, then runs, gives the run's readouts
    /// first. Nor is it when a later send's reply gives readouts before an
    /// earlier send's reply gives its own, as a placed executor's can,
    /// whether it ended or not: those pass up before the earlier send's.
    /// The tags still match. What a run reads passes up only as the logic
    /// writes it to the [`Outcomes`]. With one state, the run is one
    /// call below on the call's shots. With a state per shot
    /// ([`OnePerShotAdapter`]), it is one call on the logic's shot alone,
    /// made at once, after the shot's sends kept so far, each alone too, so
    /// that the shot's calls below keep its order; its later sends are
    /// kept, and merged, anew. A send's panic below comes at that run, not
    /// at the send, so a catch around the send alone does not catch it.
    /// Should one of those sends panic below, and the logic catch the
    /// panic, the shot's sends after it are dropped unsent, and none goes
    /// below again; for a logic whose catch holds the calls from the send
    /// that panicked to the run, the calls below are those a one-state
    /// adapter makes with the same logic (the rules at
    /// [`OnePerShotAdapter`]).
    ///
    /// With one state, the logic sees what every shot of the call read: it
    /// writes each shot's outcomes from that shot's readouts alone, or a
    /// shot's outcomes depend on the shots it is called with, while the
    /// adapter still offers the `replayable` of the executor below (the
    /// known issue at [`AdapterFn`](crate::AdapterFn)); a logic that needs
    /// other shots' readouts is not replayable, and one that must be keeps
    /// a state per shot ([`OnePerShot`](crate::OnePerShot)).
    ///
    /// It waits as [`wait`] does, parking the thread that called the
    /// adapter until the reply below ends.
    ///
    /// The call reports a refusal below as for a [`send`](Below::send),
    /// the index it names included.
    /// A run whose reply fails or is cancelled, or is not whole (it reads
    /// a shot that did not run, or a tag of a shot twice), fails the call
    /// ([`ReplyStatus::Failed`]) and the adapter, whatever the logic
    /// returns. A tag the instructions read and the reply lacks is the
    /// logic's to find: only the logic knows what its instructions read.
    ///
    /// # Errors
    ///
    /// When the call cannot go on, why: the run was refused below, as
    /// [`Rejected`] displays it; its reply failed, was cancelled, or was
    /// not whole; or an earlier call below of the call ended it
    /// ([`Below::send`]), a send's reply that failed included, which the
    /// run checks before it goes down. Nothing goes down after.
    pub fn run(&mut self, instructions: &[O]) -> Result<Vec<ShotOutcome>, AdapterError> {
        self.route.run(instructions).map_err(AdapterError::from)
    }
}

/// Why the call an adapter's logic serves cannot go on, as a call below
/// ([`Below::send`], [`Below::run`], and their [`BelowHandle`] forms) or the
/// logic returns it: the error of an [`AdapterLogic`](crate::AdapterLogic)
/// and of an [`OwningAdapterLogic`](crate::OwningAdapterLogic).
///
/// It holds a message, read with [`message`](AdapterError::message) or
/// displayed. It converts from a `String` and a `&str`, so that a logic
/// returns `Err("…".into())` or `Err(format!(..).into())`, and `?` on a
/// call that fails with a `String` returns it; and into a `String`, for a
/// caller that keeps one. The logic's error fails the call and the
/// adapter, unless the call was refused below, as
/// [`adapter_fn`](fn@crate::adapter_fn) states.
///
/// Planned, with no break: a refusal it carries,
/// `AdapterError::refusal(Rejected)`, read with `refused()`, through which
/// a logic refuses the call it serves, the adapter living on, while
/// nothing of the call went down; once something did, the call fails, as
/// today. The cases: a code's layer whose runtime refuses a call that ran
/// nothing, which today fails the adapter; and an adapter that rewrites
/// its calls, which would map the index a refusal below names
/// ([`Rejected::Instruction`]'s `at`) to its call's, which today passes
/// up unmapped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterError {
    message: String,
}

impl AdapterError {
    /// Why the call cannot go on, as [`Display`](fmt::Display) gives it.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AdapterError {}

impl From<String> for AdapterError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for AdapterError {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_owned(),
        }
    }
}

impl From<AdapterError> for String {
    fn from(error: AdapterError) -> Self {
        error.message
    }
}

impl<O> fmt::Debug for Below<'_, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Below").finish_non_exhaustive()
    }
}

/// A handle to the executor below an
/// [`owning_adapter_fn`](fn@crate::owning_adapter_fn), which one of its
/// states owns for the state's life: the state closure is given it, to
/// keep in the state it makes, as a code's runtime keeps the backend it is
/// made over. It holds no part of what is below, and dropping it frees
/// nothing: the adapter closes the executor below. While that state's
/// logic serves a call, what it [`send`](BelowHandle::send)s and
/// [`run`](BelowHandle::run)s goes below
/// as through the [`Below`] an [`adapter_fn`](fn@crate::adapter_fn)'s
/// logic is lent: on the call's shots with one state, on the logic's shot
/// with a state per shot ([`OnePerShotAdapter`]), and a refusal or a
/// failure below refuses or fails the call and the adapter by the same
/// rules ([`Below::send`], [`Below::run`]). Should a call below panic and
/// the logic catch the panic, its later calls below go on, as through a
/// lent [`Below`].
///
/// It reaches below only while its state's logic serves a call on this
/// thread, from that logic and what it runs, but not from inside another
/// owning adapter's state's logic, which reaches below through its own.
/// Elsewhere a call through it is refused with an `Err` that says which
/// of these holds, and nothing goes below:
///
/// - its state's logic serves no call: the state closure uses it; or,
///   while another state serves a call, that state's logic, the logic of
///   an adapter below, or the executor below does; or it is kept past its
///   state's life (a state made for a call that is refused or fails is
///   dropped, and every state is dropped at close), even in a
///   thread-local's destructor. A state's own `Drop` is past its life, so
///   a state cannot reach below as it is dropped: a runtime that flushes
///   below at its end flushes in its logic instead.
/// - its state's logic serves a call on another thread: a thread the logic
///   hands it to, even one the logic waits for.
/// - its state's logic serves a call, but runs another chain inside it,
///   whose owning state's logic is serving: that logic reaches below
///   through its own. A per-shot adapter's merged send, made after its
///   logics return, is outside them, so the executor below that adapter
///   reaches below through the handle.
///
/// It never waits. Nothing fails by the refusal itself; if the logic
/// returns it, it fails the call and the adapter, as any `Err` of the
/// logic does.
///
/// It is `Send` and `Sync` whatever is below, since it holds no part of
/// it, and not `Clone`: one state owns it, and no copy of it can reach
/// below elsewhere.
///
/// The adapter's executor is `Send` when what it owns is, the executor
/// below and the states included
/// ([`owning_adapter_fn`](fn@crate::owning_adapter_fn)). At close, the
/// adapter drops its states, and with them their handles, then closes the
/// executor below.
///
/// The repetition code as a runtime that owns its backend, with its glue,
/// is worked in `tests/static_isa/code_runtime.rs`, described in
/// `docs/adapters.md` ("A state that owns its way below").
///
/// # Type parameters
///
/// - `O`: the instructions it takes, as [`Below`]'s.
pub struct BelowHandle<O> {
    state: StateId,
    serving: Serving,
    instructions: PhantomData<fn(&[O])>,
}

/// Why a call through a [`BelowHandle`] is refused while its state's logic
/// serves no call.
pub(crate) const NOT_SERVING: &str = "a BelowHandle was used while its state's logic \
     serves no call; it reaches below only from inside that logic";

/// Why a call through a [`BelowHandle`] is refused on another thread than
/// the one its state's logic serves a call on.
pub(crate) const ON_ANOTHER_THREAD: &str = "a BelowHandle was used on another thread than \
     the one its state's logic serves a call on; it reaches below only on that thread";

/// Why a call through a [`BelowHandle`] is refused inside another chain's
/// owning state's logic, which its state's logic runs.
pub(crate) const INSIDE_ANOTHER_LOGIC: &str = "a BelowHandle was used inside another chain's \
     owning logic, which its state's logic runs; there, only that logic's own way below reaches \
     below";

impl<O> BelowHandle<O> {
    /// The way below of the state `state`, lent to it while the state's
    /// logic serves a call ([`lend_while`]), on the adapter's executor
    /// that marks which of its states serves in `serving`.
    pub(crate) fn new(state: StateId, serving: &Serving) -> Self {
        Self {
            state,
            serving: Serving(Arc::clone(&serving.0)),
            instructions: PhantomData,
        }
    }
}

impl<O: 'static> BelowHandle<O> {
    /// Sends `instructions` down, on the shots of the call its state's
    /// logic serves, as [`Below::send`] does.
    ///
    /// # Errors
    ///
    /// Those of [`Below::send`]; and, outside its state's logic, why it
    /// reaches nothing there ([`BelowHandle`]).
    pub fn send(&mut self, instructions: &[O]) -> Result<(), AdapterError> {
        self.through(|route| route.send(instructions))
            .map_err(AdapterError::from)
    }

    /// Runs `instructions` below, on the shots of the call its state's
    /// logic serves, and gives what they read, as [`Below::run`] does.
    ///
    /// # Errors
    ///
    /// Those of [`Below::run`]; and, outside its state's logic, why it
    /// reaches nothing there ([`BelowHandle`]).
    pub fn run(&mut self, instructions: &[O]) -> Result<Vec<ShotOutcome>, AdapterError> {
        self.through(|route| route.run(instructions))
            .map_err(AdapterError::from)
    }

    /// `call` on the route its state's logic is served through, if that
    /// logic is the innermost serving a call on this thread; otherwise,
    /// why not.
    fn through<U>(
        &mut self,
        call: impl FnOnce(&mut dyn Route<O>) -> Result<U, String>,
    ) -> Result<U, String> {
        let mut taken = Taken::from(self.state, &self.serving).map_err(str::to_owned)?;
        let route = (taken.route.as_mut().expect(TAKEN))
            .downcast_mut::<Box<dyn OwnedRoute<O>>>()
            .expect("a state's way below takes what its adapter sends");
        call(&mut **route)
    }
}

impl<O> fmt::Debug for BelowHandle<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BelowHandle")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

/// Which state of which adapter a [`BelowHandle`] belongs to: no two
/// states of a process have the same ([`StateIds`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StateId {
    adapter: u64,
    state: u64,
}

/// The ids of the states one adapter's executor makes, each new, and
/// which of them serves a call.
pub(crate) struct StateIds {
    adapter: u64,
    /// From 1, as a state numbered 0 marks none serving ([`Serving`]).
    made: u64,
    serving: Serving,
}

impl StateIds {
    /// The ids of a new adapter's states: of none made so far.
    pub(crate) fn new() -> Self {
        static ADAPTERS: AtomicU64 = AtomicU64::new(0);
        Self {
            adapter: ADAPTERS.fetch_add(1, Ordering::Relaxed),
            made: 1,
            serving: Serving(Arc::new(AtomicU64::new(Serving::NONE))),
        }
    }

    /// Which of the states serves a call.
    pub(crate) fn serving(&self) -> &Serving {
        &self.serving
    }

    /// The id of the next state.
    pub(crate) fn next(&mut self) -> StateId {
        let id = StateId {
            adapter: self.adapter,
            state: self.made,
        };
        self.made += 1;
        id
    }
}

/// Which state of one adapter's executor serves a call, if one does: the
/// number of its id, or [`NONE`](Self::NONE). Shared by the executor and
/// its states' ways below, so that a way below used on another thread
/// while its state's logic serves can say so ([`ON_ANOTHER_THREAD`]): no
/// thread-local tells it. One state at most serves at a time, as the
/// executor serves one call at a time, and the states of a call one by
/// one.
pub(crate) struct Serving(Arc<AtomicU64>);

impl Serving {
    /// The number of no state.
    const NONE: u64 = 0;

    /// Marks `state` as serving, until [`end`](Self::end). Nothing was
    /// serving before: one adapter's executor never serves inside its own
    /// call, which holds it by `&mut self`.
    fn begin(&self, state: StateId) {
        self.0.store(state.state, Ordering::Relaxed);
    }

    /// Marks no state as serving.
    fn end(&self) {
        self.0.store(Self::NONE, Ordering::Relaxed);
    }

    /// Whether `state` serves a call, on whichever thread. Read only to
    /// say why a way below is refused, so a thread the logic does not wait
    /// for may read it late.
    fn is(&self, state: StateId) -> bool {
        self.0.load(Ordering::Relaxed) == state.state
    }
}

thread_local! {
    /// The routes lent on this thread, each to the handles of one state,
    /// innermost last: one per owning adapter whose logic is serving a
    /// call in the calls this thread is in. Kept per thread, so that a
    /// route, which owns the executor below, never leaves the thread its
    /// executor serves on, and the executor is `Send` exactly when what it
    /// owns is. A handle used once they are destroyed reaches nothing
    /// ([`Taken::from`]); an owning adapter's executor called then panics,
    /// as a thread-local it needs is gone, but keeps the executor below,
    /// which its close closes ([`lend_while`]).
    static LENT: RefCell<Vec<Lent>> = const { RefCell::new(Vec::new()) };
}

/// A route lent to the handles of a state ([`lend_while`]).
struct Lent {
    state: StateId,
    /// The route, a `Box<dyn OwnedRoute<O>>`; out while a handle calls
    /// through it.
    route: Option<Box<dyn Any>>,
}

/// Why a lent route is there: out only while a handle calls through it,
/// which puts it back, even should the call panic.
const TAKEN: &str = "a lent route is out only while a handle calls through it";

/// A lent route, out of [`LENT`] while a handle calls through it, and put
/// back when dropped, even should the call panic: then the handle reaches
/// below again, as a lent [`Below`] does.
struct Taken {
    state: StateId,
    route: Option<Box<dyn Any>>,
}

impl Taken {
    /// The route lent to the handles of `state` on this thread, if it is
    /// the innermost one, so that its state's logic is the one serving
    /// there; otherwise why not, `serving` telling another thread from no
    /// call. Once the thread's lent routes are destroyed, from a
    /// thread-local's destructor, none is lent on it.
    fn from(state: StateId, serving: &Serving) -> Result<Self, &'static str> {
        let lent_here = LENT.try_with(|lent| {
            let mut lent = lent.borrow_mut();
            let at = lent.iter().rposition(|lent| lent.state == state)?;
            if at + 1 < lent.len() {
                return Some(Err(INSIDE_ANOTHER_LOGIC));
            }
            Some(Ok(lent[at].route.take().expect(ONE_HANDLE)))
        });
        match lent_here.ok().flatten() {
            Some(Ok(route)) => Ok(Self {
                state,
                route: Some(route),
            }),
            Some(Err(why)) => Err(why),
            None if serving.is(state) => Err(ON_ANOTHER_THREAD),
            None => Err(NOT_SERVING),
        }
    }
}

/// Why a lent route is in when a handle calls through it: a state has one
/// handle, which calls by `&mut self`, so it cannot call again until the
/// route is back.
const ONE_HANDLE: &str = "a state's one way below calls through its route one call at a time";

impl Drop for Taken {
    fn drop(&mut self) {
        LENT.with_borrow_mut(|lent| {
            let found = lent.iter_mut().rev().find(|lent| lent.state == self.state);
            let lent = found.expect("a route is lent until its state's logic returns");
            lent.route = self.route.take();
        });
    }
}

/// A route an owning adapter's executor lends for the length of a call, or
/// of a shot: it owns what it reaches below with, and is given back after
/// ([`lend_while`]).
trait OwnedRoute<O>: Route<O> + Any {}

impl<O, T: Route<O> + Any> OwnedRoute<O> for T {}

/// Runs `serve` while the route `take` makes from `owner`, the part of an
/// executor that owns what the route reaches below with, is the way below
/// of the handles of `state` on this thread, and `state` is marked serving
/// in `serving`; then gives the route back to `owner` by `restore`, even
/// should `serve` panic. The route is taken only once the thread's lent
/// routes are reached: once they are destroyed, from a thread-local's
/// destructor, it panics with `owner` as it was, so that the executor can
/// still close what it owns.
pub(crate) fn lend_while<O, P, T, U>(
    serving: &Serving,
    state: StateId,
    owner: &mut P,
    take: impl FnOnce(&mut P) -> T,
    serve: impl FnOnce() -> U,
    restore: impl FnOnce(&mut P, T),
) -> U
where
    O: 'static,
    T: Route<O> + 'static,
{
    LENT.with_borrow_mut(|lent| {
        let route: Box<dyn OwnedRoute<O>> = Box::new(take(owner));
        lent.push(Lent {
            state,
            route: Some(Box::new(route)),
        });
    });
    serving.begin(state);
    let _back = Back {
        state,
        serving,
        owner,
        restore: Some(restore),
        route: PhantomData::<fn(O, T)>,
    };
    serve()
}

/// Gives the route [`lend_while`] lent back to `owner` by `restore` when
/// dropped, and marks in `serving` that its state serves no more.
struct Back<'a, O: 'static, P, T: 'static, F: FnOnce(&mut P, T)> {
    state: StateId,
    serving: &'a Serving,
    owner: &'a mut P,
    restore: Option<F>,
    route: PhantomData<fn(O, T)>,
}

impl<O: 'static, P, T: 'static, F: FnOnce(&mut P, T)> Drop for Back<'_, O, P, T, F> {
    fn drop(&mut self) {
        self.serving.end();
        let lent = LENT.with_borrow_mut(|lent| {
            let at = (lent.iter().rposition(|lent| lent.state == self.state))
                .expect("a route is lent until its state's logic returns");
            lent.remove(at)
        });
        let route = lent.route.expect(TAKEN);
        let route = (route.downcast::<Box<dyn OwnedRoute<O>>>())
            .expect("a lent route takes what its adapter sends");
        let route: Box<dyn Any> = *route;
        let route = (route.downcast::<T>())
            .expect("a lent route is the one its executor lent, given back once");
        if let Some(restore) = self.restore.take() {
            restore(self.owner, *route);
        }
    }
}
