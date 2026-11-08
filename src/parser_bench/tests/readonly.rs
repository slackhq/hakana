//! Holds the native port of HHVM's readonly analysis
//! (`hakana_parser::readonly_check`) to HHVM's own pass: given the same
//! tree, both have to report the same errors, with the same positions and
//! messages, in the same order.
//!
//! The comparison ([`compare_readonly`]) runs both passes over the tree the
//! native parser builds, so any text that parses is a test case, valid Hack
//! or not. Three sources of cases:
//!
//! * every Hack file in this repository (Hakana's tests, the HSL and HHVM's
//!   test suites, where readonly errors actually occur) as it is;
//! * the same files with `readonly` injected by textual substitution, which
//!   makes the pass work on thousands of files that never mention it;
//! * randomly generated programs dense in the constructs the pass treats
//!   specially, to get at the corners no real file reaches (how an
//!   expression the pass has marked readonly is seen again later, above
//!   all).
//!
//! A last test checks the wiring: the errors of the pass sit where HHVM
//! puts them among the other errors of a file.
//!
//! `parse-bench readonly <file-list>` and `parse-bench readonly-injected
//! <file-list>` run the first two over any other corpus.

use std::path::{Path, PathBuf};

use hakana_parser_bench::{
    Native, ReportedError, compare_readonly, hhvm, native, readonly_variants,
};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("hack" | "php")
        ) {
            out.push(path);
        }
    }
}

/// Every Hack file checked into this repository.
fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = vec![];
    collect(&root.join("tests"), &mut files);
    collect(&root.join("third-party/hhvm/hphp/hsl"), &mut files);
    collect(&root.join("third-party/hhvm/hphp/hack/test"), &mut files);
    collect(&root.join("third-party/hhvm/hphp/test"), &mut files);
    files.sort();
    assert!(
        files.len() > 20_000,
        "expected the test corpus and HHVM's test suites, found {} files",
        files.len()
    );
    files
}

/// Runs `check` over `items` on all cores, with stacks deep enough for the
/// most nested files, and returns the results in order.
fn in_parallel<T: Sync, R: Send>(items: &[T], check: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = items.len().div_ceil(threads).max(1);
    let check = &check;
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|items| {
                std::thread::Builder::new()
                    .stack_size(64 * 1024 * 1024)
                    .spawn_scoped(scope, move || items.iter().map(check).collect::<Vec<R>>())
                    .unwrap()
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect()
    })
}

/// What a set of comparisons amounted to.
#[derive(Default)]
struct Tally {
    /// Trees both passes ran over.
    trees: usize,
    /// Those of sources that use `readonly` where HHVM cares.
    use_readonly: usize,
    /// Those the pass reports errors for, and the errors in total.
    with_errors: usize,
    errors: usize,
    differences: Vec<String>,
}

impl Tally {
    fn compare(&mut self, name: &str, source: &str) {
        let Some(outcome) = compare_readonly(name, source) else {
            return;
        };
        self.trees += 1;
        self.use_readonly += outcome.uses_readonly as usize;
        self.with_errors += (outcome.errors > 0) as usize;
        self.errors += outcome.errors;
        if let Some(difference) = outcome.difference {
            self.differences.push(format!("{name}: {difference}"));
        }
    }

    fn add(&mut self, other: Tally) {
        self.trees += other.trees;
        self.use_readonly += other.use_readonly;
        self.with_errors += other.with_errors;
        self.errors += other.errors;
        self.differences.extend(other.differences);
    }

    fn assert_no_differences(&self, what: &str) {
        eprintln!(
            "{what}: {} trees compared; {} use readonly; {} with errors ({} errors); {} differences",
            self.trees,
            self.use_readonly,
            self.with_errors,
            self.errors,
            self.differences.len()
        );
        assert!(
            self.differences.is_empty(),
            "the native readonly analysis differs from HHVM's on {} of {what}:\n{}",
            self.differences.len(),
            self.differences[..self.differences.len().min(50)].join("\n")
        );
    }
}

