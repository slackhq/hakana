//! However deep a file nests, parsing it ends in a tree or in the "nesting
//! too deep" error, never in a stack overflow: the parser limits the depth
//! of the tree it builds so that it, and everything that then walks the
//! tree, has room on the stack of an ordinary thread.

use hakana_parser::{Bump, ParserOptions, parse_cst_named, parse_file};

/// A thread's default stack. The limit is set for optimised code; without
/// optimisation the same recursion takes several times the stack.
const STACK: usize = if cfg!(debug_assertions) {
    16 << 20
} else {
    2 << 20
};

/// Code that nests: `open`, `core` and `close` make the levels, the rest
/// is what they are in.
struct Nesting {
    name: &'static str,
    prefix: &'static str,
    open: &'static str,
    core: &'static str,
    close: &'static str,
    suffix: &'static str,
    /// How many levels there is certainly room for.
    fits: usize,
}

/// An expression, in a statement that every pass over the tree looks at:
/// it has `readonly`, an `await` and an assignment used for its value.
const fn expression(
    name: &'static str,
    open: &'static str,
    core: &'static str,
    close: &'static str,
    fits: usize,
) -> Nesting {
    Nesting {
        name,
        prefix: "async function f($a, $x): Awaitable<void> {\n  $y = readonly g($w = ",
        open,
        core,
        close,
        suffix: ", await h());\n}\n",
        fits,
    }
}

const NESTINGS: &[Nesting] = &[
    expression("vec", "vec[", "1", "]", 350),
    expression("call", "f(", "1", ")", 350),
    expression("shape", "shape('a' => ", "1", ")", 350),
    expression("parentheses", "(", "1", ")", 350),
    expression("unary", "!", "$x", "", 350),
    expression("lambda", "() ==> ", "1", "", 120),
    expression("closure", "function() { return ", "1", "; }", 90),
    expression("conditional", "$a ? 1 : ", "1", "", 350),
    expression("assignment", "$a = ", "1", "", 350),
    expression("pipe", "$a |> ", "$$", "", 350),
    expression("subscript", "", "$x", "[0]", 350),
    expression("member", "", "$x", "->a", 350),
    expression("call chain", "", "f", "()", 350),
    expression("operands", "1 + ", "1", "", 1900),
    expression("xhp", "<a>", "", "</a>", 350),
    expression("string", "\"{$a[", "1", "]}\"", 180),
    Nesting {
        name: "call in an expression tree",
        prefix: "function f($a, $x): void {\n  $y = Code`",
        open: "f(",
        core: "1",
        close: ")",
        suffix: "`;\n}\n",
        fits: 150,
    },
    Nesting {
        name: "operands in an expression tree",
        prefix: "function f($a, $x): void {\n  $y = Code`",
        open: "1 + ",
        core: "1",
        close: "",
        suffix: "`;\n}\n",
        fits: 150,
    },
    Nesting {
        name: "block",
        prefix: "function f($a): void {\n",
        open: "if ($a) {",
        core: "",
        close: "}",
        suffix: "\n}\n",
        fits: 100,
    },
    Nesting {
        name: "statement",
        prefix: "function f($a): void {\n",
        open: "while ($a) ",
        core: ";",
        close: "",
        suffix: "\n}\n",
        fits: 150,
    },
    Nesting {
        name: "function type",
        prefix: "function f(",
        open: "(function(",
        core: "int",
        close: "): void)",
        suffix: " $a): void {}\n",
        fits: 150,
    },
    Nesting {
        name: "type argument",
        prefix: "function f(",
        open: "vec<",
        core: "int",
        close: ">",
        suffix: " $a): void {}\n",
        fits: 350,
    },
    Nesting {
        name: "namespace",
        prefix: "",
        open: "namespace N {",
        core: "",
        close: "}",
        suffix: "\n",
        fits: 350,
    },
];

fn source(nesting: &Nesting, levels: usize) -> String {
    let mut src = String::from("<?hh\n");
    src.push_str(nesting.prefix);
    for _ in 0..levels {
        src.push_str(nesting.open);
    }
    src.push_str(nesting.core);
    for _ in 0..levels {
        src.push_str(nesting.close);
    }
    src.push_str(nesting.suffix);
    src
}

/// Runs the whole of what happens to a file, on a thread with the stack of
/// an ordinary one. Says whether the file parsed, into the analysis tree
/// and into the lossless one.
fn parses(src: String) -> (bool, bool) {
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(move || {
            let arena = Bump::new();
            let parsed = match parse_file(&arena, &src, "nested.php") {
                Ok(file) => !file.has_syntax_error,
                Err(_) => false,
            };
            let tree = parse_cst_named(&src, "nested.php", ParserOptions::default()).is_ok();
            (parsed, tree)
        })
        .unwrap()
        .join()
        .expect("the parser overflowed its stack or panicked")
}

#[test]
fn nesting_just_under_the_limit_parses() {
    for nesting in NESTINGS {
        assert_eq!(
            parses(source(nesting, nesting.fits)),
            (true, true),
            "{} levels of {} should parse",
            nesting.fits,
            nesting.name
        );
    }
}

#[test]
fn nesting_far_over_the_limit_is_declined() {
    for nesting in NESTINGS {
        for levels in [3_000, 100_000] {
            assert_eq!(
                parses(source(nesting, levels)),
                (false, false),
                "{levels} levels of {} should be too deep",
                nesting.name
            );
        }
    }
}
