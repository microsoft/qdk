// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::Expect;

use crate::capabilitiesck::check_supported_capabilities;
use qsc::{PackageType, incremental::Compiler};
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_fir::fir::{Package, PackageId, PackageStore};
use qsc_frontend::compile::PackageStore as HirPackageStore;
use qsc_lowerer::{Lowerer, map_hir_package_to_fir};
use qsc_rca::{Analyzer, PackageComputeProperties, PackageStoreComputeProperties};

pub fn check(source: &str, expect: &Expect, capabilities: TargetCapabilityFlags) {
    let compilation_context = CompilationContext::new(source);
    let (package, compute_properties) = compilation_context.get_package_compute_properties_tuple();
    let errors = check_supported_capabilities(
        package,
        compute_properties,
        capabilities,
        &compilation_context.fir_store,
    );
    expect.assert_debug_eq(&errors);
}

pub fn check_for_exe(source: &str, expect: &Expect, capabilities: TargetCapabilityFlags) {
    let compilation_context = CompilationContext::new_for_exe(source);
    let (package, compute_properties) = compilation_context.get_package_compute_properties_tuple();
    let errors = check_supported_capabilities(
        package,
        compute_properties,
        capabilities,
        &compilation_context.fir_store,
    );
    expect.assert_debug_eq(&errors);
}

fn lower_hir_package_store(
    lowerer: &mut Lowerer,
    hir_package_store: &HirPackageStore,
) -> PackageStore {
    let mut fir_store = PackageStore::new();
    for (id, unit) in hir_package_store {
        let pkg = lowerer.lower_package(&unit.package, &fir_store);
        fir_store.insert(map_hir_package_to_fir(id), pkg);
    }
    fir_store
}

struct CompilationContext {
    fir_store: PackageStore,
    compute_properties: PackageStoreComputeProperties,
    package_id: PackageId,
}

impl CompilationContext {
    fn new(source: &str) -> Self {
        let mut store = qsc::PackageStore::new(qsc::compile::core());
        let std_id = store.new_package_id();
        store.insert(
            std_id,
            qsc::compile::std(std_id, &store, TargetCapabilityFlags::all()),
        );
        let mut compiler = Compiler::new(
            SourceMap::default(),
            PackageType::Lib,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
        )
        .expect("should be able to create a new compiler");
        let package_id = map_hir_package_to_fir(compiler.package_id());
        let increment = compiler
            .compile_fragments_fail_fast("test", source)
            .expect("code should compile");
        compiler.update(increment);
        let mut lowerer = Lowerer::new();
        let fir_store = lower_hir_package_store(&mut lowerer, compiler.package_store());
        let analyzer = Analyzer::init(&fir_store);
        let compute_properties = analyzer.analyze_all();
        Self {
            fir_store,
            compute_properties,
            package_id,
        }
    }

    fn new_for_exe(source: &str) -> Self {
        let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
        let compiler = Compiler::new(
            SourceMap::new([("test".into(), source.into())], Some("".into())),
            PackageType::Exe,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
        )
        .expect("should be able to create a new compiler");
        let package_id = map_hir_package_to_fir(compiler.source_package_id());
        let mut lowerer = Lowerer::new();
        let fir_store = lower_hir_package_store(&mut lowerer, compiler.package_store());
        let analyzer = Analyzer::init(&fir_store);
        let compute_properties = analyzer.analyze_all();
        Self {
            fir_store,
            compute_properties,
            package_id,
        }
    }

    fn get_package_compute_properties_tuple(&self) -> (&Package, &PackageComputeProperties) {
        (
            self.fir_store.get(self.package_id),
            self.compute_properties.get(self.package_id),
        )
    }
}

pub const MINIMAL: &str = r#"
    namespace Test {
        operation Foo() : Unit {
            use q = Qubit();
            let r = M(q);
        }
    }"#;

pub const USE_DYNAMIC_BOOLEAN: &str = r#"
    namespace Test {
        operation Foo() : Unit {
            use q = Qubit();
            let b = M(q) == Zero;
        }
    }"#;

pub const USE_DYNAMIC_INT: &str = r#"
    namespace Test {
        import Std.Convert.*;
        import Std.Measurement.*;
        operation Foo() : Unit {
            use register = Qubit[4];
            let results = MeasureEachZ(register);
            let i = ResultArrayAsInt(results);
        }
    }"#;

pub const USE_DYNAMIC_PAULI: &str = r#"
    namespace Test {
        operation Foo() : Unit {
            use q = Qubit();
            let p = M(q) == Zero ? PauliX | PauliY;
        }
    }"#;

pub const USE_DYNAMIC_RANGE: &str = r#"
    namespace Test {
        operation Foo() : Unit {
            use q = Qubit();
            let range = 1..(M(q) == Zero ? 1 | 2)..10;
        }
    }"#;