#[test]
fn same_errors_as_hhvm_on_the_corpus() {
    let files = corpus();
    let mut as_is = Tally::default();
    let mut injected = Tally::default();
    for (file_as_is, file_injected) in in_parallel(&files, |path| {
        let (mut as_is, mut injected) = (Tally::default(), Tally::default());
        if let Ok(contents) = std::fs::read_to_string(path) {
            let name = path.display().to_string();
            as_is.compare(&name, &contents);
            for (n, variant) in readonly_variants(&contents).iter().enumerate() {
                injected.compare(&format!("{name} (variant {n})"), variant);
            }
        }
        (as_is, injected)
    }) {
        as_is.add(file_as_is);
        injected.add(file_injected);
    }
    as_is.assert_no_differences("the files of the corpus");
    injected.assert_no_differences("the files of the corpus with `readonly` injected");
    // The comparison is only worth something while it has errors to compare.
    assert!(
        as_is.use_readonly > 250 && as_is.with_errors > 60,
        "the corpus no longer exercises the readonly analysis"
    );
    assert!(
        injected.with_errors > 8_000 && injected.errors > 25_000,
        "the injected variants no longer exercise the readonly analysis"
    );
}

/// A generator of programs made of what the readonly analysis looks at.
/// The programs only have to parse. There are two kinds: broad ones
/// ([`program`](Self::program)), with assignments of every shape, calls,
/// loops and branches, closures and pipes, and `readonly` sprinkled
/// throughout; and focused ones ([`focused_program`](Self::focused_program)).
struct Generator {
    state: u64,
    out: String,
    /// Inside the right-hand side of a pipe, where `$$` exists.
    in_pipe: bool,
    /// Inside a loop, where `break` and `continue` may appear.
    in_loop: bool,
}

const VARIABLES: [&str; 8] = ["$a", "$b", "$c", "$d", "$ro", "$io", "$this", "$g"];

impl Generator {
    fn new(seed: u64) -> Self {
        Generator {
            state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03,
            out: String::new(),
            in_pipe: false,
            in_loop: false,
        }
    }

    /// The next number of a SplitMix64 sequence, below `n`.
    fn below(&mut self, n: u64) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn put(&mut self, text: &str) {
        self.out.push_str(text);
    }

    fn pick(&mut self, choices: &[&str]) {
        let choice = choices[self.below(choices.len() as u64) as usize];
        self.put(choice);
    }

    fn variable(&mut self) {
        if self.in_pipe && self.chance(25) {
            self.put("$$");
        } else {
            self.pick(&VARIABLES);
        }
    }

    /// A local variable (not `$this`).
    fn local(&mut self) {
        self.pick(&VARIABLES[..6]);
    }

    fn exprs(&mut self, depth: u32, separator: &str, max: u64) {
        for i in 0..self.below(max + 1) {
            if i > 0 {
                self.put(separator);
            }
            self.expr(depth);
        }
    }

    fn arguments(&mut self, depth: u32) {
        self.put("(");
        for i in 0..self.below(4) {
            if i > 0 {
                self.put(", ");
            }
            match self.below(8) {
                0 => {
                    self.put("inout ");
                    self.lvalue(depth);
                }
                1 => {
                    self.put("readonly ");
                    self.expr(depth);
                }
                _ => self.expr(depth),
            }
        }
        self.put(")");
    }

    fn atom(&mut self) {
        match self.below(12) {
            0 => self.put("1"),
            1 => self.put("'s'"),
            2 => self.put("null"),
            3 => self.put("C::$s"),
            4 => self.put("C::K"),
            5 if self.in_pipe => self.put("$$"),
            _ => self.variable(),
        }
    }

