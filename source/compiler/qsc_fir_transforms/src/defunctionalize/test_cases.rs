// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Q# fixtures shared by semantic, specialization, and QIR regression tests.

use indoc::formatdoc;

pub(super) fn recursive_capture_cases() -> impl Iterator<Item = (String, i64)> {
    [
        ("Repeat(x->x+offset,3)", 34),
        ("Repeat(Add(offset,_),3)", 34),
        ("Repeat(Make(offset),3)", 34),
        ("Repeat(x->x+offset+extra,3)", 46),
        ("Repeat(x->x+values[0],3)", 34),
        ("RepeatPair((x->x+offset,3))", 34),
        ("RepeatStruct(new Payload { F=x->x+offset, N=3 })", 34),
        ("RepeatTwo(x->x+offset,x->x+extra,3)", 52),
        ("RepeatSwap(x->x+offset,x->x+extra,3)", 33),
        ("RepeatNext(Make(offset),3)", 19),
        ("Repeat(Inc,3)", 10),
    ]
    .into_iter()
    .map(|(entry, expected)| {
        let source = formatdoc! {r#"
            struct Payload {{ F : Int -> Int, N : Int }}
            function Inc(n : Int) : Int {{ n+1 }}
            function Add(offset : Int, n : Int) : Int {{ offset+n }}
            function Make(offset : Int) : Int -> Int {{ x->x+offset }}
            function Repeat(f : Int -> Int, n : Int) : Int {{
                if n==0 {{ f(0) }} else {{ f(n)+Repeat(f,n-1) }}
            }}
            function RepeatPair(pair : (Int -> Int, Int)) : Int {{
                let (f,n)=pair;
                if n==0 {{ f(0) }} else {{ f(n)+RepeatPair((f,n-1)) }}
            }}
            function RepeatStruct(p : Payload) : Int {{
                if p.N==0 {{ p.F(0) }} else {{
                    p.F(p.N)+RepeatStruct(new Payload {{ F=p.F, N=p.N-1 }})
                }}
            }}
            function RepeatTwo(f : Int -> Int, g : Int -> Int, n : Int) : Int {{
                if n==0 {{ f(0)+g(0) }} else {{ f(n)+g(n)+RepeatTwo(f,g,n-1) }}
            }}
            function RepeatSwap(f : Int -> Int, g : Int -> Int, n : Int) : Int {{
                if n==0 {{ f(0)+g(0) }} else {{ f(n)+RepeatSwap(g,f,n-1) }}
            }}
            function RepeatNext(f : Int -> Int, n : Int) : Int {{
                if n==0 {{ f(0) }} else {{ f(n)+RepeatNext(Make(n),n-1) }}
            }}
            @EntryPoint() operation Main() : Int {{
                let offset=7;
                let extra=3;
                let values=[7];
                {entry}
            }}
        "#};
        (source, expected)
    })
}

pub(super) fn recursive_capture_control_cases(functor: &str) -> Vec<(String, i64)> {
    let double_control = functor.matches("Controlled").count() == 2;
    let states: &[(bool, bool)] = if double_control {
        &[(false, false), (false, true), (true, false), (true, true)]
    } else {
        &[(false, false), (true, false)]
    };
    states.iter().map(|&(outer, inner)| {
        let args = if double_control {
            "[outer], ([inner], (op,2,target))"
        } else {
            "[outer], (op,2,target)"
        };
        let source = formatdoc! {r#"
            operation Repeat(op : Qubit => Unit is Adj + Ctl, n : Int, q : Qubit) : Unit is Adj + Ctl {{
                if n>0 {{ op(q); Repeat(op,n-1,q); }}
            }}
            @EntryPoint() operation Main() : Int {{
                use outer=Qubit();
                use inner=Qubit();
                use target=Qubit();
                if {outer} {{ X(outer); }}
                if {inner} {{ X(inner); }}
                let angle=1.5707963267948966;
                let op=Ry(angle,_);
                {functor} Repeat({args});
                Reset(outer);
                Reset(inner);
                if MResetZ(target) == One {{ 1 }} else {{ 0 }}
            }}
        "#};
        (source,i64::from(outer && (!double_control || inner)))
    }).collect()
}

pub(super) fn effectful_short_circuit_guard_cases() -> impl Iterator<Item = (String, i64)> {
    ["and", "or"].into_iter().flat_map(|operator| {
        [false, true].map(move |flag| {
            let rhs_runs = if operator == "and" { flag } else { !flag };
            let value = if operator == "and" { flag } else { true };
            let expected = 100 * i64::from(value) + if rhs_runs { 6 } else { 4 };
            let source = formatdoc! {r#"
                function Inc(n : Int) : Int {{ n+1 }}
                function Twice(n : Int) : Int {{ 2*n }}
                function Guard(flag : Bool) : Bool {{ Message("guard"); flag }}
                @EntryPoint() operation Main() : Int {{
                    mutable f=Inc;
                    let value=Guard({flag}) {operator} {{ Message("rhs"); set f=Twice; true }};
                    Message("ready");
                    100*(if value {{ 1 }} else {{ 0 }})+f(3)
                }}
            "#};
            (source, expected)
        })
    })
}

pub(super) fn mutating_short_circuit_guard_cases() -> impl Iterator<Item = (String, i64)> {
    [("and", false, 601), ("or", true, 600)]
        .into_iter()
        .map(|(operator, initial, expected)| {
            let source = formatdoc! {r#"
                function Inc(n : Int) : Int {{ n+1 }}
                function Twice(n : Int) : Int {{ 2*n }}
                @EntryPoint() operation Main() : Int {{
                    mutable flag={initial};
                    mutable f=Inc;
                    let unused={{ set flag=not flag; flag }} {operator} {{ set f=Twice; true }};
                    100*f(3)+(if flag {{ 1 }} else {{ 0 }})
                }}
            "#};
            (source, expected)
        })
}

pub(super) fn compound_short_circuit_guard_cases() -> impl Iterator<Item = (String, i64)> {
    [
        ("and", true, 600),
        ("and", false, 400),
        ("or", false, 601),
        ("or", true, 401),
    ]
    .into_iter()
    .map(|(operator, initial, expected)| {
        let rhs = operator == "or";
        let source = formatdoc! {r#"
                function Inc(n : Int) : Int {{ n+1 }}
                function Twice(n : Int) : Int {{ 2*n }}
                @EntryPoint() operation Main() : Int {{
                    mutable flag={initial};
                    mutable f=Inc;
                    set flag {operator}= {{ Message("rhs"); set f=Twice; {rhs} }};
                    Message("ready");
                    100*f(3)+(if flag {{ 1 }} else {{ 0 }})
                }}
            "#};
        (source, expected)
    })
}

pub(super) const MEASURED_SHORT_CIRCUIT_GUARD: &str = r#"
    function Inc(n : Int) : Int { n+1 }
    function Twice(n : Int) : Int { 2*n }
    @EntryPoint() operation Main() : Int {
        use q=Qubit();
        X(q);
        mutable f=Inc;
        let unused=(MResetZ(q) == One) and { set f=Twice; true };
        f(3)
    }
"#;

pub(super) fn nested_inline_struct_capture_cases() -> impl Iterator<Item = (String, i64)> {
    [
        (
            r#"Sum(Read(new Payload {
                Head=Log("inner-head",1), F=Add(Log("inner-capture",2),_), Tail=Log("inner-tail",3)
            }), new Payload {
                Head=Log("outer-head",4), F=Add(Log("outer-capture",5),_), Tail=Log("outer-tail",6)
            })"#,
            619,
        ),
        (
            r#"SumLast(new Payload {
                Head=Log("outer-head",4), F=Add(Log("outer-capture",5),_), Tail=Log("outer-tail",6)
            }, Read(new Payload {
                Head=Log("inner-head",1), F=Add(Log("inner-capture",2),_), Tail=Log("inner-tail",3)
            }))"#,
            619,
        ),
        (
            r#"Sum(Sum(Read(new Payload {
                Head=Log("first-head",1), F=Add(Log("first-capture",2),_), Tail=Log("first-tail",3)
            }), new Payload {
                Head=Log("second-head",4), F=Add(Log("second-capture",5),_), Tail=Log("second-tail",6)
            }), new Payload {
                Head=Log("third-head",7), F=Add(Log("third-capture",8),_), Tail=Log("third-tail",9)
            })"#,
            1428,
        ),
    ]
    .into_iter()
    .map(|(body, expected)| {
        let source = formatdoc! {r#"
            struct Payload {{ Head : Int, F : Int -> Int, Tail : Int }}
            function Log(label : String, n : Int) : Int {{ Message(label); n }}
            function Add(n : Int, x : Int) : Int {{ n+x }}
            function Read(p : Payload) : Int {{ 100*p.Head+10*p.F(2)+p.Tail }}
            function Sum(n : Int, p : Payload) : Int {{ n+Read(p) }}
            function SumLast(p : Payload, n : Int) : Int {{ Read(p)+n }}
            @EntryPoint() operation Main() : Int {{ {body} }}
        "#};
        (source, expected)
    })
}

