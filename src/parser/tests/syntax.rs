//! Syntax beyond the everyday: each test parses one construct and checks the
//! tree, the errors, or what it is lowered to.

use bumpalo::Bump;
use hakana_parser::ast::*;
use hakana_parser::{aast, parse, parse_file};

const FEATURES: &str = "<<file: __EnableUnstableFeatures('shape_and_tuple_destructuring', 'shape_field_punning', 'shape_splat_expression', 'match_statements', 'class_aliases_everywhere', 'named_parameters', 'case_type_where_clauses')>>\n";

/// The statements of the first function of `src`, parsed without errors.
fn body<'a>(arena: &'a Bump, src: &'a str) -> Block<'a> {
    let out = parse(arena, src).unwrap();
    assert_eq!(out.errors, [], "{src}");
    out.program
        .defs
        .iter()
        .find_map(|d| match d {
            Def::Fun(f) => Some(f.fun.body),
            _ => None,
        })
        .expect("no function")
}

fn first_expr<'a>(arena: &'a Bump, src: &'a str) -> &'a Expr<'a> {
    match body(arena, src)[0].kind {
        StmtKind::Expr(e) => e,
        ref other => panic!("unexpected statement {other:?}"),
    }
}

fn messages(src: &str) -> Vec<String> {
    let arena = Bump::new();
    let file = parse_file(&arena, src, "test.hack").unwrap();
    file.errors.iter().map(|e| e.message.to_string()).collect()
}

/// The first expression statement of the first function, lowered.
fn lowered<'a>(arena: &'a Bump, src: &'a str) -> aast::Expr<'a> {
    let file = parse_file(arena, src, "test.hack").unwrap();
    assert_eq!(file.errors, [], "{src}");
    let fun = file
        .program
        .0
        .iter()
        .find_map(|d| match d {
            aast::Def::Fun(f) => Some(*f),
            _ => None,
        })
        .expect("no function");
    match fun.fun.body.fb_ast.0[0].1 {
        aast::Stmt_::Expr(e) => *e,
        ref other => panic!("unexpected statement {other:?}"),
    }
}

#[test]
fn shape_fields() {
    let arena = Bump::new();
    let src = arena.alloc_str(&format!(
        "{FEATURES}function f(): void {{ shape($x, 'y' => 1, ...$rest); }}"
    ));
    let ExprKind::Shape(fields, None) = first_expr(&arena, src).kind else {
        panic!()
    };
    assert!(fields[0].is_punned());
    assert!(matches!(
        fields[0].key(),
        Some(ShapeKey::Str { value: b"x", .. })
    ));
    assert!(!fields[1].is_punned());
    assert!(matches!(fields[2], ShapeField::Splat { .. }));
    assert_eq!(fields[2].span().text(src), "...$rest");
}

#[test]
fn destructuring_patterns() {
    let arena = Bump::new();
    let src = arena.alloc_str(&format!(
        "{FEATURES}function f(): void {{ shape('a' => $a, ?'b' => tuple($b, optional $c, ...), ...) = $s; }}"
    ));
    let aast::Expr_::Assign(assign) = lowered(&arena, src).2 else {
        panic!()
    };
    let aast::Expr_::DestructureShape(shape) = assign.0.2 else {
        panic!()
    };
    assert!(shape.ellipsis);
    assert!(!shape.fields[0].optional && shape.fields[1].optional);
    let aast::DestructureTarget_::DtTuple(tuple) = shape.fields[1].target.2 else {
        panic!()
    };
    assert!(tuple.ellipsis);
    assert!(!tuple.entries[0].optional && tuple.entries[1].optional);
}

#[test]
fn pattern_syntax_outside_patterns() {
    let src = format!("{FEATURES}function f(): void {{ $x = shape(?'a' => $a, ...); }}");
    assert_eq!(
        messages(&src),
        [
            "Shape field name must be a nonempty single-quoted string or a class constant",
            "Ellipsis `...` is only valid in shape destructuring patterns",
            "Optional field syntax `?` is only valid in shape destructuring patterns",
        ]
    );
    let src = format!("{FEATURES}function f(): void {{ tuple($a->b) = $t; }}");
    assert_eq!(
        messages(&src),
        [
            "Only variables, wildcards, and nested shape/tuple patterns are allowed in destructuring patterns"
        ]
    );
}

#[test]
fn match_statement() {
    let arena = Bump::new();
    let src = arena.alloc_str(&format!(
        "{FEATURES}function f(mixed $x): void {{ match ($x) {{ _: int => {{ g(); }} _ => {{}} }} }}"
    ));
    let StmtKind::Match(m) = body(&arena, src)[0].kind else {
        panic!()
    };
    assert_eq!(m.arms.len(), 2);
    assert!(matches!(
        m.arms[0].pattern.binding,
        PatternBinding::Wildcard("_")
    ));
    assert!(m.arms[0].pattern.hint.is_some());
    assert!(m.arms[1].pattern.hint.is_none() && m.arms[1].body.is_empty());
    assert_eq!(
        messages("function f(mixed $x): void { match ($x) { $y => {} } }"),
        [
            "Cannot use unstable feature: `match_statements`",
            "Binding a variable in a pattern is not yet supported."
        ]
    );
}

