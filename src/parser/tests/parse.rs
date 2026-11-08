use bumpalo::Bump;
use hakana_parser::ast::*;
use hakana_parser::{CommentKind, parse};

fn first_fun<'a>(program: &Program<'a>) -> &'a FunDef<'a> {
    program
        .defs
        .iter()
        .find_map(|d| match d {
            Def::Fun(f) => Some(*f),
            _ => None,
        })
        .expect("no function")
}

/// The expression of the first statement of the first function.
fn first_expr<'a>(program: &Program<'a>) -> &'a Expr<'a> {
    match first_fun(program).fun.body[0].kind {
        StmtKind::Expr(e) => e,
        StmtKind::Return(Some(e)) => e,
        ref other => panic!("unexpected statement {:?}", other),
    }
}

#[test]
fn function_declaration() {
    let src = "/** Adds. */\nasync function add(int $a, ?string $b = null): Awaitable<int> {\n  return $a;\n}\n";
    let arena = Bump::new();
    let out = parse(&arena, src).unwrap();
    let f = first_fun(&out.program);
    assert_eq!(f.name.name, "add");
    assert_eq!(f.name.span.text(src), "add");
    assert!(f.fun.is_async());
    assert!(!f.fun.has_yield());
    assert_eq!(f.fun.params.len(), 2);
    assert_eq!(f.fun.params[0].name.name, "$a");
    assert!(matches!(
        f.fun.params[1].hint.unwrap().kind,
        HintKind::Option(inner) if matches!(inner.kind, HintKind::Name("string"))
    ));
    assert!(matches!(
        f.fun.params[1].default.unwrap().kind,
        ExprKind::Null
    ));
    let ret = f.fun.ret.unwrap();
    assert_eq!(ret.as_named().unwrap().0, "Awaitable");
    assert_eq!(ret.span.text(src), "Awaitable<int>");
    assert_eq!(f.fun.doc_comment.unwrap().text(src), "/** Adds. */");
    // The function spans from `async` to the closing brace.
    assert!(f.fun.span.text(src).starts_with("async function"));
    assert!(f.fun.span.text(src).ends_with('}'));
}

#[test]
fn precedence_follows_hhvm() {
    let arena = Bump::new();

    let out = parse(&arena, "function f(): void { $a + $b * $c; }").unwrap();
    let ExprKind::Binop(add) = first_expr(&out.program).kind else {
        panic!()
    };
    assert_eq!(add.op, Bop::Plus);
    assert!(matches!(add.rhs.kind, ExprKind::Binop(m) if m.op == Bop::Star));

    // An assignment to an lvalue binds tighter than the operator before it.
    let out = parse(&arena, "function f(): void { $a && $b = g(); }").unwrap();
    let ExprKind::Binop(and) = first_expr(&out.program).kind else {
        panic!()
    };
    assert_eq!(and.op, Bop::AmpAmp);
    assert!(matches!(and.rhs.kind, ExprKind::Assign(_)));

    // `??` is right-associative.
    let out = parse(&arena, "function f(): void { $a ?? $b ?? $c; }").unwrap();
    let ExprKind::Binop(outer) = first_expr(&out.program).kind else {
        panic!()
    };
    assert!(matches!(outer.lhs.kind, ExprKind::Lvar("$a")));
    assert!(matches!(outer.rhs.kind, ExprKind::Binop(_)));
}

#[test]
fn calls_and_members() {
    let arena = Bump::new();
    let src = "function f(): void { $x->run<int>(inout $a, ...$rest); }";
    let out = parse(&arena, src).unwrap();
    let ExprKind::Call(call) = first_expr(&out.program).kind else {
        panic!()
    };
    let ExprKind::ObjGet(get) = call.func.kind else {
        panic!()
    };
    assert!(get.is_method);
    assert_eq!(call.targs.len(), 1);
    assert!(matches!(call.args[0].kind, ExprKind::InoutArg(_)));
    assert!(call.unpacked.is_some());

    // Calling a property that holds a closure is not a method call.
    let out = parse(&arena, "function f(): void { ($x->callback)(1); }").unwrap();
    let ExprKind::Call(call) = first_expr(&out.program).kind else {
        panic!()
    };
    let ExprKind::ObjGet(get) = call.func.kind else {
        panic!()
    };
    assert!(!get.is_method);

    // `<` after a name is a comparison unless it really is a type list.
    let out = parse(&arena, "function f(): void { $a = COUNT < MAX; }").unwrap();
    let ExprKind::Assign(assign) = first_expr(&out.program).kind else {
        panic!()
    };
    assert!(matches!(assign.rhs.kind, ExprKind::Binop(b) if b.op == Bop::Lt));
}

