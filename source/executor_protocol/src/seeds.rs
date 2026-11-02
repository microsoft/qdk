// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Seeds: each shot's seed ([`shot_seed`]), and the seeds of the components
//! that draw random numbers in a shot ([`ShotContext`], [`ComponentId`]),
//! which a chain checks are distinct when it is built ([`RngIds`]).
//!
//! A chain has one seed, the run's: the one its last executor is given
//! ([`ExecutorFn::seed`](crate::ExecutorFn::seed)), which draws with
//! [`ComponentId::LAST_EXECUTOR`]. It reaches every adapter above that
//! draws, each under an id of its own ([`ComponentId::named`]): an
//! [`adapter_fn`](fn@crate::adapter_fn) given
//! [`AdapterFn::drawing`](crate::AdapterFn::drawing), and an adapter
//! written by hand, which is given its seeds, bound to its id, when built
//! ([`AdapterSeeds`]); so the one seed replays the whole chain. For example
//! a noise model (planned: the simulators' Pauli noise, `NoiseConfig` in
//! `source/simulators`, as an adapter upstream of any simulator given the
//! run's seed, [`ExecutorFn::seed`](crate::ExecutorFn::seed);
//! `an_adapter_drawing_with_no_run_seed_below_is_refused`). The ids
//! drawn with ([`RngIds`]) are checked distinct when the chain is built,
//! and an adapter that draws is refused where no run's seed reaches it
//! ([`BuildError::NoRunSeed`]).

use std::fmt;

use crate::{BuildError, ShotId};

/// The seed of shot `j` of a run with seed `seed`: the same whatever
/// executor or process runs the shot, so that any split of the shots gives
/// each shot the same draws.
///
/// It is one step of `SplitMix64` (Steele, Lea and Flood, "Fast splittable
/// pseudorandom number generators", OOPSLA 2014) applied to
/// `SplitMix64(seed) ^ j`:
///
/// - Two shots of a run never share a seed: a `SplitMix64` step and the
///   XOR with a constant are bijections of `u64`, so `j ↦ shot_seed(seed, j)`
///   is one-to-one. A hash, or a draw from a master generator, would only
///   make a collision unlikely, and two shots with one seed draw alike.
/// - The seeds do not change with a dependency's version: the constants are
///   `SplitMix64`'s own, fixed here.
///
/// It is the seed of the last executor's random numbers in the shot,
/// [`ShotContext::seed`].
#[must_use]
pub const fn shot_seed(seed: u64, j: ShotId) -> u64 {
    splitmix(splitmix(seed) ^ j as u64)
}

/// One step of `SplitMix64`, with its own constants: a bijection of `u64`.
const fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The name a component of a chain draws random numbers under: what selects
/// its seed in each shot.
///
/// Needed because with one generator per shot, an adapter that draws (noise)
/// would shift the last executor's draws, so a shot's outcomes would depend on
/// which adapters are in the chain. With a seed per component, component `c`
/// of shot `j` draws from its own seed, whatever else is in the chain, and
/// inserting an adapter that draws without effect changes no outcome.
///
/// Ids are stable names, not positions in the chain, so that adding a
/// component does not change the draws of the others. The last executor's
/// id is fixed; any other is a hash of a name the builder chooses
/// ([`ComponentId::named`]). It is 32 bits so that a (shot, component) pair
/// fits one `u64`, which keeps [`ShotContext::rng_seed`] one-to-one; the
/// price is that two of k names share an id with probability about
/// k²/2³³ (the birthday bound for a 32-bit hash), which a chain refuses
/// when it is built ([`BuildError::RngInUse`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ComponentId(u32);

impl ComponentId {
    /// The last executor's: its seed in shot `j` is [`shot_seed`]`(seed, j)`.
    pub const LAST_EXECUTOR: Self = Self(0);

    /// The id of the component named `name`: FNV-1a 64 (Fowler, Noll and Vo)
    /// over its UTF-8 bytes, XOR-folded to 32 bits. Its constants are fixed
    /// here, so an id, and the seeds it selects, do not change with a
    /// dependency's version. A name that folds to 0 gets 1, since 0 is the
    /// last executor's.
    #[must_use]
    pub const fn named(name: &str) -> Self {
        let bytes = name.as_bytes();
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut i = 0;
        while i < bytes.len() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(0x0100_0000_01b3);
            i += 1;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the XOR-fold keeps the low 32 bits on purpose"
        )]
        let folded = (hash ^ (hash >> 32)) as u32;
        Self(if folded == 0 { 1 } else { folded })
    }

    /// The id as a number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::LAST_EXECUTOR {
            write!(f, "the last executor's random numbers")
        } else {
            write!(f, "the random numbers of component {:#010x}", self.0)
        }
    }
}

