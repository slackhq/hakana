use hakana_code_info::analysis_result::AnalysisResult;
use hakana_orchestrator::SuccessfulScanData;

use super::test_runner::HooksProvider;
use crate::test_runners::tests::{
    CodeTransformTest, CyclomaticComplexityTest, DiffTest, ExecutableCodeFinderTest,
    GotoDefinitionTest, LinterTest, MigrationCandidatesTest, ReferencesTest, SkippedTest,
    StandardAnalysisTest,
};
use std::time::Duration;

/// Shared context passed to every [`IntegrationTest`] implementation.
pub struct TestContext<'a> {
    pub dir: String,
    pub cwd: String,
    pub cache_dir: Option<&'a String>,
    pub build_checksum: &'a str,
    pub previous_scan_data: Option<SuccessfulScanData>,
    pub previous_analysis_result: Option<AnalysisResult>,
    pub hooks_provider: &'a dyn HooksProvider,
}

/// One snapshot a test produces, together with the rules for verifying it
/// against — and rewriting — its committed expectation file.
///
/// A single test may produce several of these (e.g. `diff` checks both
/// `output.txt` and `definition_locations.json`, and the linter checks one
/// snapshot per `.in` file). The runner verifies each and, when
/// `--update-snapshots` is passed, rewrites the ones that don't match.
pub trait TestOutput {
    /// Path to the committed expectation file this output is compared against.
    /// The runner reports it alongside any failure so multi-snapshot tests make
    /// clear which file didn't match.
    fn expect_path(&self) -> String;

    /// `Ok(())` if the actual output matches the committed expectation,
    /// otherwise `Err(diagnostic)` describing the mismatch.
    fn verify(&self) -> Result<(), String>;

    /// Rewrite the expectation file with the actual output (for
    /// `--update-snapshots`). Returns `Err` for outputs that cannot be
    /// regenerated so the runner still reports
    /// them as failures even in update mode.
    fn update(&self) -> Result<(), String>;
}

/// Artifacts produced by a single integration test execution.
///
/// `scan_data`/`analysis_result` are threaded to the next test when codebase
/// reuse is enabled. `outputs` holds the snapshot comparisons the runner should
/// verify (empty means there is nothing to compare). `skipped` preserves the
/// `"S"` status char for skipped tests.
pub struct TestArtifacts {
    pub scan_data: Option<SuccessfulScanData>,
    pub analysis_result: Option<AnalysisResult>,
    pub time_in_analysis: Duration,
    pub outputs: Vec<Box<dyn TestOutput>>,
    pub skipped: bool,
}

impl TestArtifacts {
    /// Artifacts for a regular test with a set of snapshot outputs to verify.
    pub fn new(
        scan_data: Option<SuccessfulScanData>,
        analysis_result: Option<AnalysisResult>,
        time_in_analysis: Duration,
        outputs: Vec<Box<dyn TestOutput>>,
    ) -> Self {
        TestArtifacts {
            scan_data,
            analysis_result,
            time_in_analysis,
            outputs,
            skipped: false,
        }
    }

    /// Artifacts for a skipped test (renders as `"S"`, threads scan data along).
    pub fn skipped(
        scan_data: Option<SuccessfulScanData>,
        analysis_result: Option<AnalysisResult>,
    ) -> Self {
        TestArtifacts {
            scan_data,
            analysis_result,
            time_in_analysis: Duration::default(),
            outputs: vec![],
            skipped: true,
        }
    }
}

/// A single integration test type.
///
/// Each variant of test (standard analysis, diff, linter, etc.) implements
/// this trait. The test runner selects the appropriate implementation based
/// on directory-name conventions and delegates execution via [`IntegrationTest::run`].
pub trait IntegrationTest {
    /// Run the test, returning its artifacts (snapshots to verify) on success,
    /// or `Err(diagnostic)` for a hard scan/analysis failure that has no
    /// snapshot to compare. A hard error always fails the test, even under
    /// `--update-snapshots`.
    fn run(&self, ctx: TestContext) -> Result<TestArtifacts, String>;
}

/// Determine what type of integration test to run for a given directory.
pub(crate) fn select_test_type(dir: &str) -> Box<dyn IntegrationTest> {
    if dir.contains("skipped-") || dir.contains("SKIPPED-") {
        return Box::new(SkippedTest);
    }
    if dir.contains("/diff/") {
        return Box::new(DiffTest);
    }
    if dir.contains("/hhast_tests/") {
        return Box::new(LinterTest);
    }
    if dir.contains("/goto-definition/") {
        return Box::new(GotoDefinitionTest);
    }
    if dir.contains("/references/") {
        return Box::new(ReferencesTest);
    }
    if dir.contains("/executable-code-finder/") {
        return Box::new(ExecutableCodeFinderTest);
    }
    if dir.contains("/cyclomatic-complexity/") {
        return Box::new(CyclomaticComplexityTest);
    }
    if dir.contains("/migrations/")
        || dir.contains("/fix/")
        || dir.contains("/add-fixmes/")
        || dir.contains("/remove-unused-fixmes/")
    {
        return Box::new(CodeTransformTest);
    }
    if dir.contains("/migration-candidates/") {
        return Box::new(MigrationCandidatesTest);
    }
    Box::new(StandardAnalysisTest)
}
