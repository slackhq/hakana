# Hakana

Hakana is a typechecker for Hack, built by Slack.

[Online Playground](https://hakana.dev)

It complements the existing Hack typechecker that comes bundled with HHVM by providing additional insights beyond the scope of the official typechecker.

Hakana’s primary goal is to infer accurate types in the codebase, and to do so quickly.

Good type inference allows Hakana to do several other valuable things:

 - [Security Analysis](https://github.com/slackhq/hakana/blob/main/docs/security_analysis.md)
 - Custom type-aware migrations
 - Detection of potential logic bugs
 - Discovery and removal of dead code

## Non-goals

This is not intended to replace Hack's default typechecker, which supports extra features (e.g. contexts and capabilities) that aren't immediately relevant to type inference.

This tool is not designed to be run on every keypress, but it has a language server with support for on-save checks.

## Building from source

Clone this repo, install Rust and Cargo if you haven't already.

Run `./init.sh` to initialize the pinned submodules and apply `hhvm-patch.diff`.
The patch enables WASM compilation and retains Hakana's support for legacy bool
casts and `unset` targets.

`third-party/xhp-lib` is vendored from the webapp checkout's
`vendor/facebook/xhp-lib`. Its source revision is recorded in
`third-party/xhp-lib.vendor.json`. Keep this directory identical to that copy when
refreshing it; XHP no longer requires a separate submodule checkout.

Then run `cargo build --release`

That will create a binary at `./target/release/hakana`

## Running tests

You can run all tests with: `cargo run --bin hakana --release test tests`

You can run an individual test with `cargo run --bin hakana test <path-to-test-dir>`

## Formatting Hack

`hakana format` implements HHVM's hackfmt rules in Rust using the same full-fidelity
parser as Hakana. It preserves comments and honors generated/manual sections,
formatting suppressions, and `hackfmt.*` settings in the nearest `.hhconfig`.
It accepts both tagged PHP files and tagless `.hack` files.

```sh
hakana format source.hack                     # Print formatted source
hakana format < source.hack                   # Read from stdin
hakana format --in-place source.hack other.hack
hakana format --in-place src/ other.hack      # Recursively format a directory
hakana format --check-formatting src/         # Recursively check a directory
hakana format --check-formatting source.hack  # Print paths needing formatting
hakana format --line-range 10 20 source.hack  # Print formatted selected lines
git diff -U0 | hakana format --diff-dry-run --root .
```

`--check-formatting` retains hackfmt's exit-code behavior: finding a formatting
change prints the path and does not itself make the command fail. Use
`hakana format --help` for width, indentation, range, and formatting-version options.

In-place, check, and `--linttool` modes accept any mix of files and directories.
Directories are searched recursively for `.hack`, `.php`, `.hhi`, and `.hh` files,
following symlinks like `hakana lint`. Overlapping inputs are deduplicated, and
directory entries are sorted for stable output. Traversal errors are reported.
Without `--root`, configuration is found starting from the first input file's
parent or the first input directory itself.

Batch check, in-place, lint, and diff modes format files in parallel, using the
available CPU count capped at eight workers by default. Set `--jobs N` (or `-j N`)
to choose the limit; `--jobs 1` runs sequentially. Diagnostics and writes retain
each mode's existing order. Single-file/stdin requests and WASM use sequential formatting.

The `hakana-format` library exposes whole-file, byte-range, line-interval, and
format-at-offset operations independently of type analysis. To verify compatibility:

```sh
cargo build --release --bin hakana
python3 scripts/test_hackfmt.py
cargo test -p hakana-format
```

The harness reads fixtures, flags, configuration, expected output, IDE cases, and
OCaml range assertions directly from `third-party/hhvm/hphp/hack/test`; it does not
copy or regenerate them. Rust tests additionally check exact whole-file idempotence
on those upstream fixtures. For faster development, build `cargo build -p hakana-format`
and run the harness with `--program target/debug/hakana-format --standalone`.

To benchmark a source directory using its nearest `.hhconfig` without editing
the source files:

```bash
python3 scripts/benchmark_hackfmt.py path/to/source
python3 scripts/benchmark_hackfmt.py path/to/source --baseline /tmp/hakana-before
```

The baseline option alternates timing runs and checks that both binaries produce
identical output for every file, using temporary copies. Save the old release
binary before rebuilding. Add `--json timings.json` to save measurements or
`--profile profile.json.gz` to record a separate CPU profile with `samply`.
Profiles include worker threads. Use `--jobs N` and `--baseline-jobs N` to compare
worker counts when both binaries support these options.

## Type assertion helpers

Type-checking helpers can opt into the impossible and redundant comparison diagnostics
used by `is` and `as` with `<<Hakana\TypeAssertion('$value', 'T')>>`. The first argument
names the value parameter; the second names a generic type parameter on the function,
method, or containing class. Hakana resolves that type at each call using explicit type
arguments or inference (for example, from `typename<T>` or `TypeStructure<T>`).

```hack
<<Hakana\TypeAssertion('$value', 'T')>>
function verify_type<reify T>(mixed $value): ?T {
    return $value ?as T;
}
```

The annotation checks against `T`, independently of the return type. A helper returning
`?T` therefore behaves like `?as T` for diagnostics: its failure null does not make an
otherwise impossible assertion possible. Nullable types within `T` are preserved.
The annotation does not narrow the input or change the helper's return type, making it
suitable for sampled checks and helpers returning booleans or error objects as well.

A redundant diagnostic means the runtime type check always succeeds according to
Hakana's inferred types; it does not guarantee that Hack accepts removing the call.
Hakana retains individual dictionary field types and compares dictionaries and shapes
structurally, while Hack distinguishes them at static type boundaries. Prefer shape
literals for fixtures passed as shapes, and `Shapes::toDict` where a dictionary is
required. A conversion back to a shape after editing dynamic keys may still need to
be retained with `HAKANA_IGNORE[RedundantTypeComparison]`.

## Unused parts of function signatures

With unused-definition analysis enabled, allow `UnusedReturnValue` to report
functions and methods whose non-void return value is never consumed by a
production caller. The diagnostic and its `HAKANA_FIXME` belong on the
declaration. A single production use is sufficient, even if other callers
discard the result. `await f();` and `Asio\join(f());` discard the resulting
value; returning or assigning it counts as a use.

`UnusedOptionalParameter` reports a parameter with a default that production
callers never explicitly supply. Passing the default value explicitly counts
as supplying the parameter. Suppress intentional API options on the declaration
or the individual parameter with `HAKANA_FIXME[UnusedOptionalParameter]`.

Both checks disregard test files and `Hakana\TestOnly` callers. Never-called
definitions retain their existing unused-function/method diagnostics. Override
families share usage; escaped function pointers and argument unpacking are
treated conservatively. Generated and dynamically callable definitions are
excluded. Void, noreturn, unknown/mixed results and generators are excluded from
the return check. Explicit `Hakana\MustUse` retains its existing call-site
`UnusedFunctionCall` diagnostic, including in tests.

`hakana fix --issue UnusedOptionalParameter` removes unused optional parameters
and initializes local variables from their defaults at the start of the body.
It updates interface/override families together and expands promoted parameters
into explicit properties and assignments. Parameters explicitly supplied by
tests, escaped callbacks, suppressed declarations, and signatures that cannot
be changed together are left for review. Locals are initialized directly from
the defaults. Re-run analysis afterward and review newly redundant conditions
or unused locals separately.

Use `--parameter-allowlist path.json` to restrict this fix to a JSON array of
fully qualified `function::$parameter` or `Class::method::$parameter` names.
Every affected production declaration in an override family must be allowed;
test implementations follow their production contract. An empty
allowlist performs no edits. The fixer itself does not infer parameter age.
