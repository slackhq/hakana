//! Runs Hakana's parser and HHVM's over every Hack file in this repository's
//! test corpus and the bundled standard library, and requires that whenever
//! both accept a file they produce exactly the same AST, comments and
//! fixmes.
//!
//! This is the safety net for changes to the parser and for HHVM upgrades:
//! if HHVM starts lowering something differently, or gains syntax that the
//! native parser mis-reads, the affected files show up here. HHVM's parser is
//! only this test's reference: Hakana itself no longer uses it, so a file the
//! native parser cannot read is a file Hakana reports as invalid.
//!
//! For a larger corpus, point the `parse-bench` binary at a file list:
//! `parse-bench diff files.txt`, with `SHOW_FAILURES=1 DIFF_DUMP_LIMIT=20` to
//! get per-file reports and AST dumps of the first disagreements.

use std::path::{Path, PathBuf};

use hakana_parser_bench::{Outcome, compare};

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

#[test]
fn native_parser_agrees_with_hhvm() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = vec![];
    collect(&root.join("tests"), &mut files);
    collect(&root.join("third-party/hhvm/hphp/hsl"), &mut files);
    files.sort();
    assert!(
        files.len() > 500,
        "expected to find the test corpus, found {} files",
        files.len()
    );

    let mut same = 0;
    let mut declined = vec![];
    let mut failures = vec![];
    for path in &files {
        let Ok(contents) = std::fs::read_to_string(path) else {
            continue;
        };
        let name = path.display().to_string();
        match compare(&name, contents) {
            Outcome::Same => same += 1,
            Outcome::BothReject => {}
            Outcome::NativeDeclined => declined.push(name),
            // HHVM runs semantic checks the native parser does not have, so
            // it can report errors for files the native parser accepts.
            // That gap is tracked separately; it is not a mis-parse.
            Outcome::HhvmReportsErrors(_) => {}
            Outcome::HhvmRejects => failures.push(format!("{name}: accepted, but HHVM rejects it")),
            Outcome::NativeReportsErrors(e) => {
                failures.push(format!("{name}: reports `{e}`, but HHVM accepts the file"))
            }
            Outcome::Mismatch { what, .. } => failures.push(format!("{name}: mismatch in {what}")),
        }
    }

    assert!(
        failures.is_empty(),
        "the native parser disagrees with HHVM on {} file(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    // The native parser should handle practically all of the corpus itself.
    assert!(
        declined.len() * 100 < files.len(),
        "the native parser declined {} of {} files:\n{}",
        declined.len(),
        files.len(),
        declined.join("\n")
    );
    assert!(
        same * 10 > files.len() * 9,
        "only {same} of {} files compared",
        files.len()
    );
}