#[test]
fn strings() {
    let arena = Bump::new();
    let src = r#"function f(): void { return 'it\'s' . "a\tb" . "x {$y->z} $w[0]"; }"#;
    let out = parse(&arena, src).unwrap();
    let ExprKind::Binop(outer) = first_expr(&out.program).kind else {
        panic!()
    };
    let ExprKind::Binop(inner) = outer.lhs.kind else {
        panic!()
    };
    assert!(matches!(inner.lhs.kind, ExprKind::String(b"it's")));
    assert!(matches!(inner.rhs.kind, ExprKind::String(b"a\tb")));
    let ExprKind::String2(parts) = outer.rhs.kind else {
        panic!()
    };
    assert_eq!(parts.len(), 4);
    assert!(matches!(parts[0].kind, ExprKind::String(b"x ")));
    assert!(matches!(parts[1].kind, ExprKind::ObjGet(_)));
    // The braces belong to the embedded expression's span.
    assert_eq!(parts[1].span.text(src), "{$y->z}");
    assert!(matches!(parts[2].kind, ExprKind::String(b" ")));
    assert!(matches!(parts[3].kind, ExprKind::ArrayGet(_)));

    // A literal without escapes borrows from the source.
    let src = "function f(): void { return 'plain'; }";
    let out = parse(&arena, src).unwrap();
    let ExprKind::String(value) = first_expr(&out.program).kind else {
        panic!()
    };
    let range = src.as_bytes().as_ptr_range();
    assert!(range.contains(&value.as_ptr()));
}

#[test]
fn xhp() {
    let arena = Bump::new();
    let src =
        "function f(): void { return <div class=\"a\" {...$rest}>Hello   {$name}\n  <br/></div>; }";
    let out = parse(&arena, src).unwrap();
    let ExprKind::Xml(xml) = first_expr(&out.program).kind else {
        panic!()
    };
    assert_eq!(xml.name.name, "div");
    assert_eq!(xml.attrs.len(), 2);
    assert_eq!(xml.children.len(), 3);
    // Whitespace runs in text collapse to a single space.
    assert!(matches!(xml.children[0].kind, ExprKind::String(b"Hello ")));
    assert!(matches!(xml.children[1].kind, ExprKind::Lvar("$name")));
    assert!(matches!(xml.children[2].kind, ExprKind::Xml(_)));
}

#[test]
fn classes() {
    let arena = Bump::new();
    let src = "abstract class C<T> extends B implements I {\n  const int X = 1;\n  private ?string $p = null;\n  public function __construct(private int $q) {}\n  abstract protected function m(): void;\n}\n";
    let out = parse(&arena, src).unwrap();
    let Def::Class(class) = out.program.defs[0] else {
        panic!()
    };
    assert_eq!(class.name.name, "C");
    assert_eq!(class.kind, ClassKind::Class);
    assert!(class.flags & class_flags::ABSTRACT != 0);
    assert_eq!(class.tparams.len(), 1);
    assert_eq!(class.members.len(), 4);
    let Member::Method(ctor) = class.members[2] else {
        panic!()
    };
    assert_eq!(ctor.name.name, "__construct");
    assert_eq!(ctor.fun.params[0].visibility, Some(Visibility::Private));
    let Member::Method(m) = class.members[3] else {
        panic!()
    };
    assert!(!m.fun.has_body());
    assert_eq!(m.visibility, Some(Visibility::Protected));
}

#[test]
fn namespaces_group_following_definitions() {
    let arena = Bump::new();
    let src = "namespace A;\nfunction f(): void {}\nnamespace B;\nfunction g(): void {}\n";
    let out = parse(&arena, src).unwrap();
    assert_eq!(out.program.defs.len(), 2);
    let Def::Namespace(a) = out.program.defs[0] else {
        panic!()
    };
    assert_eq!(a.name.name, "A");
    assert_eq!(a.defs.len(), 1);
}