    fn expr(&mut self, depth: u32) {
        if depth == 0 {
            return self.atom();
        }
        let depth = depth - 1;
        match self.below(60) {
            0..=5 => self.atom(),
            6..=8 => {
                self.put("readonly ");
                self.expr(depth);
            }
            9..=10 => {
                self.expr(depth);
                self.pick(&["->p", "?->p", "->q"]);
            }
            11..=12 => {
                self.expr(depth);
                self.put("[");
                self.expr(depth);
                self.put("]");
            }
            // Calls: plain, on a value (the callee may be readonly), and
            // the builtins whose result is as readonly as their argument.
            13..=14 => {
                self.pick(&["f", "g", "C::sm", "idx", "\\f"]);
                self.arguments(depth);
            }
            15 => {
                self.expr(depth);
                self.put("->m");
                self.arguments(depth);
            }
            16 => {
                self.variable();
                self.arguments(depth);
            }
            17 => {
                self.put("(");
                self.expr(depth);
                self.put(")");
                self.arguments(depth);
            }
            18..=21 => {
                self.pick(&[
                    "vec",
                    "dict",
                    "varray",
                    "darray",
                    "\\vec",
                    "HH\\vec",
                    "\\HH\\dict",
                    "hphp_array_idx",
                    "\\hphp_array_idx",
                    "HH\\FIXME\\UNSAFE_CAST<mixed, C>",
                    "\\HH\\FIXME\\UNSAFE_NONNULL_CAST",
                    "keyset",
                ]);
                self.arguments(depth);
            }
            22..=23 => {
                self.put("new C");
                self.arguments(depth);
            }
            24 => {
                self.pick(&["vec[", "keyset[", "Vector {", "Set {", "tuple("]);
                let close = match self.out.as_bytes()[self.out.len() - 1] {
                    b'[' => "]",
                    b'{' => "}",
                    _ => ")",
                };
                self.exprs(depth, ", ", 3);
                self.put(close);
            }
            25 => {
                self.pick(&["dict[", "Map {", "shape("]);
                let close = match self.out.as_bytes()[self.out.len() - 1] {
                    b'[' => "]",
                    b'{' => "}",
                    _ => ")",
                };
                for i in 0..self.below(3) {
                    if i > 0 {
                        self.put(", ");
                    }
                    self.pick(&["'k' => ", "'l' => "]);
                    self.expr(depth);
                }
                self.put(close);
            }
            26 => {
                self.put("Pair {");
                self.expr(depth);
                self.put(", ");
                self.expr(depth);
                self.put("}");
            }
            27..=29 => {
                self.put("(");
                self.expr(depth);
                self.pick(&[" ?? ", " ?: ", " + ", " . ", " === ", " && "]);
                self.expr(depth);
                self.put(")");
            }
            30 => {
                self.put("(");
                self.expr(depth);
                self.put(" ? ");
                self.expr(depth);
                self.put(" : ");
                self.expr(depth);
                self.put(")");
            }
            31..=32 => {
                self.put("(");
                self.expr(depth);
                self.pick(&[
                    " as int)",
                    " as string)",
                    " as arraykey)",
                    " as \\int)",
                    " as C)",
                    " ?as C)",
                    " as nonnull)",
                    " is C)",
                ]);
            }
            33 => {
                self.pick(&["clone ", "await ", "(int)", "!", "-"]);
                self.expr(depth);
            }
            // Pipes: `$$` is whatever the left-hand side is.
            34..=36 => {
                self.put("(");
                self.expr(depth);
                self.put(" |> ");
                let outer = std::mem::replace(&mut self.in_pipe, true);
                self.expr(depth);
                self.in_pipe = outer;
                self.put(")");
            }
            // Assignments and increments as expressions.
            37..=39 => {
                self.put("(");
                self.assignment(depth);
                self.put(")");
            }
            // Closures see the locals of the function around them.
            40..=41 => {
                self.pick(&[
                    "(() ==> ",
                    "(($x) ==> ",
                    "((readonly $a) ==> ",
                    "((inout $io) ==> ",
                    "((C $x = $a) ==> ",
                    "((readonly C $x = $ro) ==> ",
                    "(readonly ($x) ==> ",
                    "(async () ==> ",
                    "((): readonly C ==> ",
                    "(<<__IgnoreReadonlyLocalErrors>> () ==> ",
                ]);
                if self.chance(50) {
                    self.expr(depth);
                } else {
                    self.block(depth);
                }
                self.put(")");
            }
            42 => {
                self.pick(&[
                    "(function() ",
                    "(function(readonly C $b, inout $io): readonly C use ($a) ",
                    "(readonly function() use ($a, $ro) ",
                    "(async function($x = $ro) use ($b) ",
                ]);
                self.block(depth);
                self.put(")");
            }
            43 => {
                self.put("<x:y a={");
                self.expr(depth);
                self.put("} b=\"s\" {...");
                self.expr(depth);
                self.put("}>{");
                self.expr(depth);
                self.put("}</x:y>");
            }
            44 => {
                self.put("\"a {");
                self.variable();
                self.put("->p} ");
                self.local();
                self.put("\"");
            }
            _ => self.atom(),
        }
    }

