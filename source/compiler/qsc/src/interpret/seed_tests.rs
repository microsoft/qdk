// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Seeded runs must be reproducible, every shot and evaluation must draw fresh values, and
//! measurement, noise, and classical draws must not affect one another. None of these checks
//! depend on the values that a particular seed produces or on how many values any component
//! draws.

use super::{CircuitEntryPoint, CircuitGenerationMethod, Interpreter, SimType};
use qsc_circuit::TracerConfig;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_eval::{backend::SparseSim, output::GenericReceiver, val::Value};
use qsc_passes::PackageType;

const SOURCE: &str = r"
    namespace Test {
        import Std.Diagnostics.*;
        import Std.Random.*;

        operation Coins() : Result[] {
            use qs = Qubit[64];
            ApplyToEach(H, qs);
            MResetEachZ(qs)
        }

        operation Draws() : Int[] {
            mutable draws = [];
            for _ in 1..4 {
                set draws += [DrawRandomInt(0, 1 <<< 62)];
            }
            draws
        }

        operation CoinsAfterDraws() : Result[] {
            let _ = Draws();
            Coins()
        }

        operation DrawsAfterCoins() : Int[] {
            let _ = Coins();
            Draws()
        }

        operation Branches() : Unit {
            use target = Qubit();
            for _ in 1..64 {
                use q = Qubit();
                H(q);
                if MResetZ(q) == One { X(target); } else { Z(target); }
            }
            Reset(target);
        }

        operation NoisyCoins() : Result[] {
            ConfigurePauliNoise(BitFlipNoise(0.5));
            use qs = Qubit[64];
            MResetEachZ(qs)
        }

        @EntryPoint()
        operation Main() : (Result[], Int[]) {
            (Coins(), Draws())
        }
    }
";

const SEED: u64 = 1;

/// How a shot of a run is started.
#[derive(Clone, Copy, Debug)]
enum Entry {
    /// `run` with an entry expression.
    Expr,
    /// `invoke_with_noise` with a callable.
    Callable,
}

fn interpreter() -> Interpreter {
    let sources = SourceMap::new([("test.qs".into(), SOURCE.into())], None);
    let capabilities = TargetCapabilityFlags::all();
    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
    Interpreter::new(
        sources,
        PackageType::Exe,
        capabilities,
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
        Default::default(),
    )
    .expect("interpreter should be created")
}

/// Runs shot `shot` of `operation` (a parameterless operation in `Test`).
fn shot(
    interpreter: &mut Interpreter,
    entry: Entry,
    operation: &str,
    seed: Option<u64>,
    shot: u64,
) -> Value {
    let mut sink = std::io::sink();
    let mut out = GenericReceiver::new(&mut sink);
    let value = match entry {
        Entry::Expr => interpreter.run(
            &mut out,
            Some(&format!("Test.{operation}()")),
            None,
            None,
            None,
            seed,
            shot,
            SimType::Sparse,
        ),
        Entry::Callable => {
            let callable = eval(interpreter, &format!("Test.{operation}"));
            interpreter.invoke_with_noise(
                &mut out,
                callable,
                Value::unit(),
                None,
                None,
                None,
                seed,
                shot,
                SimType::Sparse,
            )
        }
    }
    .expect("shot should succeed");
    assert_random(&value);
    value
}

fn eval(interpreter: &mut Interpreter, expr: &str) -> Value {
    let mut sink = std::io::sink();
    let mut out = GenericReceiver::new(&mut sink);
    interpreter
        .eval_fragments(&mut out, expr)
        .expect("evaluation should succeed")
}

fn assert_random(value: &Value) {
    // Guards the comparisons in these tests against passing vacuously.
    if let Value::Array(items) = value {
        assert!(items.iter().any(|item| *item != items[0]), "{value}");
    }
}

const ENTRIES: [Entry; 2] = [Entry::Expr, Entry::Callable];

#[test]
fn seeded_runs_are_reproducible() {
    let mut interpreter = interpreter();
    for entry in ENTRIES {
        for operation in ["Coins", "Draws", "NoisyCoins"] {
            assert_eq!(
                shot(&mut interpreter, entry, operation, Some(SEED), 0),
                shot(&mut interpreter, entry, operation, Some(SEED), 0),
                "{entry:?} {operation}"
            );
        }
    }
}

#[test]
fn shots_and_nearby_seeds_draw_fresh_values() {
    let mut interpreter = interpreter();
    for entry in ENTRIES {
        for operation in ["Coins", "Draws", "NoisyCoins"] {
            let second = shot(&mut interpreter, entry, operation, Some(SEED), 1);
            let label = format!("{entry:?} {operation}");
            assert_ne!(
                second,
                shot(&mut interpreter, entry, operation, Some(SEED), 0),
                "{label}"
            );
            assert_ne!(
                second,
                shot(&mut interpreter, entry, operation, Some(SEED + 1), 0),
                "{label}"
            );
        }
    }
}

