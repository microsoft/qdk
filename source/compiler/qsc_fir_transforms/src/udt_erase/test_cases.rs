// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Q# evaluation-order fixtures shared by semantic and QIR regressions.

use indoc::formatdoc;

/// Both update forms evaluate the replacement before reading the record. The
/// mutation makes that order visible in the result, not only in log messages.
pub(super) fn field_update_order_cases() -> impl Iterator<Item = (String, i64)> {
    [
        ("let p=Make(n) w/ B <- {set n=3;n}; Read(p)", 303),
        ("let p=Make(n) w/ A <- {set n=3;n}; Read(p)", 332),
        (
            "let p=Make({set n+=1;n}) w/ B <- {set n=3;n}; 10*Read(p)+n",
            4034,
        ),
        (
            "mutable p=Make(0); set p w/= B <- {set p=Make(3);3}; Read(p)",
            303,
        ),
        (
            "mutable p=Make(0); set p w/= A <- {set p=Make(3);3}; Read(p)",
            332,
        ),
        (
            "let p=(Make(n) w/ B <- {set n=3;n}) w/ A <- {set n=4;n}; Read(p)",
            403,
        ),
    ]
    .into_iter()
    .map(|(body, expected)| {
        let source = formatdoc! {r#"
            struct Pair {{ A : Int, B : Int }}
            function Make(n : Int) : Pair {{ new Pair {{ A=n, B=10*n+2 }} }}
            function Read(p : Pair) : Int {{ 100*p.A+p.B }}
            @EntryPoint() operation Main() : Int {{
                mutable n=0;
                {body}
            }}
        "#};
        (source, expected)
    })
}

pub(super) const NESTED_FIELD_UPDATE_ORDER: &str = r#"
    newtype Triple = (A : Int, (B : Int, C : Int));
    function Make(n : Int) : Triple { Triple(n, (n+1,n+2)) }
    @EntryPoint() operation Main() : Int {
        mutable n=0;
        let p=Make(n) w/ B <- {set n=3;7};
        100*p::A+10*p::B+p::C
    }
"#;

pub(super) const SINGLE_FIELD_UPDATE_ORDER: &str = r#"
    newtype Only = (N : Int);
    function Make(n : Int) : Only { Only(n) }
    @EntryPoint() operation Main() : Int {
        mutable n=0;
        let p=Make({set n+=1;n}) w/ N <- {set n=3;7};
        10*p::N+n
    }
"#;

pub(super) const QUANTUM_FIELD_UPDATE_ORDER: &str = r#"
    struct Pair { A : Int, B : Int }
    operation Record(q : Qubit) : Pair {
        let bit=MResetZ(q);
        new Pair { A=if bit==One {1} else {0}, B=2 }
    }
    operation Replacement(q : Qubit) : Int { X(q); 3 }
    @EntryPoint() operation Main() : Int {
        use q=Qubit();
        let p=Record(q) w/ B <- Replacement(q);
        100*p.A+p.B
    }
"#;

/// Defunc leaves this conditional callable first-class. Erasure must preserve
/// source-order evaluation of the fields, including only the selected capture,
/// rather than evaluating them in the struct's declaration order.
pub(super) fn conditional_callable_field_cases() -> impl Iterator<Item = (String, i64)> {
    [(true, 457), (false, 477)]
        .into_iter()
        .map(|(flag, expected)| {
            let source = formatdoc! {r#"
            struct Payload {{ Head : Int, F : Int -> Int, Tail : Int }}
            function Log(label : String, n : Int) : Int {{ Message(label); n }}
            function Add(n : Int, x : Int) : Int {{ n+x }}
            function Read(p : Payload) : Int {{ 100*p.Head+10*p.F(2)+p.Tail }}
            function Choose(flag : Bool) : Int {{
                Read(new Payload {{
                    Tail=Log("tail",7),
                    F=if flag {{ Add(Log("first",3),_) }} else {{ Add(Log("second",5),_) }},
                    Head=Log("head",4)
                }})
            }}
            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
        "#};
            (source, expected)
        })
}

pub(super) fn struct_initializer_order_cases() -> impl Iterator<Item = (String, i64)> {
    [
        ("Tail={set n=8;n}, Head=n", 808),
        ("Head=n, Tail={set n=8;n}", 8),
    ]
    .into_iter()
    .flat_map(|(fields, expected)| {
        [false, true].map(|wrapped| {
            let value = format!("new Pair {{ {fields} }}");
            let call = if wrapped {
                format!("ReadWrapper(Wrapper({value}))")
            } else {
                format!("Read({value})")
            };
            let source = formatdoc! {r#"
                struct Pair {{ Head : Int, Tail : Int }}
                newtype Wrapper = (Value : Pair);
                function Read(p : Pair) : Int {{ 100*p.Head+p.Tail }}
                function ReadWrapper(w : Wrapper) : Int {{ Read(w::Value) }}
                @EntryPoint() operation Main() : Int {{
                    mutable n=0;
                    {call}
                }}
            "#};
            (source, expected)
        })
    })
}

pub(super) fn struct_copy_snapshot_cases() -> impl Iterator<Item = (String, i64)> {
    [
        ("...Original({set n+=1;n}), A=4", 411),
        ("...original, A={set original w/= C <- 99;4}", 411),
        ("...original, C={set original w/= A <- 99;8}", 418),
        ("...Original(1), C={set n=8;n}, A=n", 818),
        ("...Original({set n+=1;n}), C=n, B=n, A=n", 111),
    ]
    .into_iter()
    .flat_map(|(fields, expected)| {
        [false, true].map(|wrapped| {
            let value = format!("new Triple {{ {fields} }}");
            let call = if wrapped {
                format!("ReadWrapper(Wrapper({value}))")
            } else {
                format!("Read({value})")
            };
            let source = formatdoc! {r#"
                struct Triple {{ A : Int, B : Int, C : Int }}
                newtype Wrapper = (Value : Triple);
                function Original(n : Int) : Triple {{ new Triple {{ A=1, B=n, C=n }} }}
                function Read(p : Triple) : Int {{ 100*p.A+10*p.B+p.C }}
                function ReadWrapper(w : Wrapper) : Int {{ Read(w::Value) }}
                @EntryPoint() operation Main() : Int {{
                    mutable n=0;
                    mutable original=new Triple {{ A=4, B=1, C=1 }};
                    {call}
                }}
            "#};
            (source, expected)
        })
    })
}

pub(super) const PURE_STRUCT_COPY: &str = r#"
    struct Triple { A : Int, B : Int, C : Int }
    newtype Wrapper = (Value : Triple);
    function Original(n : Int) : Triple { new Triple { A=n+1, B=2*n, C=3*n } }
    function Read(w : Wrapper) : Int { let p=w::Value; 100*p.A+10*p.B+p.C }
    @EntryPoint() operation Main() : Int {
        Read(Wrapper(new Triple { ...Original(2), A=4 }))
    }
"#;

pub(super) const SINGLE_FIELD_COPY: &str = r#"
    struct Only { N : Int }
    newtype Wrapper = (Value : Only);
    function Original(n : Int) : Only { new Only { N=n } }
    function Read(w : Wrapper) : Int { (w::Value).N }
    @EntryPoint() operation Main() : Int {
        mutable n=0;
        Read(Wrapper(new Only { ...Original({set n+=1;n}), N=n+3 }))
    }
"#;