    /// Something to assign to.
    fn lvalue(&mut self, depth: u32) {
        match self.below(20) {
            0..=5 => self.local(),
            6..=8 => {
                self.variable();
                self.put("[");
                self.expr(depth);
                self.put("]");
            }
            9 => {
                self.variable();
                self.put("[]");
            }
            10..=11 => {
                self.variable();
                self.pick(&["->p", "->p->q", "?->p"]);
            }
            12 => {
                self.variable();
                self.put("->p[");
                self.expr(depth);
                self.put("]");
            }
            13 => {
                self.variable();
                self.put("[");
                self.expr(depth);
                self.put("]->p");
            }
            14 => self.pick(&["C::$s", "C::$s['k']", "C::$s[0][1]", "C::$s->p"]),
            15 => {
                self.pick(&["f()", "(readonly $a)", "vec($a)", "$a->m()", "C::K"]);
                self.pick(&["", "[0]", "['k'][1]", "->p"]);
            }
            16 => {
                self.put("list(");
                for i in 0..self.below(4) {
                    if i > 0 {
                        self.put(", ");
                    }
                    if depth > 0 && self.chance(85) {
                        self.lvalue(depth - 1);
                    }
                }
                self.put(")");
            }
            17 => self.pattern(depth),
            _ => {
                self.local();
                self.put("[");
                self.expr(depth);
                self.put("][");
                self.expr(depth);
                self.put("]");
            }
        }
    }

    /// A tuple or shape pattern to destructure a value into.
    fn pattern(&mut self, depth: u32) {
        let shape = self.chance(40);
        self.put(if shape { "shape(" } else { "tuple(" });
        for i in 0..1 + self.below(3) {
            if i > 0 {
                self.put(", ");
            }
            if shape {
                self.pick(&["'a' => ", "'b' => ", "?'c' => "]);
            }
            match self.below(8) {
                0 if depth > 0 => self.pattern(depth - 1),
                1 => self.put("$_"),
                2 if depth > 0 => self.lvalue(depth - 1),
                _ => self.local(),
            }
        }
        if self.chance(20) {
            self.put(", ...");
        }
        self.put(")");
    }

    fn assignment(&mut self, depth: u32) {
        match self.below(10) {
            0 => {
                self.lvalue(depth);
                self.pick(&["++", "--"]);
            }
            1 => {
                self.pick(&["++", "--"]);
                self.lvalue(depth);
            }
            2 => {
                self.lvalue(depth);
                self.pick(&[" .= ", " += ", " ??= "]);
                self.expr(depth);
            }
            _ => {
                self.lvalue(depth);
                self.put(" = ");
                self.expr(depth);
            }
        }
    }

    fn block(&mut self, depth: u32) {
        self.put("{ ");
        for _ in 0..self.below(4) {
            self.stmt(depth);
        }
        self.put("} ");
    }

    fn loop_body(&mut self, depth: u32) {
        let outer = std::mem::replace(&mut self.in_loop, true);
        self.block(depth);
        self.in_loop = outer;
    }

