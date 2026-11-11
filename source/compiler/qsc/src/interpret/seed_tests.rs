// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Seeded runs must be reproducible, every shot and evaluation must draw fresh values, and
//! measurement, noise, and classical draws must not affect one another. None of these checks
//! depend on the values that a particular seed produces.

use super::{CircuitEntryPoint, CircuitGenerationMethod, Interpreter, SimType};
use qsc_circuit::TracerConfig;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_eval::{output::GenericReceiver, val::Value};
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
    }
";

const SEED: u64 = 1;

fn interpreter() -> Interpreter {
    let sources = SourceMap::new([("test.qs".into(), SOURCE.into())], None);
    let capabilities = TargetCapabilityFlags::all();
    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
    Interpreter::new(
        sources,
        PackageType::Lib,
        capabilities,
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
        Default::default(),
    )
    .expect("interpreter should be created")
}

fn run(interpreter: &mut Interpreter, expr: &str, seed: Option<u64>, shot: u64) -> Value {
    let mut sink = std::io::sink();
    let mut out = GenericReceiver::new(&mut sink);
    let value = interpreter
        .run(
            &mut out,
            Some(expr),
            None,
            None,
            None,
            seed,
            shot,
            SimType::Sparse,
        )
        .expect("run should succeed");
    assert_random(&value);
    value
}

fn invoke(interpreter: &mut Interpreter, callable: &str, seed: Option<u64>, shot: u64) -> Value {
    let callable = eval(interpreter, callable);
    let mut sink = std::io::sink();
    let mut out = GenericReceiver::new(&mut sink);
    let value = interpreter
        .invoke_with_noise(
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
        .expect("invocation should succeed");
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

#[test]
fn seeded_runs_are_reproducible() {
    let mut interpreter = interpreter();
    for expr in ["Test.Coins()", "Test.Draws()", "Test.NoisyCoins()"] {
        assert_eq!(
            run(&mut interpreter, expr, Some(SEED), 0),
            run(&mut interpreter, expr, Some(SEED), 0),
            "{expr}"
        );
    }
}

#[test]
fn shots_and_nearby_seeds_draw_fresh_values() {
    let mut interpreter = interpreter();
    for expr in ["Test.Coins()", "Test.Draws()", "Test.NoisyCoins()"] {
        let shot = run(&mut interpreter, expr, Some(SEED), 1);
        assert_ne!(shot, run(&mut interpreter, expr, Some(SEED), 0), "{expr}");
        assert_ne!(
            shot,
            run(&mut interpreter, expr, Some(SEED + 1), 0),
            "{expr}"
        );
    }
}

#[test]
fn interpreter_seeds_apply_to_each_shot() {
    let mut interpreter = interpreter();
    interpreter.set_quantum_seed(Some(SEED));
    interpreter.set_classical_seed(Some(SEED));
    for expr in ["Test.Coins()", "Test.Draws()"] {
        let first = run(&mut interpreter, expr, None, 0);
        assert_eq!(first, run(&mut interpreter, expr, None, 0), "{expr}");
        assert_ne!(first, run(&mut interpreter, expr, None, 1), "{expr}");
    }
    for callable in ["Test.Coins", "Test.Draws"] {
        let first = invoke(&mut interpreter, callable, None, 0);
        assert_eq!(
            first,
            invoke(&mut interpreter, callable, None, 0),
            "{callable}"
        );
        assert_ne!(
            first,
            invoke(&mut interpreter, callable, None, 1),
            "{callable}"
        );
    }
}

#[test]
fn measurement_and_classical_streams_are_independent() {
    let mut interpreter = interpreter();
    for shot in 0..4 {
        assert_eq!(
            run(&mut interpreter, "Test.Coins()", Some(SEED), shot),
            run(&mut interpreter, "Test.CoinsAfterDraws()", Some(SEED), shot),
            "classical draws changed measurements"
        );
        assert_eq!(
            run(&mut interpreter, "Test.Draws()", Some(SEED), shot),
            run(&mut interpreter, "Test.DrawsAfterCoins()", Some(SEED), shot),
            "measurements changed classical draws"
        );
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
