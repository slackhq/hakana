mod code_transform;
mod cyclomatic_complexity;
mod diff;
mod executable_code_finder;
mod goto_definition;
mod linter;
mod migration_candidates;
mod references;
mod skipped;
mod standard_analysis;

pub use code_transform::CodeTransformTest;
pub use cyclomatic_complexity::CyclomaticComplexityTest;
pub use diff::DiffTest;
pub use executable_code_finder::ExecutableCodeFinderTest;
pub use goto_definition::GotoDefinitionTest;
pub use linter::LinterTest;
pub use migration_candidates::MigrationCandidatesTest;
pub use references::ReferencesTest;
pub use skipped::SkippedTest;
pub use standard_analysis::StandardAnalysisTest;

/// Where the XHP library the tests are analysed with is: a copy of
/// webapp's, kept either with the tests (`tests/xhp-lib`) or with the other
/// vendored code (`third-party/xhp-lib`).
pub(crate) fn xhp_lib_dir(cwd: &str) -> String {
    let with_tests = format!("{cwd}/tests/xhp-lib/src");
    if std::path::Path::new(&with_tests).is_dir() {
        with_tests
    } else {
        format!("{cwd}/third-party/xhp-lib/src")
    }
}