    fn stmt(&mut self, depth: u32) {
        if depth == 0 {
            self.assignment(1);
            return self.put("; ");
        }
        let depth = depth - 1;
        match self.below(40) {
            0..=11 => {
                self.assignment(depth);
                self.put("; ");
            }
            12..=14 => {
                self.expr(depth);
                self.put("; ");
            }
            15..=17 => {
                self.put("if (");
                self.expr(depth);
                self.put(") ");
                self.block(depth);
                if self.chance(60) {
                    self.put("else ");
                    self.block(depth);
                }
            }
            18..=19 => {
                self.put("while (");
                self.expr(depth);
                self.put(") ");
                self.loop_body(depth);
            }
            20..=22 => {
                self.put("do ");
                self.loop_body(depth);
                self.put("while (");
                self.expr(depth);
                self.put("); ");
            }
            23 => {
                self.put("for (");
                self.exprs(depth, ", ", 2);
                self.put("; ");
                self.exprs(depth, ", ", 1);
                self.put("; ");
                self.exprs(depth, ", ", 2);
                self.put(") ");
                self.loop_body(depth);
            }
            24..=26 => {
                self.put("foreach (");
                self.expr(depth);
                self.pick(&[" as ", " as $k => ", " await as ", " await as $k => "]);
                if self.chance(80) {
                    self.local();
                } else {
                    self.lvalue(depth);
                }
                self.put(") ");
                self.loop_body(depth);
            }
            27 => {
                self.put("switch (");
                self.expr(depth);
                self.put(") { ");
                for _ in 0..self.below(3) {
                    self.put("case ");
                    self.expr(depth);
                    self.put(": ");
                    self.stmt(depth);
                    self.pick(&["break; ", ""]);
                }
                if self.chance(50) {
                    self.put("default: ");
                    self.stmt(depth);
                }
                self.put("} ");
            }
            28..=29 => {
                self.put("try ");
                self.block(depth);
                for _ in 0..self.below(3) {
                    self.put("catch (Exception $e) ");
                    self.block(depth);
                }
                self.put("finally ");
                self.block(depth);
            }
            30..=31 => {
                self.put("return ");
                self.expr(depth);
                self.put("; ");
            }
            32 => {
                self.put("throw ");
                self.expr(depth);
                self.put("; ");
            }
            33 => {
                self.put("concurrent { ");
                for _ in 0..1 + self.below(3) {
                    if self.chance(70) {
                        self.local();
                        self.put(" = ");
                    }
                    self.put("await ");
                    self.expr(depth);
                    self.put("; ");
                }
                self.put("} ");
            }
            34 => {
                self.put("using (");
                self.expr(depth);
                self.put(") ");
                self.block(depth);
            }
            35 if self.in_loop => self.pick(&["break; ", "continue; "]),
            36 => {
                self.put("echo ");
                self.expr(depth);
                self.put("; ");
            }
            37 => {
                self.pick(&["yield ", "yield 'k' => ", "$a = yield ", "yield $b => "]);
                self.expr(depth);
                self.put("; ");
            }
            _ => self.block(depth),
        }
    }

    fn parameters(&mut self) {
        self.put("(");
        self.pick(&["readonly C $ro", "C $ro", "readonly C $ro = C::K"]);
        self.pick(&[
            ", inout vec<C> $io",
            ", vec<C> $io",
            ", inout readonly C $io",
        ]);
        if self.chance(40) {
            self.put(", mixed $a = ");
            self.expr(1);
        }
        if self.chance(20) {
            self.put(", readonly mixed $b = ");
            self.expr(1);
        }
        self.put(")");
        self.pick(&[": mixed ", ": readonly mixed ", " "]);
    }

    fn attribute(&mut self) {
        if self.chance(25) {
            self.put("<<A(");
            self.exprs(2, ", ", 2);
            self.put(")>> ");
        } else if self.chance(5) {
            self.put("<<__IgnoreReadonlyLocalErrors>> ");
        }
    }

    // The focused programs: three variables, and little else than what
    // makes the pass mark an expression readonly (assignments into
    // collections, arguments of calls) inside loops, between assignments
    // that make the variables readonly or mutable again. The pass analyses
    // a loop body several times, and what it marked the first time must be
    // seen as readonly the next, whatever the variables hold by then.

    fn focused_variable(&mut self) {
        self.pick(&["$a", "$b", "$c"]);
    }