pub(super) fn inline_struct_capture_cases() -> impl Iterator<Item = (String, i64)> {
    [
        (
            "Read(new Payload { Tail=Log(\"tail\",7), F=Add(Log(\"capture\",3),_), Head=Log(\"head\",4) })",
            457,
        ),
        (
            "ReadNested((Log(\"prefix\",9), new Payload { Tail=Log(\"tail\",7), F=Add(Log(\"capture\",3),_), Head=Log(\"head\",4) }))",
            466,
        ),
        (
            "Read(new Payload { ...Original(), F=Add(Log(\"capture\",3),_), Head=Log(\"head\",4) })",
            457,
        ),
        (
            "Read(new Payload { Head=n, F=Add(Log(\"capture\",{set n+=1;n}),_), Tail=n })",
            31,
        ),
        (
            "Read(new Payload { Tail=n, F=Add(Log(\"capture\",{set n+=1;n}),_), Head=n })",
            130,
        ),
        (
            "Read(new Payload { F=Add(Log(\"capture\",{set n+=1;n}),_), Tail=n, Head=n })",
            131,
        ),
        (
            "Read(new Payload { Tail=Log(\"tail\",7), F={Message(\"field\");Add(Log(\"capture\",3),_)}, Head=Log(\"head\",4) })",
            457,
        ),
    ]
    .into_iter()
    .map(|(body, expected)| {
        let source = formatdoc! {r#"
            struct Payload {{ Head : Int, F : Int -> Int, Tail : Int }}
            function Log(label : String, n : Int) : Int {{ Message(label); n }}
            function Add(n : Int, x : Int) : Int {{ n+x }}
            function Inc(n : Int) : Int {{ n+1 }}
            function Original() : Payload {{
                Message("copy");
                new Payload {{ Head=4, F=Inc, Tail=7 }}
            }}
            function Read(p : Payload) : Int {{ 100*p.Head+10*p.F(2)+p.Tail }}
            function ReadNested(pair : (Int, Payload)) : Int {{
                let (prefix, p)=pair;
                prefix+100*p.Head+10*p.F(2)+p.Tail
            }}
            @EntryPoint() operation Main() : Int {{
                mutable n=0;
                {body}
            }}
        "#};
        (source, expected)
    })
}

pub(super) fn direct_struct_capture_control_cases(functor: &str) -> Vec<(String, i64)> {
    let double_control = functor.matches("Controlled").count() == 2;
    let control_states: &[(bool, bool)] = if double_control {
        &[(false, false), (false, true), (true, false), (true, true)]
    } else {
        &[(false, false), (true, false)]
    };
    let mut cases = Vec::new();
    for &(outer, inner) in control_states {
        for (setup, payload) in [
            ("", "new Payload { Head=Head(), Q=target, F=Ry(Angle(),_) }"),
            (
                "let head=Head(); let op=Ry(Angle(),_);",
                "new Payload { Head=head, Q=target, F=op }",
            ),
        ] {
            let args = if double_control {
                format!("[outer], ([inner], {payload})")
            } else {
                format!("[outer], {payload}")
            };
            let enabled = outer && (!double_control || inner);
            let source = formatdoc! {r#"
                struct Payload {{ Head : Int, F : Qubit => Unit is Adj + Ctl, Q : Qubit }}
                function Head() : Int {{ Message("head"); 4 }}
                function Angle() : Double {{ Message("capture"); 1.5707963267948966 }}
                operation Apply(p : Payload) : Unit is Adj + Ctl {{
                    if p.Head == 4 {{ p.F(p.Q); }}
                }}
                @EntryPoint() operation Main() : Int {{
                    use outer=Qubit();
                    use inner=Qubit();
                    use target=Qubit();
                    if {outer} {{ X(outer); }}
                    if {inner} {{ X(inner); }}
                    if {enabled} {{ H(target); }}
                    {setup}
                    {functor} Apply({args});
                    Reset(outer);
                    Reset(inner);
                    if MResetZ(target) == One {{ 1 }} else {{ 0 }}
                }}
            "#};
            cases.push((source, i64::from(enabled && !functor.contains("Adjoint"))));
        }
    }
    cases
}

pub(super) const PARTIAL_APPLICATION_CAPTURE_TIMING: &str = r#"
    function Logged(value : Int) : Int { Message($"capture:{value}"); value }
    function Add(offset : Int, x : Int) : Int { offset+x }
    @EntryPoint() operation Main() : Int {
        let f=Add(Logged(17),_);
        Message("ready");
        f(1)
    }
"#;

pub(super) const PARTIAL_APPLICATION_MUTATING_CAPTURE: &str = r#"
    function Add(offset : Int, x : Int) : Int { offset+x }
    @EntryPoint() operation Main() : Int {
        mutable n=0;
        let f=Add({set n+=1;n},_);
        let ready=n;
        100*ready+10*f(2)+f(3)
    }
"#;

pub(super) const PARTIAL_APPLICATION_INDEXED_CAPTURE: &str = r#"
    function Add(offset : Int, x : Int) : Int { offset+x }
    @EntryPoint() operation Main() : Int {
        mutable values=[2];
        let f=Add(values[0],_);
        set values w/= 0 <- { let before=f(1); 100*before };
        1000*f(1)+values[0]
    }
"#;

pub(super) fn direct_struct_field_order_cases() -> Vec<(String, i64)> {
    let mut cases = Vec::new();
    for fields in [
        "Head=4, F=op, Tail=7",
        "Head=4, Tail=7, F=op",
        "F=op, Head=4, Tail=7",
        "F=op, Tail=7, Head=4",
        "Tail=7, Head=4, F=op",
        "Tail=7, F=op, Head=4",
    ] {
        for (callable, expected) in [("Inc", 408), ("Make(3)", 410)] {
            for body in [
                format!("Read(new Payload {{ {fields} }})"),
                format!("let payload=new Payload {{ {fields} }}; Read(payload)"),
            ] {
                cases.push((
                    formatdoc! {r#"
                    struct Payload {{ Head : Int, F : Int -> Int, Tail : Int }}
                    function Inc(x : Int) : Int {{ x+1 }}
                    function Make(n : Int) : Int -> Int {{ x -> x+n }}
                    function Read(p : Payload) : Int {{ 100*p.Head+p.F(p.Tail) }}
                    @EntryPoint() operation Main() : Int {{
                        let op={callable};
                        {body}
                    }}
                "#},
                    expected,
                ));
            }
        }
    }
    cases
}

pub(super) fn struct_copy_factory_cases() -> impl Iterator<Item = (String, i64)> {
    ["Original()", "FromOne(3)", "FromThree(2,3,5)"]
        .into_iter()
        .flat_map(|factory| {
            [
                format!("Read(new Payload {{ ...{factory}, F=selected }})"),
                format!("let original={factory}; Read(new Payload {{ ...original, F=selected }})"),
                format!("let payload=new Payload {{ ...{factory}, F=selected }}; Read(payload)"),
            ]
            .into_iter()
            .flat_map(|body| {
                [(true, 408), (false, 414)].map(|(flag, expected)| {
                    let source = formatdoc! {r#"
                        struct Payload {{ Head : Int, F : Int -> Int, Tail : Int }}
                        function Inc(x : Int) : Int {{ x+1 }}
                        function Twice(x : Int) : Int {{ 2*x }}
                        function Original() : Payload {{ new Payload {{ Head=4, F=Inc, Tail=7 }} }}
                        function FromOne(n : Int) : Payload {{
                            new Payload {{ Tail=2*n+1, F=Inc, Head=n+1 }}
                        }}
                        function FromThree(a : Int, b : Int, c : Int) : Payload {{
                            new Payload {{ Tail=a+c, F=Inc, Head=b+1 }}
                        }}
                        function Read(payload : Payload) : Int {{ 100*payload.Head+payload.F(payload.Tail) }}
                        function Choose(flag : Bool) : Int {{
                            let selected=if flag {{ Inc }} else {{ Twice }};
                            {body}
                        }}
                        @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
                    "#};
                    (source, expected)
                })
            })
        })
}

pub(super) fn nested_struct_copy_factory_cases() -> impl Iterator<Item = (String, i64)> {
    [(true, 417), (false, 423)]
        .into_iter()
        .map(|(flag, expected)| {
            let source = formatdoc! {r#"
            struct Payload {{ Head : Int, F : Int -> Int, Tail : Int }}
            function Inc(x : Int) : Int {{ x+1 }}
            function Twice(x : Int) : Int {{ 2*x }}
            function Original(a : Int, b : Int, c : Int) : Payload {{
                new Payload {{ Head=b+1, F=Inc, Tail=a+c }}
            }}
            function Read(pair : (Int, Payload)) : Int {{
                let (prefix, payload)=pair;
                prefix+100*payload.Head+payload.F(payload.Tail)
            }}
            function Choose(flag : Bool) : Int {{
                let selected=if flag {{ Inc }} else {{ Twice }};
                Read((9,new Payload {{ ...Original(2,3,5), F=selected }}))
            }}
            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
        "#};
            (source, expected)
        })
}

pub(super) const DIRECT_STRUCT_COPY_FACTORY: &str = r#"
    struct Payload { Head : Int, F : Int -> Int, Tail : Int }
    function Inc(x : Int) : Int { x+1 }
    function Original(a : Int, b : Int, c : Int) : Payload {
        new Payload { Head=b+1, F=Inc, Tail=a+c }
    }
    function Read(payload : Payload) : Int { 100*payload.Head+payload.F(payload.Tail) }
    @EntryPoint() operation Main() : Int {
        Read(new Payload { ...Original(2,3,5), F=Inc })
    }
"#;

pub(super) fn type_constructor_argument_cases() -> impl Iterator<Item = (String, i64)> {
    [(true, 408), (false, 414)]
        .into_iter()
        .map(|(flag, expected)| {
            let source = formatdoc! {r#"
            newtype Payload = (Head : Int, F : Int -> Int, Tail : Int);
            function Inc(x : Int) : Int {{ x+1 }}
            function Twice(x : Int) : Int {{ 2*x }}
            function Read(payload : Payload) : Int {{ 100*payload::Head+payload::F(payload::Tail) }}
            function Choose(flag : Bool) : Int {{
                let selected=if flag {{ Inc }} else {{ Twice }};
                Read(Payload(4,selected,7))
            }}
            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
        "#};
            (source, expected)
        })
}

pub(super) fn stored_aggregate_snapshot_cases() -> impl Iterator<Item = (String, i64)> {
    [
        (
            "mutable n=3; let pair=(9,new Payload { F=Inc,N=n }); set n=99; Read(pair)",
            13,
        ),
        (
            "mutable n=3; let pair=(9,(Inc,n)); set n=99; ReadTuple(pair)",
            13,
        ),
        (
            "mutable original=new Payload { F=Inc,N=3 }; \
             let pair=(9,new Payload { ...original,F=Inc }); \
             set original w/= N <- 99; Read(pair)",
            13,
        ),
        (
            "mutable n=3; let snapshot=n; let pair=(9,new Payload { F=Inc,N=snapshot }); \
             set n=99; Read(pair)",
            13,
        ),
        (
            "mutable n=3; let f=Make(2); let pair=(9,new Payload { N=n,F=f }); \
             set n=99; Read(pair)",
            14,
        ),
        (
            "mutable n=3; let pair=new Payload { F=Inc,N=n }; set n=99; ReadFlat(pair)",
            4,
        ),
        (
            "mutable pair=(9,new Payload { F=Inc,N=3 }); \
             set pair=(9,new Payload { F=Inc,N=7 }); Read(pair)",
            17,
        ),
        (
            "mutable n=3; let pair=(9,new Payload { F=Inc,N=n }); \
             set n=99; let first=Read(pair); set n=100; 100*first+Read(pair)",
            1313,
        ),
        (
            "mutable prefix=9; let pair=(prefix,new Payload { F=Inc,N=3 }); \
             set prefix=99; Read(pair)",
            13,
        ),
        (
            "mutable n=3; let snapshot=new Payload { F=Inc,N=n }; \
             let alias=snapshot; let pair=(9,alias); set n=99; Read(pair)",
            13,
        ),
        (
            "mutable n=3; let pair=(9,new Payload { F=Inc,N=2*n }); \
             set n=99; Read(pair)",
            16,
        ),
    ]
    .into_iter()
    .map(|(body, expected)| {
        let source = formatdoc! {r#"
            struct Payload {{ F : Int -> Int, N : Int }}
            function Inc(x : Int) : Int {{ x+1 }}
            function Make(n : Int) : Int -> Int {{ x -> x+n }}
            function Read(pair : (Int, Payload)) : Int {{
                let (prefix, payload) = pair;
                prefix+payload.F(payload.N)
            }}
            function ReadTuple(pair : (Int, (Int -> Int, Int))) : Int {{
                let (prefix, (f, n)) = pair;
                prefix+f(n)
            }}
            function ReadFlat(payload : Payload) : Int {{ payload.F(payload.N) }}
            @EntryPoint() operation Main() : Int {{ {body} }}
        "#};
        (source, expected)
    })
}

pub(super) fn conditional_stored_aggregate_cases() -> impl Iterator<Item = (String, i64)> {
    [("true", 13), ("false", 15)]
        .into_iter()
        .map(|(flag, expected)| {
            let source = formatdoc! {r#"
            struct Payload {{ F : Int -> Int, N : Int }}
            function Inc(x : Int) : Int {{ x+1 }}
            function Twice(x : Int) : Int {{ 2*x }}
            function Read(pair : (Int, Payload)) : Int {{
                let (prefix, payload) = pair;
                prefix+payload.F(payload.N)
            }}
            function Choose(flag : Bool) : Int {{
                mutable n=3;
                let f=if flag {{ Inc }} else {{ Twice }};
                let pair=(9,new Payload {{ F=f,N=n }});
                set n=99;
                Read(pair)
            }}
            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
        "#};
            (source, expected)
        })
}

pub(super) fn struct_branch_cases() -> Vec<(String, i64)> {
    let mut cases = Vec::new();
    for (first, second, first_result, second_result) in [
        ("Inc", "Twice", 2407, 2607),
        ("Make(1)", "Make(3)", 2407, 2607),
        ("Wrap(Inc)", "Wrap(Twice)", 2507, 2707),
    ] {
        for (flag, expected) in [("true", first_result), ("false", second_result)] {
            for call in [
                "Read(new Payload { Head=2, F=selected, Tail=7 })",
                "Read(new Payload { Tail=7, F=selected, Head=2 })",
                "let payload=new Payload { Head=2, F=selected, Tail=7 }; Read(payload)",
                "let original=new Payload { Head=2, F=Inc, Tail=7 }; Read(new Payload { ...original, F=selected })",
            ] {
                let source = formatdoc! {r#"
                    struct Payload {{ Head : Int, F : Int -> Int, Tail : Int }}
                    function Inc(x : Int) : Int {{ x+1 }}
                    function Twice(x : Int) : Int {{ 2*x }}
                    function Make(n : Int) : Int -> Int {{ x -> x+n }}
                    function Wrap(f : Int -> Int) : Int -> Int {{ x -> f(x)+1 }}
                    function Read(payload : Payload) : Int {{
                        1000*payload.Head+100*payload.F(3)+payload.Tail
                    }}
                    function Choose(flag : Bool) : Int {{
                        let selected = if flag {{ {first} }} else {{ {second} }};
                        {call}
                    }}
                    @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
                "#};
                cases.push((source, expected));
            }
        }
    }
    cases
}

pub(super) fn nested_struct_branch_cases() -> impl Iterator<Item = (String, i64)> {
    [("true", 2416), ("false", 2616)]
        .into_iter()
        .map(|(flag, expected)| {
            let source = formatdoc! {r#"
            struct Payload {{ Head : Int, F : Int -> Int, Tail : Int }}
            function Make(n : Int) : Int -> Int {{ x -> x+n }}
            function ReadNested(pair : (Int, Payload)) : Int {{
                let (prefix, payload) = pair;
                prefix+1000*payload.Head+100*payload.F(3)+payload.Tail
            }}
            function Choose(flag : Bool) : Int {{
                let selected = if flag {{ Make(1) }} else {{ Make(3) }};
                ReadNested((9, new Payload {{ Tail=7, F=selected, Head=2 }}))
            }}
            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
        "#};
            (source, expected)
        })
}

pub(super) fn controlled_branch_cases(functor: &str) -> Vec<(String, i64)> {
    let double_control = functor
        .split_whitespace()
        .filter(|word| *word == "Controlled")
        .count()
        == 2;
    let adjoint = functor.contains("Adjoint");
    let control_states: &[(bool, bool)] = if double_control {
        &[(false, false), (false, true), (true, false), (true, true)]
    } else {
        &[(false, false), (true, false)]
    };
    let mut cases = Vec::new();
    for &choose_first in &[true, false] {
        for &(outer, inner) in control_states {
            for (operation, payload) in [
                ("Apply", "(selected, target)"),
                ("ApplyNested", "((9, (selected, 4, 13)), target)"),
                (
                    "ApplyStruct",
                    "(new Payload { Tail=13, F=selected, Head=9 }, target)",
                ),
                (
                    "ApplyStruct",
                    "(new Payload { ...Original(8,12), F=selected }, target)",
                ),
            ] {
                let enabled = outer && (!double_control || inner);
                let expected = i64::from(enabled && (choose_first != adjoint));
                let arguments = if double_control {
                    format!("[outer], ([inner], {payload})")
                } else {
                    format!("[outer], {payload}")
                };
                let source = formatdoc! {r#"
                struct Payload {{ Head : Int, F : Qubit => Unit is Adj + Ctl, Tail : Int }}
                function Make(angle : Double) : Qubit => Unit is Adj + Ctl {{
                    Ry(angle, _)
                }}
                function Original(head : Int, tail : Int) : Payload {{
                    new Payload {{ Head=head+1, F=X, Tail=tail+1 }}
                }}
                operation Apply(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl {{
                    op(q);
                }}
                operation ApplyNested(
                    payload : (Int, ((Qubit => Unit is Adj + Ctl), Int, Int)), q : Qubit
                ) : Unit is Adj + Ctl {{
                    let (prefix, (op, x, y)) = payload;
                    if prefix == 9 and x == 4 and y == 13 {{ op(q); }}
                }}
                operation ApplyStruct(payload : Payload, q : Qubit) : Unit is Adj + Ctl {{
                    if payload.Head == 9 and payload.Tail == 13 {{ payload.F(q); }}
                }}
                operation Choose(flag : Bool, outer : Qubit, inner : Qubit, target : Qubit) : Unit {{
                    let selected = if flag {{
                        Make(1.5707963267948966)
                    }} else {{
                        Make(-1.5707963267948966)
                    }};
                    {functor} {operation}({arguments});
                }}
                @EntryPoint() operation Main() : Int {{
                    use outer = Qubit();
                    use inner = Qubit();
                    use target = Qubit();
                    if {outer} {{ X(outer); }}
                    if {inner} {{ X(inner); }}
                    if {enabled} {{ H(target); }}
                    Choose({choose_first}, outer, inner, target);
                    Reset(outer);
                    Reset(inner);
                    if MResetZ(target) == One {{ 1 }} else {{ 0 }}
                }}
            "#};
                cases.push((source, expected));
            }
        }
    }
    cases
}

pub(super) fn nested_branch_payload_cases() -> impl Iterator<Item = (String, i64)> {
    [("Inc", "Add"), ("Make(1)", "Make(3)")]
        .into_iter()
        .flat_map(|(first, second)| {
            [("true", 9513), ("false", 9713)]
                .into_iter()
                .map(move |(flag, expected)| {
                    let source = formatdoc! {r#"
                function Inc(x : Int) : Int {{ x+1 }}
                function Add(x : Int) : Int {{ x+3 }}
                function Make(n : Int) : Int -> Int {{ x -> x+n }}
                function Read(pair : (Int, (Int -> Int, Int, Int))) : Int {{
                    let (prefix, (f, x, y)) = pair;
                    1000*prefix+100*f(x)+y
                }}
                function Choose(flag : Bool) : Int {{
                    let f = if flag {{ {first} }} else {{ {second} }};
                    Read((9, (f, 4, 13)))
                }}
                @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
            "#};
                    (source, expected)
                })
        })
}

pub(super) fn forwarded_capture_environment_cases() -> impl Iterator<Item = (String, i64)> {
    [
        (
            "function Forward(n : Int) : Int -> Int { Make(n+1) }",
            "Forward(2)",
            3,
        ),
        (
            "function Forward(unused : Int, n : Int) : Int -> Int { Make(n+1) }",
            "Forward(99,2)",
            3,
        ),
        (
            "function Inner(n : Int) : Int -> Int { Make(n+1) }\n\
             function Forward(n : Int) : Int -> Int { Inner(2*n) }",
            "Forward(2)",
            5,
        ),
        (
            "function Forward(n : Int) : Int -> Int { Make(n+1) }",
            "Forward(5)",
            6,
        ),
        (
            "function Forward(n : Int) : Int -> Int { let adjusted = n+1; Make(adjusted) }",
            "Forward(2)",
            3,
        ),
    ]
    .into_iter()
    .map(|(forward, factory, expected)| {
        let source = formatdoc! {r#"
            function Make(n : Int) : Int -> Int {{
                let values = [n];
                x -> values[0]+x
            }}
            {forward}
            @EntryPoint() operation Main() : Int {{
                let f = {factory};
                f(0)
            }}
        "#};
        (source, expected)
    })
}

pub(super) const FORWARDED_MULTI_FIELD_CAPTURES: &str = r#"
    function Make(a : Int, b : Int) : Int -> Int {
        let values = [a,b];
        x -> 100*values[0]+10*values[1]+x
    }
    function Forward(a : Int, b : Int) : Int -> Int { Make(b+1,2*a) }
    @EntryPoint() operation Main() : Int {
        let first = Forward(2,3);
        let second = Forward(5,7);
        1000*first(1)+second(1)
    }
"#;

pub(super) fn conditional_capture_layout_cases() -> impl Iterator<Item = (String, i64)> {
    [
        ("Wrap(Inc)", "Wrap(Twice)", 5, 7),
        ("Make(2)", "Make(5)", 5, 8),
        ("WrapWithOffset(Inc,10)", "WrapWithOffset(Twice,20)", 14, 26),
    ]
    .into_iter()
    .flat_map(|(first, second, first_result, second_result)| {
        [("true", first_result), ("false", second_result)]
            .into_iter()
            .flat_map(move |(flag, expected)| {
                ["Apply(chosen,3)", "Use(chosen)"]
                    .into_iter()
                    .map(move |call| {
                        let source = formatdoc! {r#"
                            function Inc(x : Int) : Int {{ x+1 }}
                            function Twice(x : Int) : Int {{ 2*x }}
                            function Wrap(f : Int -> Int) : Int -> Int {{ x -> f(x)+1 }}
                            function WrapWithOffset(f : Int -> Int, n : Int) : Int -> Int {{
                                x -> f(x)+n
                            }}
                            function Make(n : Int) : Int -> Int {{ x -> x+n }}
                            function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                            function Use(f : Int -> Int) : Int {{ f(3) }}
                            function Choose(flag : Bool) : Int {{
                                let a = {first};
                                let b = {second};
                                let chosen = if flag {{ a }} else {{ b }};
                                {call}
                            }}
                            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
                        "#};
                        (source, expected)
                    })
            })
    })
}

pub(super) fn embedded_callable_source(first: &str, second: &str, call: &str) -> String {
    formatdoc! {r#"
        function Inc(x : Int) : Int {{ x+1 }}
        function Twice(x : Int) : Int {{ 2*x }}
        function Wrap(f : Int -> Int) : Int -> Int {{ x -> f(x)+1 }}
        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
        function Both(f : Int -> Int, g : Int -> Int, x : Int) : Int {{ f(x)*100+g(x) }}
        @EntryPoint() operation Main() : Int {{
            let a = Wrap({first});
            let b = Wrap({second});
            {call}
        }}
    "#}
}

pub(super) fn embedded_callable_cases() -> impl Iterator<Item = (String, i64)> {
    [
        ("Apply(a, 3)*100+Apply(b, 3)", 507),
        ("Apply(b, 3)*100+Apply(a, 3)", 705),
        ("a(3)*100+b(3)", 507),
        ("Apply(b, 3)", 7),
        ("Both(a, b, 3)", 507),
        ("Apply(a, 3)*100+Apply(a, 3)", 505),
    ]
    .into_iter()
    .map(|(call, expected)| (embedded_callable_source("Inc", "Twice", call), expected))
}

pub(super) fn compound_capture_cases() -> impl Iterator<Item = (String, i64)> {
    [
        ("[offset, offset+1]", 1719),
        ("[offset, size = 2]", 1718),
        ("[offset, offset+1] w/ 1 <- offset+2", 1720),
        ("[DoubleIt(offset), -offset]", 3384),
        (
            "[(new Pair { First = offset, Second = offset+1 }).First, offset+1]",
            1719,
        ),
    ]
    .into_iter()
    .map(|(capture, expected)| {
        let source = formatdoc! {r#"
            struct Pair {{ First : Int, Second : Int }}
            function DoubleIt(x : Int) : Int {{ 2*x }}
            function Make(offset : Int) : Int -> Int {{
                let values = {capture};
                x -> 100*values[0]+values[1]+x
            }}
            function Forward(f : Int -> Int) : Int -> Int {{ f }}
            @EntryPoint() operation Main() : Int {{
                let f = Forward(Forward(Make(17)));
                f(1)
            }}
        "#};
        (source, expected)
    })
}

pub(super) fn mutable_scalar_capture_cases() -> impl Iterator<Item = (String, i64)> {
    ["Make(offset)", "Add(offset, _)", "Make([offset][0])"]
        .into_iter()
        .flat_map(|factory| {
            ["f(1)", "Apply(f, 1)"].into_iter().map(move |call| {
                let source = formatdoc! {r#"
                    function Add(offset : Int, x : Int) : Int {{ x+offset }}
                    function Make(offset : Int) : Int -> Int {{ x -> x+offset }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    @EntryPoint() operation Main() : Int {{
                        mutable offset = 17;
                        let f = {factory};
                        set offset = 99;
                        {call}
                    }}
                "#};
                (source, 18)
            })
        })
}

pub(super) fn early_return_capture_cases() -> impl Iterator<Item = (String, i64)> {
    [("true", 13), ("false", 17)]
        .into_iter()
        .map(|(choose, expected)| {
            let source = formatdoc! {r#"
                function Make(first : Bool, offset : Int) : Int -> Int {{
                    if first {{ return x -> x+offset; }}
                    let other = offset+4;
                    x -> x+other
                }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                @EntryPoint() operation Main() : Int {{
                    Apply(Make({choose}, 3), 10)
                }}
            "#};
            (source, expected)
        })
}

pub(super) fn callable_array_identity_cases() -> impl Iterator<Item = (String, i64)> {
    [
        ("[Wrap(Inc), Wrap(Twice)]", 57),
        ("[Wrap(Twice), Wrap(Inc)]", 75),
        ("[Wrap(Inc), Wrap(Inc)]", 55),
    ]
    .into_iter()
    .map(|(members, expected)| {
        let source = formatdoc! {r#"
            function Inc(x : Int) : Int {{ x+1 }}
            function Twice(x : Int) : Int {{ 2*x }}
            function Wrap(f : Int -> Int) : Int -> Int {{ x -> f(x)+1 }}
            function Read(functions : (Int -> Int)[]) : Int {{
                mutable result = 0;
                for f in functions {{ set result = 10*result+f(3); }}
                result
            }}
            @EntryPoint() operation Main() : Int {{ Read({members}) }}
        "#};
        (source, expected)
    })
}

pub(super) const RECORD_COPY_UPDATE: &str = r#"
    struct Pair { First : Int, Second : Int }
    function Make(offset : Int) : Int -> Int {
        let pair = (new Pair { First = offset, Second = offset+1 })
            w/ First <- offset+2;
        x -> 100*pair.First+pair.Second+x
    }
    function Forward(f : Int -> Int) : Int -> Int { f }
    @EntryPoint() operation Main() : Int {
        let f = Forward(Make(17));
        f(1)
    }
"#;

pub(super) const COMPOUND_REPEAT_AND_RANGE: &str = r#"
    struct Config { Values : Int[], Bounds : Range }
    function Make(offset : Int, count : Int) : Int -> Int {
        let config = new Config {
            Values = [offset, size = count],
            Bounds = offset..2..offset+count
        };
        x -> 10000*Length(config.Values)+100*config.Values[1]+config.Bounds.End+x
    }
    @EntryPoint() operation Main() : Int {
        let f = Make(17, 3);
        f(1)
    }
"#;

pub(super) const MUTABLE_ARRAY_CAPTURE: &str = r#"
    function Make(values : Int[]) : Int -> Int { x -> values[0]+x }
    @EntryPoint() operation Main() : Int {
        mutable values = [17];
        let f = Make(values);
        set values w/= 0 <- 99;
        f(1)
    }
"#;

pub(super) const LOOP_FACTORY_CAPTURES: &str = r#"
    function Make(offset : Int) : Int -> Int { x -> x+offset }
    function Apply(f : Int -> Int, x : Int) : Int { f(x) }
    @EntryPoint() operation Main() : Int {
        mutable result = 0;
        for offset in 1..3 {
            let f = Make(offset);
            set result = 10*result+Apply(f, 1);
        }
        result
    }
"#;

pub(super) const SHADOWED_CAPTURE: &str = r#"
    function Make(offset : Int) : Int -> Int { x -> x+offset }
    function Apply(f : Int -> Int, x : Int) : Int { f(x) }
    @EntryPoint() operation Main() : Int {
        let offset = 17;
        let f = Make(offset);
        let inner = {
            let offset = 99;
            Apply(f, offset)
        };
        1000*Apply(f, 1)+inner
    }
"#;

pub(super) const IMMUTABLE_CAPTURE_SNAPSHOT: &str = r#"
    function Make(offset : Int) : Int -> Int { x -> x+offset }
    function Apply(f : Int -> Int, x : Int) : Int { f(x) }
    @EntryPoint() operation Main() : Int {
        mutable offset = 17;
        let snapshot = offset;
        let f = Make(snapshot);
        set offset = 99;
        Apply(f, 1)
    }
"#;

pub(super) const RETURNED_CALLABLE_ARRAY: &str = r#"
    function Make(offset : Int) : (Int -> Int)[] {
        let first = x -> x+offset;
        let second = x -> x+2*offset;
        [first, second]
    }
    @EntryPoint() operation Main() : Int {
        let functions = Make(3);
        100*functions[0](1)+functions[1](1)
    }
"#;