pub const USE_DYNAMIC_DOUBLE: &str = r#"
    namespace Test {
        import Std.Convert.*;
        import Std.Measurement.*;
        operation Foo() : Unit {
            use register = Qubit[4];
            let results = MeasureEachZ(register);
            let d = IntAsDouble(ResultArrayAsInt(results));
        }
    }"#;

pub const USE_DYNAMIC_QUBIT: &str = r#"
    namespace Test {
        operation Foo() : Unit {
            use control = Qubit();
            if M(control) == Zero {
                    use q = Qubit();
            }
        }
    }"#;

pub const USE_DYNAMIC_BIG_INT: &str = r#"
    namespace Test {
        import Std.Convert.*;
        import Std.Measurement.*;
        operation Foo() : Unit {
            use register = Qubit[4];
            let results = MeasureEachZ(register);
            let bi = IntAsBigInt(ResultArrayAsInt(results));
        }
    }"#;

pub const USE_DYNAMIC_STRING: &str = r#"
    namespace Test {
        operation Foo() : Unit {
            use q = Qubit();
            let r = M(q);
            let s = $"{r == Zero}";
        }
    }"#;

pub const USE_DYNAMICALLY_SIZED_ARRAY: &str = r#"
    namespace Test {
        operation Foo() : Unit {
            use q = Qubit();
            let a = [0, size = M(q) == Zero ? 1 | 2];
        }
    }"#;

pub const USE_DYNAMIC_UDT: &str = r#"
    namespace Test {
        import Std.Convert.*;
        import Std.Math.*;
        import Std.Measurement.*;
        operation Foo() : Unit {
            use register = Qubit[4];
            let results = MeasureEachZ(register);
            let c = Complex(0.0, IntAsDouble(ResultArrayAsInt(results)));
        }
    }"#;

pub const USE_DYNAMIC_FUNCTION: &str = r#"
    namespace Test {
        import Std.Math.*;
        operation Foo() : Unit {
            use q = Qubit();
            let fn = M(q) == Zero ? Cos | Sin;
        }
    }"#;

pub const USE_DYNAMIC_OPERATION: &str = r#"
    namespace Test {
        import Std.Math.*;
        operation Foo() : Unit {
            use q = Qubit();
            let op = M(q) == Zero ? X | Y;
        }
    }"#;

pub const CALL_TO_CYCLIC_FUNCTION_WITH_CLASSICAL_ARGUMENT: &str = r#"
    function GaussSum(n : Int) : Int {
        if n == 0 {
            0
        } else {
            n + GaussSum(n - 1)
        }
    }
    operation Foo() : Unit {
        let sum = GaussSum(10);
    }"#;

pub const CALL_TO_CYCLIC_FUNCTION_WITH_DYNAMIC_ARGUMENT: &str = r#"
    function GaussSum(n : Int) : Int {
        if n == 0 {
            0
        } else {
            n + GaussSum(n - 1)
        }
    }
    operation Foo() : Unit {
        use q = Qubit();
        let sum = GaussSum(M(q) == Zero ? 10 | 20);
    }"#;

pub const CALL_TO_CYCLIC_OPERATION_WITH_CLASSICAL_ARGUMENT: &str = r#"
    operation GaussSum(n : Int) : Int {
        if n == 0 {
            0
        } else {
            n + GaussSum(n - 1)
        }
    }
    operation Foo() : Unit {
        let sum = GaussSum(10);
    }"#;

pub const CALL_TO_CYCLIC_OPERATION_WITH_DYNAMIC_ARGUMENT: &str = r#"
    operation GaussSum(n : Int) : Int {
        if n == 0 {
            0
        } else {
            n + GaussSum(n - 1)
        }
    }
    operation Foo() : Unit {
        use q = Qubit();
        let sum = GaussSum(M(q) == Zero ? 10 | 20);
    }"#;

pub const CALL_DYNAMIC_FUNCTION: &str = r#"
    namespace Test {
        import Std.Math.*;
        operation Foo() : Unit {
            use q = Qubit();
            let fn = M(q) == Zero ? Cos | Sin;
            fn(PI());
        }
    }"#;

pub const CALL_DYNAMIC_OPERATION: &str = r#"
    namespace Test {
        import Std.Math.*;
        operation Foo() : Unit {
            use q = Qubit();
            let op = M(q) == Zero ? X | Y;
            op(q);
        }
    }"#;

pub const CALL_UNRESOLVED_FUNCTION: &str = r#"
    namespace Test {
        import Std.Math.*;
        operation Foo() : Unit {
            use q = Qubit();
            let fn = true ? Cos | Sin;
            fn(PI());
        }
    }"#;

pub const MEASUREMENT_WITHIN_DYNAMIC_SCOPE: &str = r#"
    namespace Test {
        operation Foo() : Unit {
            use q = Qubit();
            if M(q) == One {
                let r = M(q);
            }
        }
    }"#;

pub const CUSTOM_MEASUREMENT: &str = r#"
    namespace Test {
        operation Main() : Result {
            use q = Qubit();
            Foo(q)
        }

        @Measurement()
        operation Foo(q: Qubit) : Result {
            body intrinsic;
        }
    }"#;