    fn focused_expr(&mut self) {
        match self.below(16) {
            0..=4 => self.focused_variable(),
            5..=7 => {
                self.pick(&["vec(", "dict(", "f(", "new C(", "f(vec(", "g(1, "]);
                let nested = self.out.ends_with("f(vec(");
                self.focused_variable();
                self.put(if nested { "))" } else { ")" });
            }
            8 => {
                self.put("(");
                self.focused_expr();
                self.pick(&[" ?? ", " ?: "]);
                self.focused_expr();
                self.put(")");
            }
            9 => {
                self.focused_variable();
                self.pick(&["[0]", "->p", "->m()"]);
            }
            10 => {
                self.put("(");
                self.focused_variable();
                self.put(" = ");
                self.focused_expr();
                self.put(")");
            }
            11 => {
                self.put("readonly ");
                self.focused_variable();
            }
            12 => {
                self.put("(");
                self.focused_expr();
                self.pick(&[" |> vec($$))", " |> f($$, $$->p = 1))", " |> $$)"]);
            }
            13 => self.pick(&["new C()", "readonly new C()"]),
            _ => self.put("1"),
        }
    }

    /// Somewhere a value can be stored that is not just a local.
    fn focused_target(&mut self) {
        if self.chance(15) {
            return self.pick(&["C::$s", "C::$s[0]", "C::$s[0][1]"]);
        }
        self.focused_variable();
        self.pick(&["[0]", "[0][1]", "[]", "->p", "->p[0]", "[0]->p"]);
    }

    fn focused_block(&mut self, depth: u32) {
        self.put("{ ");
        for _ in 0..1 + self.below(4) {
            self.focused_stmt(depth);
        }
        self.put("} ");
    }

    fn focused_stmt(&mut self, depth: u32) {
        match self.below(if depth == 0 { 15 } else { 22 }) {
            0..=3 => {
                self.focused_variable();
                self.pick(&[" = readonly new C(); ", " = new C(); ", " = vec[]; "]);
            }
            4 => {
                self.focused_variable();
                self.put(" = ");
                self.focused_expr();
                self.put("; ");
            }
            5..=8 => {
                self.focused_target();
                self.pick(&[" = ", " = ", " .= "]);
                self.focused_expr();
                self.put("; ");
            }
            9..=11 => {
                // An element may be left out of a `list`, not of a `tuple`.
                let list = self.chance(65);
                self.put(if list { "list(" } else { "tuple(" });
                for i in 0..2 + self.below(2) {
                    if i > 0 {
                        self.put(", ");
                    }
                    match self.below(6) {
                        0 if list => {}
                        0 | 1 => self.focused_variable(),
                        _ => self.focused_target(),
                    }
                }
                self.put(") = ");
                self.focused_expr();
                self.put("; ");
            }
            12 => {
                self.focused_expr();
                self.put("; ");
            }
            13 => {
                if self.chance(50) {
                    self.focused_variable();
                } else {
                    self.focused_target();
                }
                self.pick(&["++; ", "--; "]);
            }
            14 => {
                self.pick(&["return ", "throw ", "yield "]);
                self.focused_expr();
                self.put("; ");
            }
            15..=18 => {
                self.put("do ");
                self.focused_block(depth - 1);
                self.put("while (");
                self.focused_expr();
                self.put("); ");
            }
            19 => {
                self.put("while (");
                self.focused_expr();
                self.put(") ");
                self.focused_block(depth - 1);
            }
            20 => {
                self.put("foreach (");
                self.focused_expr();
                self.put(" as ");
                self.focused_variable();
                self.put(") ");
                self.focused_block(depth - 1);
            }
            _ => {
                self.put("if (");
                self.focused_expr();
                self.put(") ");
                self.focused_block(depth - 1);
                self.put("else ");
                self.focused_block(depth - 1);
            }
        }
    }

    fn focused_program(mut self) -> String {
        self.put("function f(");
        self.pick(&["readonly C $a", "C $a"]);
        self.pick(&[", readonly C $b", ", C $b"]);
        self.pick(&[", readonly C $c", ", C $c", ""]);
        self.put("): ");
        self.pick(&["readonly mixed ", "mixed "]);
        self.focused_block(3);
        self.out
    }