#[test]
fn comments() {
    let arena = Bump::new();
    let src = "function f(): void {\n  // leading\n  $a = 1; // trailing\n  /* HH_FIXME[4110] why */\n  g();\n}\n/* HH_FIXME[4110] outside */\nfunction h(): void {}\n";
    let out = parse(&arena, src).unwrap();
    let kinds: Vec<_> = out.comments.iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        [
            CommentKind::Line,
            CommentKind::Line,
            CommentKind::FixMe,
            CommentKind::FixMe
        ]
    );
    assert!(out.comments[0].first_leading && !out.comments[0].trailing);
    assert!(out.comments[1].trailing);
    // Only the fixme inside the function body is in a statement block.
    assert!(out.comments[2].in_block);
    assert!(!out.comments[3].in_block);
}

#[test]
fn rejects_what_hhvm_rejects() {
    let arena = Bump::new();
    for src in [
        // plain syntax errors
        "function f(): void { $a = ; }",
        "function f(: void {}",
        "class C { public function }",
        // constructs HHVM reports an error for while lowering
        "function f(): void { @g(); }",
        "function f(): void { new Foo()->bar(); }",
        "function f(): void { $x instanceof Foo; }",
        // a keyword in the wrong case
        "Function f(): void {}",
    ] {
        // Either the parse fails or it reports the error HHVM would.
        let rejected = match parse(&arena, src) {
            Err(_) => true,
            Ok(out) => !out.errors.is_empty(),
        };
        assert!(rejected, "should be rejected: {src}");
    }
}

#[test]
fn error_reports_where_it_stopped() {
    let arena = Bump::new();
    let src = "function f(): void {\n  $a = ;\n}";
    let parsed = parse(&arena, src).unwrap();
    let err = parsed.recovered.expect("a syntax error");
    assert_eq!(&src[err.offset as usize..err.offset as usize + 1], ";");
    // The syntax error, and what HHVM's translation to its AST makes of the
    // expression that is not there.
    let messages: Vec<&str> = parsed.errors.iter().map(|e| &*e.message).collect();
    assert_eq!(
        messages,
        [
            "An expression is expected here.",
            "Encountered unexpected text ``, was expecting a expression."
        ]
    );
    assert_eq!(parsed.errors[0].start, err.offset);
}

/// What a broken file declares survives its syntax errors.
#[test]
fn recovers_declarations() {
    fn names(src: &str) -> Vec<String> {
        let arena = Bump::new();
        let parsed = parse(&arena, src).unwrap();
        assert!(
            parsed.recovered.is_some(),
            "should have a syntax error: {src}"
        );
        let mut out = vec![];
        for def in parsed.program.defs {
            match def {
                Def::Fun(f) => out.push(f.name.name.to_string()),
                Def::Class(c) => {
                    out.push(c.name.name.to_string());
                    for member in c.members {
                        if let Member::Method(m) = member {
                            out.push(format!("{}::{}", c.name.name, m.name.name));
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    // An error inside a function body.
    assert_eq!(
        names("function a(): void {\n  $x = ;\n}\n\nfunction b(): int {\n  return 1;\n}\n"),
        ["a", "b"]
    );
    // Conflict markers: braces cannot be trusted, layout can.
    assert_eq!(
        names(
            "function a(): void {\n  $f = () ==> {\n<<<<<<< HEAD\n  };\n=======\n    echo 1;\n  };\n>>>>>>> other\n  $f();\n}\n\nfunction b(): void {}\n"
        ),
        ["a", "b"]
    );
    // A member that does not parse is dropped; its neighbours are kept.
    assert_eq!(
        names(
            "class C {\n  public function a(): void {}\n  public function (): void {}\n  public function c(): void {\n    foo(;\n  }\n}\n\nfunction after(): void {}\n"
        ),
        ["C", "C::a", "C::c", "after"]
    );
    // A declaration that does not parse is dropped.
    assert_eq!(
        names("function a(): void {}\n\nfunction b(int $x: void {}\n\nfunction c(): void {}\n"),
        ["a", "c"]
    );
}