pub const CUSTOM_MEASUREMENT_WITH_SIMULATABLE_INTRINSIC_ATTR: &str = r#"
    namespace Test {
        operation Main() : Result {
            use q = Qubit();
            Foo(q)
        }

        @Measurement()
        @SimulatableIntrinsic()
        operation Foo(q: Qubit) : Result {
            H(q);
            M(q)
        }
    }"#;

pub const CUSTOM_RESET: &str = r#"
    namespace Test {
        operation Main() : Result {
            use q = Qubit();
            H(q);
            let res = M(q);
            Foo(q);
            res
        }

        @Reset()
        operation Foo(q: Qubit) : Unit {
            body intrinsic;
        }
    }"#;

pub const CUSTOM_RESET_WITH_SIMULATABLE_INTRINSIC_ATTR: &str = r#"
    namespace Test {
        operation Main() : Result {
            use q = Qubit();
            H(q);
            let res = M(q);
            Foo(q);
            res
        }

        @Reset()
        @SimulatableIntrinsic()
        operation Foo(q: Qubit) : Unit {
            Reset(q);
        }
    }"#;

pub const USE_DYNAMIC_INDEX: &str = r#"
    namespace Test {
        import Std.Convert.*;
        import Std.Measurement.*;
        operation Foo() : Unit {
            use register = Qubit[2];
            let results = MeasureEachZ(register);
            let i = ResultArrayAsInt(results);
            let a = [1, 2, 3, 4];
            a[i];
        }
    }"#;

pub const USE_DYNAMIC_LHS_EXP_BINOP: &str = r#"
    namespace Test {
        operation Foo() : Unit {
            use q = Qubit();
            let i = M(q) == Zero ? 0 | 1;
            i ^ 1;
        }
    }"#;

pub const USE_DYNAMIC_RHS_EXP_BINOP: &str = r#"
    namespace Test {
        operation Foo() : Unit {
            use q = Qubit();
            let i = M(q) == Zero ? 0 | 1;
            1 ^ i;
        }
    }"#;

pub const RETURN_WITHIN_DYNAMIC_SCOPE: &str = r#"
    namespace Test {
        operation Foo() : Int {
            use q = Qubit();
            if M(q) == One {
                return 1;
            }
            return 0;
        }
    }"#;

pub const LOOP_WITH_DYNAMIC_CONDITION: &str = r#"
    namespace Test {
        operation Foo() : Unit {
            use q = Qubit();
            let end = M(q) == Zero ? 5 | 10;
            for _ in 0..end {}
        }
    }"#;

pub const USE_CLOSURE_FUNCTION: &str = r#"
    namespace Test {
        import Std.Math.*;
        operation Foo() : Unit {
            let theta = PI();
            let lambdaFn = theta -> Sin(theta);
        }
    }"#;

pub const USE_ENTRY_POINT_STATIC_INT: &str = r#"
    namespace Test {
        @EntryPoint()
        operation Foo() : Int {
            42
        }
    }"#;

pub const USE_ENTRY_POINT_STATIC_DOUBLE: &str = r#"
    namespace Test {
        @EntryPoint()
        operation Foo() : Double {
            42.0
        }
    }"#;

pub const USE_ENTRY_POINT_STATIC_STRING: &str = r#"
    namespace Test {
        @EntryPoint()
        operation Foo() : String {
            "Hello, World!"
        }
    }"#;

pub const USE_ENTRY_POINT_STATIC_BOOL: &str = r#"
    namespace Test {
        @EntryPoint()
        operation Foo() : Bool {
            true
        }
    }"#;

pub const USE_ENTRY_POINT_STATIC_BIG_INT: &str = r#"
    namespace Test {
        @EntryPoint()
        operation Foo() : BigInt {
            42L
        }
    }"#;

pub const USE_ENTRY_POINT_STATIC_PAULI: &str = r#"
    namespace Test {
        @EntryPoint()
        operation Foo() : Pauli {
            PauliX
        }
    }"#;

pub const USE_ENTRY_POINT_STATIC_RANGE: &str = r#"
    namespace Test {
        @EntryPoint()
        operation Foo() : Range {
            1..10
        }
    }"#;

pub const USE_ENTRY_POINT_STATIC_INT_IN_TUPLE: &str = r#"
    namespace Test {
        @EntryPoint()
        operation Foo() : (Result, Int) {
            use q = Qubit();
            (M(q), 42)
        }
    }"#;

pub const USE_ENTRY_POINT_INT_ARRAY_IN_TUPLE: &str = r#"
    namespace Test {
        @EntryPoint()
        operation Foo() : (Result, Int[]) {
            use q = Qubit();
            (M(q), [1, 2, 3])
        }
    }"#;

pub const DYNAMIC_ARRAY_BINARY_OP: &str = r#"
    operation Main() : Unit {
        use qs = Qubit[2];
        MResetEachZ(qs) == [Zero, Zero];
    }
"#;