    /// A whole file: statements at the top level (their locals carry into
    /// class members that are not methods), a class and a function.
    fn program(mut self) -> String {
        if self.chance(40) {
            self.put("$g = readonly new C(); ");
        }
        for _ in 0..self.below(3) {
            self.stmt(2);
        }
        self.put("\n");
        self.attribute();
        self.put("class C {\n");
        self.put("  const K = ");
        self.expr(1);
        self.put(";\n  ");
        self.attribute();
        self.put("public static $s = ");
        self.expr(2);
        self.put(";\n");
        if self.chance(50) {
            self.put("  public function __construct(");
            self.attribute();
            self.put("public readonly C $q, ");
            self.attribute();
            self.put("private C $r = ");
            self.expr(1);
            self.put(") ");
            self.block(2);
            self.put("\n");
        }
        for _ in 0..1 + self.below(2) {
            self.put("  ");
            self.attribute();
            self.pick(&[
                "public function m",
                "public readonly function m",
                "public async function m",
                "public static function sm",
            ]);
            self.parameters();
            self.block(3);
            self.put("\n");
        }
        self.put("}\n");
        self.attribute();
        self.pick(&["function f", "async function f"]);
        self.parameters();
        self.block(4);
        self.put("\n");
        for _ in 0..self.below(2) {
            self.stmt(2);
        }
        self.out
    }
}

#[test]
fn same_errors_as_hhvm_on_generated_programs() {
    // READONLY_GENERATED=<n> runs more (or fewer) programs.
    let count: u64 = std::env::var("READONLY_GENERATED")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(200_000);
    let seeds: Vec<u64> = (0..count).collect();
    let mut tally = Tally::default();
    for program in in_parallel(&seeds, |&seed| {
        let generator = Generator::new(seed);
        let source = if seed % 2 == 0 {
            generator.program()
        } else {
            generator.focused_program()
        };
        let mut tally = Tally::default();
        tally.compare(&format!("generated program {seed}"), &source);
        if std::env::var_os("SHOW_FAILURES").is_some() && !tally.differences.is_empty() {
            eprintln!("generated program {seed}:\n{source}");
        }
        tally
    }) {
        tally.add(program);
    }
    tally.assert_no_differences("the generated programs");
    // The generator has to keep producing programs that parse and that the
    // analysis has something to say about.
    assert!(
        tally.trees * 10 > count as usize * 9,
        "only {} of {count} generated programs parse",
        tally.trees
    );
    assert!(
        tally.with_errors * 2 > tally.trees,
        "only {} of {} generated programs have readonly errors",
        tally.with_errors,
        tally.trees
    );
}

/// Whether `error` is one of the readonly analysis.
fn from_readonly_analysis(error: &ReportedError) -> bool {
    [
        "This expression is readonly",
        "Cannot write a readonly value to an inout parameter",
        "This exception is readonly;",
        "Readonly values cannot be written to xhp attributes",
    ]
    .iter()
    .any(|start| error.message.starts_with(start))
}

/// Whether the errors of the readonly analysis are where HHVM reports them
/// among the errors of a source: `None` unless there are some and both
/// parsers report the same errors overall. The second flag tells whether
/// there are errors from other passes too.
fn placed_like_hhvm(name: &str, source: &str) -> Option<(bool, bool)> {
    let outcome = compare_readonly(name, source)?;
    if !outcome.uses_readonly || outcome.errors == 0 {
        return None;
    }
    let Native::Parsed { errors, .. } = native(name, source) else {
        return None;
    };
    let (_, _, hhvm_errors) = std::panic::catch_unwind(|| hhvm(name, source)).ok()?.ok()?;
    let (mut sorted, mut hhvm_sorted) = (errors.clone(), hhvm_errors.clone());
    sorted.sort();
    hhvm_sorted.sort();
    if sorted != hhvm_sorted {
        // Not the same errors to begin with; `full_fidelity_cases` and
        // `parse-bench errors` keep track of those.
        return None;
    }
    // The same errors of the analysis at the same places in both lists.
    // (The order HHVM gives two errors of another pass for the same
    // piece of code is not this test's business.)
    let placed = |errors: &[ReportedError]| -> Vec<(usize, ReportedError)> {
        errors
            .iter()
            .cloned()
            .enumerate()
            .filter(|(_, error)| from_readonly_analysis(error))
            .collect()
    };
    let native_placed = placed(&errors);
    Some((
        native_placed == placed(&hhvm_errors),
        errors.len() > native_placed.len(),
    ))
}

