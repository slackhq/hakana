//! Runs Hakana's parser over HHVM's own test suites.
//!
//! * `full_fidelity_cases` is HHVM's parser test suite proper: each case is a
//!   file plus the syntax errors HHVM's parser is expected to report for it
//!   (`<case>.errors.exp`). The native parser has to agree on which files are
//!   clean, and should report the same errors.
//! * `hhvm_test_files_parse_the_same` runs both parsers over every Hack file
//!   in HHVM's test directories (about 26,000 of them, full of syntax no real
//!   codebase uses) and requires identical trees wherever both accept a file.
//!
//! * `hhvm_test_files_report_the_same_errors` and
//!   `hhvm_test_files_have_the_same_lossless_tree` run both parsers over
//!   those same files and compare, for every file, the errors reported (the
//!   whole list, in order) and the lossless tree (with the syntax errors the
//!   parser recovered from).
//!
//! Agreement is not total — HHVM has experimental syntax and checks the
//! native parser has no use for — so the tests hold the line at what has been
//! achieved: the numbers below may only move in the right direction. Run with
//! `--nocapture` for the current counts, and `SHOW_FAILURES=1` for the files.

use std::path::{Path, PathBuf};

use hakana_parser_bench::{
    CstOutcome, ErrorOutcome, Native, Outcome, compare, compare_cst, compare_errors, native,
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

fn hhvm_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third-party/hhvm/hphp")
}

fn show_failures() -> bool {
    std::env::var_os("SHOW_FAILURES").is_some()
}

/// `(line,column)-(line,column) message`, as HHVM's parser tests print errors.
fn format_error(src: &str, start: usize, end: usize, message: &str) -> String {
    let locate = |offset: usize| {
        let offset = offset.min(src.len());
        let before = &src.as_bytes()[..offset];
        let line = before.iter().filter(|&&b| b == b'\n').count() + 1;
        let bol = before
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        (line, offset - bol + 1)
    };
    let (start_line, start_column) = locate(start);
    let (end_line, end_column) = locate(end);
    format!("({start_line},{start_column})-({end_line},{end_column}) {message}")
}

#[test]
fn full_fidelity_cases() {
    // The most this may be: files HHVM's parser accepts that the native
    // parser reports an error for.
    const MAX_FALSE_POSITIVES: usize = 11;
    // Files with errors of which the native parser reports none.
    const MAX_MISSED: usize = 1;
    // Files where both report errors, but not the same ones.
    const MAX_DIFFERENT: usize = 112;

    let mut files = vec![];
    collect(
        &hhvm_root().join("hack/test/full_fidelity/cases"),
        &mut files,
    );
    files.sort();

    let (mut cases, mut exact) = (0, 0);
    let mut false_positives = vec![];
    let mut missed = vec![];
    let mut different = vec![];
    for path in &files {
        let expected_path = PathBuf::from(format!("{}.errors.exp", path.display()));
        let (Ok(src), Ok(expected)) = (
            std::fs::read_to_string(path),
            std::fs::read_to_string(&expected_path),
        ) else {
            continue;
        };
        cases += 1;
        let mut expected: Vec<&str> = expected.lines().filter(|l| !l.is_empty()).collect();
        let name = path.display().to_string();
        // The expected messages that name the file name it without its
        // directory.
        let file_name = path.file_name().unwrap().to_string_lossy();
        let (Native::Parsed { errors, .. } | Native::Failed(errors)) = native(&file_name, &src);
        let mut actual: Vec<String> = errors
            .iter()
            .map(|e| format_error(&src, e.start, e.end, &e.message))
            .collect();
        // HHVM's tool prints some errors twice.
        expected.sort();
        expected.dedup();
        actual.sort();
        actual.dedup();
        if expected
            .iter()
            .copied()
            .eq(actual.iter().map(String::as_str))
        {
            exact += 1;
        } else if expected.is_empty() {
            false_positives.push(format!("{name}: {}", actual.join(" | ")));
        } else if actual.is_empty() {
            missed.push(format!("{name}: {}", expected.join(" | ")));
        } else {
            different.push(format!(
                "{name}:\n    hhvm:   {}\n    native: {}",
                expected.join(" | "),
                actual.join(" | ")
            ));
        }
    }
    assert!(
        cases > 150,
        "expected HHVM's parser test cases, found {cases}"
    );

    eprintln!(
        "full_fidelity cases: {cases}; same errors: {exact}; false positives: {}; missed: {}; different: {}",
        false_positives.len(),
        missed.len(),
        different.len()
    );
    if show_failures() {
        for (title, list) in [
            ("FALSE POSITIVE", &false_positives),
            ("MISSED", &missed),
            ("DIFFERENT", &different),
        ] {
            for item in list {
                eprintln!("{title} {item}");
            }
        }
    }
    assert!(
        false_positives.len() <= MAX_FALSE_POSITIVES,
        "{} files HHVM's parser accepts are reported as invalid (was {MAX_FALSE_POSITIVES}):\n{}",
        false_positives.len(),
        false_positives.join("\n")
    );
    assert!(
        missed.len() <= MAX_MISSED,
        "{} invalid files are accepted without an error (was {MAX_MISSED}):\n{}",
        missed.len(),
        missed.join("\n")
    );
    assert!(
        different.len() <= MAX_DIFFERENT,
        "{} files get different errors than HHVM reports (was {MAX_DIFFERENT})",
        different.len()
    );
}

