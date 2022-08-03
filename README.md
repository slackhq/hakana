# Hakana

Hakana is a typechecker for Hack, built by Slack.

It complements the existing Hack typechecker that comes bundled with HHVM by providing additional insights beyond the scope of the official typechecker.

Hakana’s primary goal is to infer accurate types in the codebase, and to do so quickly.

Good type inference allows Hakana to do several other valuable things:

 - Security Analysis
 - Custom type-aware migrations
 - Detection of potential logic bugs
 - Discovery and removal of dead code

## Non-goals

This is not intended to replace Hack's default typechecker, which supports extra features (e.g. contexts and capabilities) that aren't immediately relevant to type inference.

This tool is not designed to be run on every keypress — LSP integration is not on the roadmap.

## Building from source

Clone this repo, install Rust and Cargo if you haven't already.

Run `git submodule init && git submodule update` to ensure HHVM is present (Hakana borrows HHVM's parser).

Then run `cargo build --release`

That will create a binary at `./target/release/hakana-default`

## Running tests

You can run all tests with: `cargo run --release test tests`

You can run an individual test with `cargo run test <path-to-test-dir>`