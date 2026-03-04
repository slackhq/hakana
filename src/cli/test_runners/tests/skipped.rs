use crate::test_runners::integration_test::{IntegrationTest, TestContext, TestResult};

pub struct SkippedTest;

impl IntegrationTest for SkippedTest {
    fn run(&self, ctx: TestContext) -> TestResult {
        TestResult::skipped(ctx.previous_scan_data, ctx.previous_analysis_result)
    }
}