#[test]
fn hhvm_test_files_parse_the_same() {
    // Files HHVM parses cleanly that the native parser cannot parse: the
    // one left is nested deeper than the native parser goes.
    const MAX_UNSUPPORTED: usize = 1;

    let mut files = vec![];
    collect(&hhvm_root().join("hack/test"), &mut files);
    collect(&hhvm_root().join("test"), &mut files);
    files.sort();
    assert!(
        files.len() > 20_000,
        "expected HHVM's test suites, found {} files",
        files.len()
    );

    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = files.len().div_ceil(threads);
    let mut same = 0;
    let mut unsupported = vec![];
    let mut mismatches = vec![];
    std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|files| {
                std::thread::Builder::new()
                    // Some of these files nest very deeply.
                    .stack_size(64 * 1024 * 1024)
                    .spawn_scoped(scope, move || {
                        let mut same = 0;
                        let mut unsupported = vec![];
                        let mut mismatches = vec![];
                        for path in files {
                            let Ok(contents) = std::fs::read_to_string(path) else {
                                continue;
                            };
                            let name = path.display().to_string();
                            match compare(&name, contents) {
                                Outcome::Same => same += 1,
                                Outcome::NativeDeclined => unsupported.push(name),
                                Outcome::Mismatch { what, .. } => {
                                    mismatches.push(format!("{name}: mismatch in {what}"))
                                }
                                // Which files have errors, and which errors,
                                // is what `full_fidelity_cases` is about.
                                Outcome::BothReject
                                | Outcome::HhvmRejects
                                | Outcome::HhvmReportsErrors(_)
                                | Outcome::NativeReportsErrors(_) => {}
                            }
                        }
                        (same, unsupported, mismatches)
                    })
                    .unwrap()
            })
            .collect();
        for handle in handles {
            let (s, u, m) = handle.join().unwrap();
            same += s;
            unsupported.extend(u);
            mismatches.extend(m);
        }
    });

    eprintln!(
        "HHVM test files: {}; identical: {same}; unsupported: {}; mismatches: {}",
        files.len(),
        unsupported.len(),
        mismatches.len()
    );
    if show_failures() {
        for name in &unsupported {
            eprintln!("UNSUPPORTED {name}");
        }
    }
    assert!(
        mismatches.is_empty(),
        "the native parser reads {} file(s) differently from HHVM:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    assert!(
        unsupported.len() <= MAX_UNSUPPORTED,
        "the native parser cannot parse {} files HHVM accepts (was {MAX_UNSUPPORTED})",
        unsupported.len()
    );
    assert!(same > 15_000, "only {same} files compared");
}