#[test]
fn errors_sit_where_hhvm_puts_them() {
    // HHVM reports the errors of the readonly analysis after those of its
    // syntax checks and before those of its other checks over the AST. The
    // files with `readonly` injected are the ones that have all three.
    let files = corpus();
    let results = in_parallel(&files, |path| {
        let mut results = vec![];
        if let Ok(contents) = std::fs::read_to_string(path) {
            let name = path.display().to_string();
            results.extend(placed_like_hhvm(&name, &contents).map(|order| (name.clone(), order)));
            for (n, variant) in readonly_variants(&contents).iter().enumerate() {
                let name = format!("{name} (variant {n})");
                results.extend(placed_like_hhvm(&name, variant).map(|order| (name, order)));
            }
        }
        results
    });
    let compared: Vec<_> = results.into_iter().flatten().collect();
    let out_of_order: Vec<&str> = compared
        .iter()
        .filter(|(_, (same_place, _))| !same_place)
        .map(|(name, _)| name.as_str())
        .collect();
    let mixed = compared.iter().filter(|(_, (_, mixed))| *mixed).count();
    eprintln!(
        "sources with readonly errors and the same errors as HHVM: {}; with other errors too: {mixed}; readonly errors elsewhere than in HHVM's list: {}",
        compared.len(),
        out_of_order.len()
    );
    assert!(
        out_of_order.is_empty(),
        "the errors of the readonly analysis are not where HHVM reports them in {} sources:\n{}",
        out_of_order.len(),
        out_of_order[..out_of_order.len().min(50)].join("\n")
    );
    assert!(
        compared.len() > 8_000 && mixed > 1_500,
        "the corpus no longer says where readonly errors belong"
    );
}

/// The errors both parsers report for `source`, in order.
fn both_errors(source: &str) -> (Vec<ReportedError>, Vec<ReportedError>) {
    let Native::Parsed { errors, .. } = native("case.hack", source) else {
        panic!("the native parser cannot parse:\n{source}");
    };
    let (_, _, hhvm_errors) = hhvm("case.hack", source).unwrap();
    (errors, hhvm_errors)
}

#[test]
fn marked_expressions_stay_readonly() {
    // HHVM's pass wraps the expressions it finds readonly in certain places
    // in `readonly (...)`, and sees the wrapper when it comes by again. In
    // both functions `$a` is mutable by the end of the loop body, and the
    // second analysis of the body starts from there.
    let source = "
function arguments(readonly C $a, C $b): void {
  do {
    // The argument of `vec` was readonly the first time, so the call
    // still is the second time.
    $b[0] = vec($a);
    $a = new C();
  } while (f());
}
function collections(readonly C $a, C $b): void {
  do {
    // `$a` was a readonly collection the first time, so `$b`, readonly
    // the second time, may still be stored in it.
    $a[0] = $b;
    $a = vec[];
    $b = readonly new C();
  } while (f());
}
";
    let (errors, hhvm_errors) = both_errors(source);
    assert_eq!(errors, hhvm_errors);
    let lines: Vec<usize> = errors
        .iter()
        .map(|e| source[..e.start].matches('\n').count() + 1)
        .collect();
    // Twice for `vec($a)`, never for `$a[0] = $b`.
    assert_eq!(lines, [6, 6], "{errors:#?}");
}

#[test]
fn nothing_reported_in_declarations_hhvm_drops() {
    // HHVM cannot translate `nameof string` and leaves the function it is in
    // out of its AST, so its readonly analysis never sees that function.
    let source = "
function dropped(readonly C $x): void {
  $x->prop = 1;
  $y = nameof string;
}
function kept(readonly C $x): void {
  $x->prop = 1;
}
";
    let (errors, hhvm_errors) = both_errors(source);
    assert_eq!(errors, hhvm_errors);
    assert_eq!(
        errors.iter().filter(|e| from_readonly_analysis(e)).count(),
        1,
        "{errors:#?}"
    );
}
