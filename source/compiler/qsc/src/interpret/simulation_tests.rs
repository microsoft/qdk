// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Interpreter, SimType};
use crate::compile::package_store_with_stdlib;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_eval::{
    output::CursorReceiver,
    val::{self, Value},
};
use qsc_passes::PackageType;
use rustc_hash::FxHashMap;
use std::io::Cursor;

#[test]
fn clifford_supports_qubit_loss() {
    let capabilities = TargetCapabilityFlags::all();
    let (std_id, store) = package_store_with_stdlib(capabilities);
    let mut interpreter = Interpreter::new(
        SourceMap::new(
            [(
                "test.qs".into(),
                "namespace Test { operation LoseQubit() : Result { use q = Qubit(); M(q) } }"
                    .into(),
            )],
            None,
        ),
        PackageType::Lib,
        capabilities,
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
        FxHashMap::default(),
    )
    .expect("interpreter should be created");
    let mut cursor = Cursor::new(Vec::<u8>::new());
    let mut receiver = CursorReceiver::new(&mut cursor);

    let result = interpreter.run(
        &mut receiver,
        Some("Test.LoseQubit()"),
        None,
        Some(1.0),
        None,
        None,
        SimType::Clifford(1),
    );

    assert_eq!(
        result.expect("Clifford simulation should support loss"),
        Value::Result(val::Result::Loss),
        "{}",
        receiver.dump()
    );
}
