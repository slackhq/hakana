use hakana_code_info::analysis_result::AnalysisResult;
use hakana_logger::Logger;
use hakana_orchestrator::SuccessfulScanData;

use std::sync::Arc;
use std::time::Duration;
use crate::test_runners::tests::{CodeTransformTest, DiffTest, ExecutableCodeFinderTest, GotoDefinitionTest, LinterTest, MigrationCandidatesTest, ReferencesTest, SkippedTest, StandardAnalysisTest};
use super::test_runner::HooksProvider;

/// Shared context passed to every [`IntegrationTest`] implementation.
pub struct TestContext<'a> {
    pub dir: String,
    pub cwd: String,
    pub logger: Arc<Logger>,
    pub cache_dir: Option<&'a String>,
    pub build_checksum: &'a str,
    pub previous_scan_data: Option<SuccessfulScanData>,
    pub previous_analysis_result: Option<AnalysisResult>,
    pub hooks_provider: &'a dyn HooksProvider,
}

/// Outcome of a single integration test execution.
pub struct TestResult {
    /// (`"."` pass, `"F"` fail, `"S"` skipped)
    pub status_char: String,
    pub scan_data: Option<SuccessfulScanData>,
    pub analysis_result: Option<AnalysisResult>,
    pub time_in_analysis: Duration,
    pub diagnostic: Option<(String, String)>,
}

impl TestResult {
    pub fn pass(
        scan_data: Option<SuccessfulScanData>,
        analysis_result: Option<AnalysisResult>,
        time_in_analysis: Duration,
    ) -> Self {
        TestResult {
            status_char: ".".to_string(),
            scan_data,
            analysis_result,
            time_in_analysis,
            diagnostic: None,
        }
    }

    pub fn fail(
        dir: String,
        diagnostic: String,
        scan_data: Option<SuccessfulScanData>,
        analysis_result: Option<AnalysisResult>,
        time_in_analysis: Duration,
    ) -> Self {
        TestResult {
            status_char: "F".to_string(),
            scan_data,
            analysis_result,
            time_in_analysis,
            diagnostic: Some((dir, diagnostic)),
        }
    }

    pub fn skipped(
        scan_data: Option<SuccessfulScanData>,
        analysis_result: Option<AnalysisResult>,
    ) -> Self {
        TestResult {
            status_char: "S".to_string(),
            scan_data,
            analysis_result,
            time_in_analysis: Duration::default(),
            diagnostic: None,
        }
    }
}

/// A single integration test type.
///
/// Each variant of test (standard analysis, diff, linter, etc.) implements
/// this trait. The test runner selects the appropriate implementation based
/// on directory-name conventions and delegates execution via [`IntegrationTest::run`].
pub trait IntegrationTest {
    fn run(&self, ctx: TestContext) -> TestResult;
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