/// Every Hack file in HHVM's test directories that is valid UTF-8.
fn hhvm_test_files() -> Vec<PathBuf> {
    let mut files = vec![];
    collect(&hhvm_root().join("hack/test"), &mut files);
    collect(&hhvm_root().join("test"), &mut files);
    files.sort();
    assert!(
        files.len() > 20_000,
        "expected HHVM's test suites, found {} files",
        files.len()
    );
    files
}

/// Runs `check` over the files on all cores and gathers what it has to say
/// about each (nothing, for a file that is as it should be).
fn check_files(
    files: &[PathBuf],
    check: impl Fn(&str, &str) -> Option<String> + Sync,
) -> Vec<String> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = files.len().div_ceil(threads);
    let check = &check;
    let mut found = vec![];
    std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|files| {
                std::thread::Builder::new()
                    // Some of these files nest very deeply.
                    .stack_size(64 * 1024 * 1024)
                    .spawn_scoped(scope, move || {
                        let mut found = vec![];
                        for path in files {
                            let Ok(contents) = std::fs::read_to_string(path) else {
                                continue;
                            };
                            let name = path.display().to_string();
                            found.extend(check(&name, &contents));
                        }
                        found
                    })
                    .unwrap()
            })
            .collect();
        for handle in handles {
            found.extend(handle.join().unwrap());
        }
    });
    found.sort();
    found
}

#[test]
fn hhvm_test_files_report_the_same_errors() {
    // Files the two parsers report different errors for. Those left are
    // nested deeper than the native parser goes: it says so, where HHVM
    // reports something else or nothing.
    const MAX_DIFFERENT: usize = 4;

    let files = hhvm_test_files();
    let different = check_files(&files, |name, contents| {
        match compare_errors(name, contents) {
            ErrorOutcome::BothClean | ErrorOutcome::SameErrors => None,
            ErrorOutcome::DifferentOrder { .. } => Some(format!("{name}: order")),
            ErrorOutcome::DifferentErrors { .. } => Some(format!("{name}: errors")),
            ErrorOutcome::Missed(_) => Some(format!("{name}: missed")),
            ErrorOutcome::FalsePositive(_) => Some(format!("{name}: false positive")),
        }
    });
    eprintln!(
        "HHVM test files: {}; different errors: {}",
        files.len(),
        different.len()
    );
    if show_failures() {
        for name in &different {
            eprintln!("DIFFERENT {name}");
        }
    }
    assert!(
        different.len() <= MAX_DIFFERENT,
        "{} files get different errors than HHVM reports (was {MAX_DIFFERENT}):\n{}",
        different.len(),
        different.join("\n")
    );
}

#[test]
fn hhvm_test_files_have_the_same_lossless_tree() {
    // Files HHVM has a tree for and the native parser has none: those
    // nested deeper than it goes.
    const MAX_WITHOUT_TREE: usize = 4;

    let files = hhvm_test_files();
    let found = check_files(&files, |name, contents| match compare_cst(name, contents) {
        CstOutcome::Same => None,
        CstOutcome::NoRecovery(why) | CstOutcome::NativeFailed(why) => {
            Some(format!("NO TREE {name}: {why}"))
        }
        CstOutcome::Different(at) => Some(format!("DIFFERENT {name}: {at}")),
    });
    let different: Vec<&String> = found
        .iter()
        .filter(|f| f.starts_with("DIFFERENT"))
        .collect();
    let without_tree = found.len() - different.len();
    eprintln!(
        "HHVM test files: {}; different lossless trees: {}; no tree: {without_tree}",
        files.len(),
        different.len()
    );
    if show_failures() {
        for line in &found {
            eprintln!("{line}");
        }
    }
    assert!(
        different.is_empty(),
        "the lossless tree of {} file(s) differs from HHVM's:\n{}",
        different.len(),
        different
            .iter()
            .map(|line| line.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        without_tree <= MAX_WITHOUT_TREE,
        "the native parser has no lossless tree for {without_tree} files HHVM has one for (was {MAX_WITHOUT_TREE})"
    );
}