/// Shot `j`'s context, as one component of a chain sees it: what each
/// component of the shot draws from. Built only from the run's seed, `j`
/// and the component, so it is the same whichever executor or process runs
/// the shot.
///
/// Needed as a type of its own because a run's seed and a shot's seed
/// are both `u64`: a shot made from the run's seed instead of its own
/// compiles, and then every shot draws the same values. A `ShotContext`
/// names both: [`ShotContext::seed`] is the component's seed in this shot,
/// [`ShotContext::run_seed`] the run's.
///
/// It gives seeds, not generators, so that this crate depends on no random
/// number library, and a shot's seeds do not change with one's version
/// (`rand` does not promise to keep `StdRng`'s algorithm): each component
/// seeds the generator it uses, for example
/// `StdRng::seed_from_u64(shot.seed())`, which is what a method returning a
/// generator would do.
///
/// An [`executor_fn`](fn@crate::executor_fn) given the run's seed
/// ([`ExecutorFn::seed`](crate::ExecutorFn::seed)) makes shot `j`'s state
/// from `ShotContext::new(seed, j)`, whatever its shot form. Given none, it
/// makes it from a context that names the shot only, and drawing from that
/// panics. A state made before
/// [`ExecutorFn::shots`](crate::ExecutorFn::shots) and carried over keeps
/// the context it was made with. An
/// [`adapter_fn`](fn@crate::adapter_fn) that draws
/// ([`AdapterFn::drawing`](crate::AdapterFn::drawing)) makes its states
/// from the same context, bound to the id it draws with, so that its
/// `seed` is its own, the run's seed being the one given to the executor
/// below it; one that draws nothing is given no seed. An adapter written
/// by hand that draws takes shot `j`'s context from the seeds it is given
/// when built, [`AdapterSeeds::shot`], bound to its id the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShotContext {
    /// `None` where no run's seed reaches the component.
    seed: Option<u64>,
    shot: ShotId,
    /// The component whose seed [`ShotContext::seed`] is: the last
    /// executor, or an adapter that draws; `None` for an adapter that draws
    /// nothing, which is then given no seed.
    component: Option<ComponentId>,
}

impl ShotContext {
    /// Shot `shot` of a run with seed `seed`, as the last executor sees it:
    /// its [`seed`](Self::seed) is the last executor's,
    /// [`shot_seed`]`(seed, shot)`. An adapter that draws takes its own
    /// context from [`AdapterSeeds::shot`], whose `seed` is the adapter's.
    //
    // Design question, held for the user (blocks neither): a name that says
    // whose context it is.
    #[must_use]
    pub const fn new(seed: u64, shot: ShotId) -> Self {
        Self {
            seed: Some(seed),
            shot,
            component: Some(ComponentId::LAST_EXECUTOR),
        }
    }

    /// Shot `shot` of a run with seed `seed`, or, when `seed` is `None`,
    /// shot `shot` where no run's seed reaches the component: the context
    /// every form of `executor_fn` makes its states from.
    pub(crate) const fn in_run(seed: Option<u64>, shot: ShotId) -> Self {
        Self {
            seed,
            shot,
            component: Some(ComponentId::LAST_EXECUTOR),
        }
    }

    /// Shot `shot` of a run with seed `seed`, as the adapter that draws with
    /// `drawing` sees it: the context every form of `adapter_fn` makes its
    /// states from. An adapter that draws nothing (`None`) is given no seed.
    pub(crate) const fn of_adapter(
        drawing: Option<ComponentId>,
        seed: Option<u64>,
        shot: ShotId,
    ) -> Self {
        match drawing {
            Some(id) => Self {
                seed,
                shot,
                component: Some(id),
            },
            None => Self {
                seed: None,
                shot,
                component: None,
            },
        }
    }

    /// The component's seed in this shot: [`ShotContext::rng_seed`] of its
    /// id. The last executor's is [`shot_seed`]`(run_seed, j)`, its id being
    /// [`ComponentId::LAST_EXECUTOR`]. What a state seeds its random
    /// numbers with.
    ///
    /// # Panics
    ///
    /// When no run's seed reaches the component, as
    /// [`ShotContext::run_seed`].
    #[must_use]
    pub const fn seed(&self) -> u64 {
        match self.component {
            Some(id) => self.rng_seed(id),
            None => self.run_seed(),
        }
    }

    /// The run's seed.
    ///
    /// # Panics
    ///
    /// When no run's seed reaches the component, since a fixed seed made up
    /// for it would draw the same values in every run, and one drawn from
    /// the system could not be replayed: a state of an
    /// [`executor_fn`](fn@crate::executor_fn) not given one
    /// ([`ExecutorFn::seed`](crate::ExecutorFn::seed)), whatever its shot
    /// form; and a state of an [`adapter_fn`](fn@crate::adapter_fn) that
    /// states no id it draws with
    /// ([`AdapterFn::drawing`](crate::AdapterFn::drawing)). An adapter that
    /// states one is refused when no run's seed reaches it
    /// ([`BuildError::NoRunSeed`]), so it never gets here. Where the
    /// state is made on a placement's thread, the panic fails the call's
    /// reply, as any panic in `call` there does.
    ///
    /// Known issue, open (blocks neither this commit nor a later one): a
    /// failed reply's reason is then `the executor panicked during the
    /// call`, without this panic's message, which only the panic hook
    /// prints (on stderr). Carrying it would change the reason every later
    /// call is refused with, on both placements.
    #[must_use]
    pub const fn run_seed(&self) -> u64 {
        match (self.seed, self.component) {
            (Some(seed), _) => seed,
            (None, Some(_)) => panic!(
                "no run's seed reaches this state: an executor_fn whose states draw \
                 is given the run's seed with .seed(seed)"
            ),
            (None, None) => panic!(
                "this state draws under no id: an adapter_fn whose states draw states \
                 the id it draws with, .drawing(id)"
            ),
        }
    }

