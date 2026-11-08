//! HHVM is the reference exactly as it is checked out. These cases are the
//! ones a patched HHVM used to hide: block comments after a token (Hakana
//! attaches them differently on purpose - see `hakana_trivia` - yet has to
//! put errors and doc comments where HHVM does), `unset` of anything but a
//! container element, and `(bool)` casts.

use hakana_parser_bench::{
    CstOutcome, ErrorOutcome, Outcome, compare, compare_cst, compare_errors,
};

const CASES: &[(&str, &str)] = &[
    (
        "doc comments after a token",
        "class A { /** one */ public function f(): void {}
  public function g(): void {} /** two */ public function h(): void {}
  /** three */
  public function i(): void {}
}
function a(): void {} /** four */ function b(): void {}
",
    ),
    (
        "comments everywhere",
        "function f(int $a /* a */, /* b */ int $b): int { /* c */
  $x = $a /* d */ + /* e */ $b; /* f */ /* g */ // h
  /* HH_FIXME[4110] i */ return $x; /* j */
} /* k */
",
    ),
    (
        "missing semicolons",
        "function f(): void {
  $a = 1 /* c */
  $b = 2 /* d */ /* e */ // x
  foo($a /* k */
}
",
    ),
    (
        "missing pieces of declarations",
        "enum E /* a */ {}
function g(int /* q */ $x, /* r */): void {}
class C { const int X /* v */; }
namespace N /* z */
",
    ),
    (
        "a fixme does not trail either",
        "function f(): void {
  $a = 1 /* HH_FIXME[4110] c */
  $b = 2 /* HH_IGNORE_ERROR[1] d */
}
",
    ),
    (
        "enum without a base",
        "enum A/*range-start*//*range-end*/ {}\n",
    ),
    (
        "enum class without a base",
        "enum class A/*range-start*//*range-end*/ {}\n",
    ),
    (
        "lambda body",
        "function main(): void {
  $z = 100 +
    () ==>/*range-start*/300 + 2/*range-end*/
}
",
    ),
    (
        "unset of container elements",
        "function f(dict<string, int> $d, C $c): void {
  unset($d['a']);
  unset($d['a'], $d['b']);
  unset(($d['a']));
  unset($c->p['a']);
  unset(C::$s['a']);
  unset($d['a']['b']);
}
",
    ),
    (
        "unset of anything else",
        "function f(dict<string, int> $d, C $c, vec<int> $v): void {
  unset($d);
  unset($c->p);
  unset(C::$s);
  unset(($d));
  unset($c?->p);
  unset($d['a'], $v, $c->q);
  unset($this['a']);
  unset(foo());
  unset(foo()['a']);
  unset($d[]);
  unset($$);
  unset($c?->p['a']);
  unset(1 + 2);
  unset(await $x);
  unset(list($a, $b));
  unset(C::X);
  unset($c->:attr);
}
",
    ),
    // Not a matter of the patch: found by mutating real files.
    (
        "names written with gaps",
        "final class A extends Foo \\Bar<int> {}
function f(Foo   \\Bar $x): Foo \\Bar { return new Foo  \\Bar(); }
function g(Foo /* c */ \\Bar<int> $x): void { Foo \\Bar::m(); Foo
  \\baz(); }
",
    ),
    (
        "bool casts",
        "function f(int $i): bool {
  $a = (bool) $i;
  $b = (bool)($i + 1);
  return (bool) $a && $b;
}
",
    ),
];

#[test]
fn agrees_with_unpatched_hhvm() {
    let mut failures = vec![];
    for (name, source) in CASES {
        let path = format!("{}.hack", name.replace(' ', "_"));
        match compare(&path, source.to_string()) {
            Outcome::Same | Outcome::BothReject => {}
            Outcome::Mismatch { what, .. } => failures.push(format!("{name}: mismatch in {what}")),
            Outcome::NativeReportsErrors(e) | Outcome::HhvmReportsErrors(e) => {
                failures.push(format!("{name}: only one side reports `{e}`"))
            }
            Outcome::NativeDeclined | Outcome::HhvmRejects => {
                failures.push(format!("{name}: only one side parses it"))
            }
        }
        match compare_errors(&path, source) {
            ErrorOutcome::BothClean | ErrorOutcome::SameErrors => {}
            ErrorOutcome::DifferentOrder { native, hhvm }
            | ErrorOutcome::DifferentErrors { native, hhvm } => {
                failures.push(format!("{name}: errors: {native:?} but HHVM has {hhvm:?}"))
            }
            _ => failures.push(format!("{name}: errors on one side only")),
        }
        match compare_cst(&path, source) {
            CstOutcome::Same => {}
            CstOutcome::NoRecovery(e) | CstOutcome::NativeFailed(e) | CstOutcome::Different(e) => {
                failures.push(format!("{name}: lossless tree: {e}"))
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The cases above are not vacuous: the ones meant to be errors are.
#[test]
fn rejects_what_hhvm_rejects() {
    for name in ["unset of anything else", "bool casts", "missing semicolons"] {
        let source = CASES.iter().find(|c| c.0 == name).unwrap().1;
        assert!(
            matches!(
                compare_errors("case.hack", source),
                ErrorOutcome::SameErrors
            ),
            "{name}"
        );
    }
}