#[test]
fn polymorphic_functions() {
    let arena = Bump::new();
    let src = "function f((function<T as arraykey>(T): T) $g): void { $a = function<T>(T $x): T ==> $x; $b = async function<T>(T $x): Awaitable<T> { return $x; }; }";
    let out = parse(&arena, src).unwrap();
    assert_eq!(out.errors, []);
    let Def::Fun(f) = out.program.defs[0] else {
        panic!()
    };
    let HintKind::Fun(hint) = f.fun.params[0].hint.unwrap().kind else {
        panic!()
    };
    assert_eq!(hint.tparams[0].name.name, "T");
    assert_eq!(hint.tparams[0].constraints.len(), 1);
    fn assigned<'a>(stmt: &Stmt<'a>) -> ExprKind<'a> {
        match stmt.kind {
            StmtKind::Expr(Expr {
                kind: ExprKind::Assign(assign),
                ..
            }) => assign.rhs.kind,
            ref other => panic!("{other:?}"),
        }
    }
    assert!(matches!(assigned(&f.fun.body[0]), ExprKind::Lfun(fun) if fun.tparams.len() == 1));
    assert!(
        matches!(assigned(&f.fun.body[1]), ExprKind::Efun(efun) if efun.fun.tparams.len() == 1)
    );
    assert_eq!(
        messages("function f(): void { $a = function<reify T>(T $x): T ==> $x; }"),
        [
            "Reified generics are not allowed in polymorphic function hints or polymorphic lambda signatures."
        ]
    );
}

#[test]
fn class_declarations() {
    let arena = Bump::new();
    let src = arena.alloc_str(&format!(
        "{FEATURES}class A<T> = B<T>;\nabstract enum class E: int {{}}\ntrait Tr {{ require this as C; }}\ncase type CT<T> = int where T super int | string;"
    ));
    let out = parse(&arena, src).unwrap();
    assert_eq!(out.errors, []);
    let defs = out.program.defs;
    let Def::ClassAlias(alias) = defs[1] else {
        panic!()
    };
    assert_eq!((alias.name.name, alias.original.name), ("A", "B"));
    assert_eq!(alias.original_tparams.len(), 1);
    assert!(matches!(
        defs[2],
        Def::Class(c) if c.kind == ClassKind::EnumClass && c.flags & class_flags::ABSTRACT != 0
    ));
    let Def::Class(tr) = defs[3] else { panic!() };
    assert!(matches!(tr.members[0], Member::Require(r) if r.kind == RequireKind::ThisAs));
    let Def::Typedef(ct) = defs[4] else { panic!() };
    assert_eq!(ct.variants[0].where_constraints.len(), 1);
    assert!(ct.variants[1].where_constraints.is_empty());
}

#[test]
fn parameters_and_types() {
    let arena = Bump::new();
    let src = arena.alloc_str(&format!(
        "{FEATURES}function f(tuple<int, string> $t, named int...): void {{ try {{}} catch (Foo<int> $e) {{}} }}"
    ));
    let out = parse(&arena, src).unwrap();
    assert_eq!(out.errors, []);
    let Def::Fun(f) = out.program.defs[1] else {
        panic!()
    };
    let (name, args) = f.fun.params[0].hint.unwrap().as_named().unwrap();
    assert_eq!((name, args.len()), ("tuple", 2));
    assert_eq!(f.fun.params[1].name.name, "...");
    let StmtKind::Try(t) = f.fun.body[0].kind else {
        panic!()
    };
    assert_eq!(t.catches[0].class.name, "Foo<int>");
}

#[test]
fn package_and_delay() {
    let arena = Bump::new();
    let src = "function f(): void { package foo; package default; delay g(); }";
    let stmts = body(&arena, src);
    let name = |stmt: &Stmt<'_>| match stmt.kind {
        StmtKind::Expr(Expr {
            kind: ExprKind::Package(id),
            ..
        }) => id.name.to_string(),
        ref other => panic!("{other:?}"),
    };
    assert_eq!(
        (name(&stmts[0]), name(&stmts[1])),
        ("foo".into(), "default".into())
    );
    assert!(matches!(
        stmts[2].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::Delay(_),
            ..
        })
    ));
}

#[test]
fn prefixed_strings() {
    assert_eq!(
        messages("function f(): void { $x = sql\"a\"; }"),
        ["Only `re`-prefixed strings allowed."]
    );
    assert_eq!(messages("function f(): void { $x = re\"a\"; }"), [""; 0]);
}

#[test]
fn expression_trees() {
    let arena = Bump::new();
    let src = "function f(int $x): void { Dsl`1 + ${ g($x) }`; }";
    let ExprKind::ExpressionTree(tree) = first_expr(&arena, src).kind else {
        panic!()
    };
    assert_eq!(tree.prefix.unwrap().name, "Dsl");
    assert_eq!(tree.body_span.text(src), "1 + ${ g($x) }");
    let EtBody::Expr(Expr {
        kind: ExprKind::Binop(sum),
        ..
    }) = tree.body
    else {
        panic!()
    };
    assert!(matches!(sum.rhs.kind, ExprKind::EtSplice(_)));

    // Lowered, the literal is a call that builds the tree at run time, with
    // the splice evaluated first.
    let aast::Expr_::ExpressionTree(et) = lowered(&arena, src).2 else {
        panic!()
    };
    assert_eq!(et.class.1, "Dsl");
    assert!(et.free_vars.is_none());
    assert!(matches!(et.runtime_expr.2, aast::Expr_::Call(_)));

    assert_eq!(
        messages("function f(): void { Dsl`1 + new Foo()`; }"),
        ["`new` is not supported in expression trees."]
    );
    assert_eq!(
        messages("function f(): void { `1`; ${ g() }; }"),
        [
            "Expression trees must provide the DSL name.",
            "Splicing can only occur inside expression tree literals (between backticks)",
            "Splice syntax ${...} can only occur inside expression trees Foo``...``.",
        ]
    );
}