    /// The shot, `j`.
    #[must_use]
    pub const fn shot(&self) -> ShotId {
        self.shot
    }

    /// The seed of component `c` in this shot: one step of `SplitMix64`
    /// applied to `SplitMix64(seed) ^ (c · 2³² + j)`, the construction of
    /// [`shot_seed`] with the component in the high half.
    ///
    /// - The last executor's is [`shot_seed`]`(seed, j)` exactly, since its
    ///   id is 0: [`ShotContext::seed`].
    /// - No two (shot, component) pairs of a run share a seed:
    ///   `c · 2³² + j` is one-to-one over 32-bit shots and ids, and the rest
    ///   is a bijection of `u64`.
    /// - Not guaranteed: seeds of different runs may meet, as for
    ///   [`shot_seed`], and two names may share an id ([`ComponentId`]).
    ///
    /// A component draws from its own [`seed`](Self::seed); another's id is
    /// for tests' expected values.
    ///
    /// # Panics
    ///
    /// When no run's seed reaches the component, as
    /// [`ShotContext::run_seed`].
    #[must_use]
    pub const fn rng_seed(&self, c: ComponentId) -> u64 {
        splitmix(splitmix(self.run_seed()) ^ ((c.0 as u64) << 32 | self.shot as u64))
    }
}

/// The seeds of an adapter that draws: the run's seed and the id the
/// adapter draws with ([`Adapter::draws`](crate::Adapter::draws)), which
/// [`compose`](fn@crate::compose) gives an adapter written by hand when it
/// builds it
/// ([`AdapterBoundary::drawing_seeds`](crate::AdapterBoundary::drawing_seeds)).
///
/// Needed so that the adapter draws from its own seeds, under the id it states
/// once, in `draws`: with the run's seed alone it would make each shot's
/// context with [`ShotContext::new`], whose `seed` is the last executor's, and
/// its draws would then repeat the executor's: a readout error of ½ over fair
/// coins, drawing as the coins do, flips exactly the coins that come up One,
/// and every readout reads Zero. The seeds bind the id, so the adapter's shot
/// `j` draws from the `seed` of [`AdapterSeeds::shot`], its own, the one an
/// [`adapter_fn`](fn@crate::adapter_fn) drawing with that id has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AdapterSeeds {
    run_seed: u64,
    id: ComponentId,
}

impl AdapterSeeds {
    /// The seeds of the adapter that draws with `id`, in a run with seed
    /// `run_seed`.
    pub(crate) const fn new(run_seed: u64, id: ComponentId) -> Self {
        Self { run_seed, id }
    }

    /// Shot `shot`'s context as the adapter sees it: its
    /// [`seed`](ShotContext::seed) is the adapter's own in that shot,
    /// [`ShotContext::rng_seed`] of the adapter's id.
    #[must_use]
    pub const fn shot(self, shot: ShotId) -> ShotContext {
        ShotContext::of_adapter(Some(self.id), Some(self.run_seed), shot)
    }
}

/// The ids of the components that draw random numbers at or downstream of a
/// component of a chain
/// ([`Capabilities::rng_ids`](crate::Capabilities::rng_ids)): what a
/// component that draws checks its own id against when it is built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RngIds(Vec<ComponentId>);

impl RngIds {
    /// The last executor's: its own id only.
    #[must_use]
    pub fn last_executor() -> Self {
        Self(vec![ComponentId::LAST_EXECUTOR])
    }

    /// Whether one of these components draws with `id`.
    #[must_use]
    pub fn contains(&self, id: ComponentId) -> bool {
        self.0.contains(&id)
    }

    /// These and `id`, for a component that draws with `id` upstream of them.
    ///
    /// # Errors
    ///
    /// [`BuildError::RngInUse`] when a component downstream already draws with
    /// `id`: the two would draw the same numbers.
    pub fn with(mut self, id: ComponentId) -> Result<Self, BuildError> {
        if self.contains(id) {
            return Err(BuildError::RngInUse(id));
        }
        self.0.push(id);
        Ok(self)
    }

    /// The ids: the last executor's first, then upstream from it.
    #[must_use]
    pub fn as_slice(&self) -> &[ComponentId] {
        &self.0
    }
}

impl Default for RngIds {
    /// The last executor's.
    fn default() -> Self {
        Self::last_executor()
    }
}
