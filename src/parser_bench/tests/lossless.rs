//! The lossless syntax tree (`hakana_parser::cst`) that linters and the
//! formatter read has to be HHVM's full-fidelity tree, node for node and
//! token for token, trivia included: the formatter's rules and the linters
//! address children by HHVM's names and positions.
//!
//! This compares the two over the Hack files in this repository's tests, the
//! bundled standard library and HHVM's formatter tests. That includes files
//! with syntax errors the parser recovers from: the tree then has to have the
//! shape HHVM's recovery gives it.
//!
//! For a larger corpus: `parse-bench cst files.txt`, with `SHOW_FAILURES=1`
//! for per-file reports and `CST_DUMP=1` to dump both trees of the first
//! file that differs.

use std::path::{Path, PathBuf};

use hakana_parser::ParserOptions;
use hakana_parser_bench::{CstOutcome, compare_cst, compare_cst_with};

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
            Some("hack" | "php" | "hhi")
        ) {
            out.push(path);
        }
    }
}

#[test]
fn lossless_tree_matches_hhvm() {
    // Files HHVM parses that the native parser cannot.
    const MAX_UNSUPPORTED: usize = 0;

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = vec![];
    collect(&root.join("tests"), &mut files);
    collect(&root.join("third-party/hhvm/hphp/hsl"), &mut files);
    collect(&root.join("third-party/hhvm/hphp/hack/hhi"), &mut files);
    collect(
        &root.join("third-party/hhvm/hphp/hack/test/hackfmt"),
        &mut files,
    );
    files.sort();
    assert!(
        files.len() > 2_500,
        "expected to find the test corpus, found {} files",
        files.len()
    );

    let mut same = 0;
    let mut unsupported = vec![];
    let mut different = vec![];
    for path in &files {
        let Ok(contents) = std::fs::read_to_string(path) else {
            continue;
        };
        let name = path.display().to_string();
        match compare_cst(&name, &contents) {
            CstOutcome::Same => same += 1,
            // HHVM recovers from every syntax error; where the native
            // parser gives up instead there is nothing to compare.
            CstOutcome::NoRecovery(_) => {}
            CstOutcome::NativeFailed(why) => unsupported.push(format!("{name}: {why}")),
            CstOutcome::Different(at) => different.push(format!("{name}: {at}")),
        }
    }

    eprintln!(
        "lossless trees: {} files; identical: {same}; unsupported: {}; different: {}",
        files.len(),
        unsupported.len(),
        different.len()
    );
    assert!(
        different.is_empty(),
        "the lossless tree differs from HHVM's for {} file(s):\n{}",
        different.len(),
        different.join("\n")
    );
    // A ratchet that has reached zero is still a ratchet.
    #[allow(clippy::absurd_extreme_comparisons)]
    let within_limit = unsupported.len() <= MAX_UNSUPPORTED;
    assert!(
        within_limit,
        "no lossless tree for {} files HHVM parses (was {MAX_UNSUPPORTED}):\n{}",
        unsupported.len(),
        unsupported.join("\n")
    );
    assert!(same > 2_000, "only {same} files compared");
}

/// The formatter parses with the settings of the code it formats, which
/// HHVM's parser honours: the same comparison over HHVM's formatter tests
/// with each of them the other way round.
#[test]
fn lossless_tree_matches_hhvm_with_other_parser_options() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = vec![];
    collect(
        &root.join("third-party/hhvm/hphp/hack/test/hackfmt"),
        &mut files,
    );
    files.sort();
    assert!(
        files.len() > 200,
        "expected to find HHVM's formatter tests, found {} files",
        files.len()
    );

    let settings = [
        ParserOptions {
            enable_xhp_class_modifier: false,
            disable_xhp_children_declarations: false,
        },
        ParserOptions {
            enable_xhp_class_modifier: true,
            disable_xhp_children_declarations: true,
        },
        ParserOptions {
            enable_xhp_class_modifier: false,
            disable_xhp_children_declarations: true,
        },
    ];
    let mut same = 0;
    let mut different = vec![];
    for path in &files {
        let Ok(contents) = std::fs::read_to_string(path) else {
            continue;
        };
        let name = path.display().to_string();
        for options in settings {
            match compare_cst_with(&name, &contents, options) {
                CstOutcome::Same => same += 1,
                CstOutcome::NoRecovery(why) | CstOutcome::NativeFailed(why) => {
                    different.push(format!("{name} with {options:?}: no tree: {why}"))
                }
                CstOutcome::Different(at) => {
                    different.push(format!("{name} with {options:?}: {at}"))
                }
            }
        }
    }
    assert!(
        different.is_empty(),
        "the lossless tree differs from HHVM's for {} file(s):\n{}",
        different.len(),
        different.join("\n")
    );
    assert!(same > 600, "only {same} comparisons made");
}
