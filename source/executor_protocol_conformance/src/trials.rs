// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Trials`]: how many times a check runs its rule, each time on run
//! patterns drawn from a seed, and why; each a [`Trial`], the one a report
//! names. A check asks a trial's [`TrialPatterns`] for each choice it
//! makes; in the kit's own tests, `detection_rates` measures how often
//! the drawn trials alone catch each fake part.

use crate::nonconformance::Nonconformance;

#[cfg(test)]
pub(crate) mod detection_rates;
mod patterns;
mod trial;

pub(crate) use patterns::TrialPatterns;
pub use trial::Trial;

/// The trials a check runs: trial 0, on fixed patterns, then trials 1 to
/// `count`, each on patterns drawn from `seed` and its index; or one trial
/// alone, to replay it. A check stops at the first trial that finds the
/// part breaking a rule, or unable to be judged, and names that trial.
///
/// # Why drawn trials
///
/// A check's rule holds for any shots, batches and conditions, but a check
/// runs only some. Run patterns fixed in the kit (shots at even and odd
/// positions, kinds of condition in a fixed turn, the shots' own ids, one
/// seed for every run) can be fitted: a part built by iterating against
/// fixed runs may pass them all and break the rule elsewhere. So each
/// check runs trial 0, on fixed patterns, the script and shots as given,
/// then trials 1 and on, each drawing its patterns from the seed and its
/// index: which shots are raised on and
/// with which kinds together, how the shots are ordered and split into
/// batches and groups, their ids, where a run fails, and the seed of its
/// runs, so that a part that keeps the seed of an earlier run, as a
/// generator seeded once at its first use would, shows.
/// A trial's patterns depend only on the seed and its index, so a report
/// names both, and [`Trials::replaying`] runs that trial alone again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trials {
    seed: u64,
    first: u32,
    last: u32,
}

impl Trials {
    /// The seed of [`Trials::default`], fixed so that a check gives the
    /// same result each time, in CI as on a desk.
    pub const DEFAULT_SEED: u64 = 0x0C0F_FEE5_EED0_0002;

    /// The drawn trials of [`Trials::default`], beyond trial 0. The broken
    /// parts of the kit's own tests that trial 0 misses are each caught by
    /// about one drawn trial in five or more (the least, by 23% of them), so
    /// 32 trials miss a given one with odds of at most about one in 4,300.
    pub const DEFAULT_COUNT: u32 = 32;

    /// The environment variable [`Trials::from_env`] reads the seed from,
    /// in decimal or in hexadecimal after `0x` or `0X`, with no sign. Spaces
    /// around the value and each `_` in it are ignored, so a seed copied
    /// from a Rust literal or a report reads as written.
    pub const SEED_VARIABLE: &str = "EXECUTOR_PROTOCOL_CONFORMANCE_SEED";

    /// The environment variable [`Trials::from_env`] reads the number of
    /// drawn trials from, written as the seed may be.
    pub const COUNT_VARIABLE: &str = "EXECUTOR_PROTOCOL_CONFORMANCE_TRIALS";

    /// Trial 0, then `count` trials drawn from `seed`.
    #[must_use]
    pub fn new(seed: u64, count: u32) -> Self {
        Self {
            seed,
            first: 0,
            last: count,
        }
    }

    /// `trial` alone, as a report named it, to replay it.
    #[must_use]
    pub fn replaying(trial: Trial) -> Self {
        Self {
            seed: trial.seed,
            first: trial.index,
            last: trial.index,
        }
    }

    /// The trials [`Trials::SEED_VARIABLE`] and [`Trials::COUNT_VARIABLE`]
    /// set, each unset one as [`Trials::default`] has it: so that CI runs
    /// the default trials, and a run with the variables set searches more
    /// seeds, or more trials, with no change to the tests.
    ///
    /// # Panics
    ///
    /// If a variable is set to a value it cannot read, not Unicode
    /// included, naming it.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_settings(|variable| match std::env::var(variable) {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(value)) => {
                panic!("{variable} is `{}`, not Unicode", value.display())
            }
        })
    }

    /// The trials that the variables read by `setting` set, as
    /// [`Trials::from_env`] says.
    pub(crate) fn from_settings(setting: impl Fn(&str) -> Option<String>) -> Self {
        let read = |variable: &str| {
            setting(variable).map(|raw| {
                let value = raw.trim().replace('_', "");
                let (digits, radix) = match value
                    .strip_prefix("0x")
                    .or_else(|| value.strip_prefix("0X"))
                {
                    Some(hex) => (hex, 16),
                    None => (value.as_str(), 10),
                };
                // `from_str_radix` reads a leading `+`, which the variables'
                // documentation does not allow.
                let parsed = u64::from_str_radix(digits, radix)
                    .ok()
                    .filter(|_| !digits.starts_with('+'));
                let number = parsed.unwrap_or_else(|| {
                    panic!("{variable} is `{raw}`, not a number in decimal or in 0x hexadecimal")
                });
                (number, raw)
            })
        };
        let count = read(Self::COUNT_VARIABLE).map_or(Self::DEFAULT_COUNT, |(count, raw)| {
            u32::try_from(count).unwrap_or_else(|_| {
                panic!(
                    "{} is `{raw}`, more trials than a u32",
                    Self::COUNT_VARIABLE
                )
            })
        });
        Self::new(
            read(Self::SEED_VARIABLE).map_or(Self::DEFAULT_SEED, |(seed, _)| seed),
            count,
        )
    }

    /// Only the trials drawn from `seed`, 1 to `count`, without trial 0:
    /// for the kit's measures of what the drawn trials catch alone.
    #[cfg(test)]
    pub(crate) fn drawn_only(seed: u64, count: u32) -> Self {
        Self {
            seed,
            first: 1,
            last: count,
        }
    }

    /// Runs `check` on each trial's patterns, in order, until one does not
    /// pass: its result, naming the trial; or a pass.
    pub(crate) fn run(
        self,
        mut check: impl FnMut(&mut TrialPatterns) -> Result<(), Nonconformance>,
    ) -> Result<(), Nonconformance> {
        let run = |trials: Self| {
            for index in trials.first..=trials.last {
                check(&mut TrialPatterns::new(Trial {
                    seed: trials.seed,
                    index,
                }))?;
            }
            Ok(())
        };
        #[cfg(test)]
        return detection_rates::measuring(self, self.first, run);
        #[cfg(not(test))]
        {
            let mut run = run;
            run(self)
        }
    }
}

impl Default for Trials {
    /// Trial 0, then [`Trials::DEFAULT_COUNT`] trials drawn from
    /// [`Trials::DEFAULT_SEED`].
    fn default() -> Self {
        Self::new(Self::DEFAULT_SEED, Self::DEFAULT_COUNT)
    }
}