#[test]
fn interpreter_seeds_apply_to_each_shot() {
    let mut interpreter = interpreter();
    interpreter.set_quantum_seed(Some(SEED));
    interpreter.set_classical_seed(Some(SEED));
    for entry in ENTRIES {
        for operation in ["Coins", "Draws"] {
            let first = shot(&mut interpreter, entry, operation, None, 0);
            let label = format!("{entry:?} {operation}");
            assert_eq!(
                first,
                shot(&mut interpreter, entry, operation, None, 0),
                "{label}"
            );
            assert_ne!(
                first,
                shot(&mut interpreter, entry, operation, None, 1),
                "{label}"
            );
        }
    }
}

#[test]
fn entry_point_shots_draw_fresh_reproducible_values() {
    let mut interpreter = interpreter();
    interpreter.set_quantum_seed(Some(SEED));
    interpreter.set_classical_seed(Some(SEED));
    let mut entry_shot = |shot| {
        let mut sink = std::io::sink();
        let mut out = GenericReceiver::new(&mut sink);
        interpreter
            .eval_entry_with_sim(&mut SparseSim::new(), &mut out, shot)
            .expect("entry point should succeed")
    };
    let first = entry_shot(0);
    assert_eq!(first, entry_shot(0));
    let Value::Tuple(first, _) = first else {
        panic!("entry point should return a tuple");
    };
    let Value::Tuple(second, _) = entry_shot(1) else {
        panic!("entry point should return a tuple");
    };
    for (first, second) in first.iter().zip(second.iter()) {
        assert_random(first);
        assert_ne!(first, second);
    }
}

#[test]
fn measurement_and_classical_streams_are_independent() {
    let mut interpreter = interpreter();
    for entry in ENTRIES {
        for n in 0..4 {
            assert_eq!(
                shot(&mut interpreter, entry, "Coins", Some(SEED), n),
                shot(&mut interpreter, entry, "CoinsAfterDraws", Some(SEED), n),
                "{entry:?}: classical draws changed measurements"
            );
            assert_eq!(
                shot(&mut interpreter, entry, "Draws", Some(SEED), n),
                shot(&mut interpreter, entry, "DrawsAfterCoins", Some(SEED), n),
                "{entry:?}: measurements changed classical draws"
            );
        }
    }
}

#[test]
fn evaluations_continue_the_seeded_streams() {
    let mut interpreter = interpreter();
    let evaluate = |interpreter: &mut Interpreter| {
        interpreter.set_quantum_seed(Some(SEED));
        interpreter.set_classical_seed(Some(SEED));
        [0, 1].map(|_| {
            (
                eval(interpreter, "Test.Coins()"),
                eval(interpreter, "Test.Draws()"),
            )
        })
    };
    let [first, second] = evaluate(&mut interpreter);
    assert_ne!(first.0, second.0);
    assert_ne!(first.1, second.1);
    assert_eq!([first, second], evaluate(&mut interpreter));
}

#[test]
fn evaluations_and_shots_draw_different_values() {
    let mut interpreter = interpreter();
    interpreter.set_quantum_seed(Some(SEED));
    interpreter.set_classical_seed(Some(SEED));
    let evaluated = eval(&mut interpreter, "Test.Draws()");
    assert_random(&evaluated);
    assert_ne!(
        evaluated,
        shot(&mut interpreter, Entry::Expr, "Draws", None, 0)
    );
}

#[test]
fn simulated_circuits_follow_the_quantum_seed() {
    let mut interpreter = interpreter();
    interpreter.set_quantum_seed(Some(SEED));
    let branches = eval(&mut interpreter, "Test.Branches");
    let entries: [&dyn Fn() -> CircuitEntryPoint; 2] = [
        &|| CircuitEntryPoint::EntryExpr("Test.Branches()".into()),
        &|| CircuitEntryPoint::Callable(branches.clone(), Value::unit()),
    ];
    let config = TracerConfig {
        max_operations: TracerConfig::DEFAULT_MAX_OPERATIONS,
        max_loop_iterations: None,
        source_locations: false,
        group_by_scope: false,
        prune_classical_qubits: false,
    };
    for entry in entries {
        let mut circuit = || {
            interpreter
                .circuit(entry(), CircuitGenerationMethod::Simulate, config)
                .expect("circuit generation should succeed")
                .to_string()
        };
        let first = circuit();
        // Guards against passing vacuously: both branches appear.
        assert!(first.contains(" X ") && first.contains(" Z "), "{first}");
        assert_eq!(first, circuit());
    }
}
