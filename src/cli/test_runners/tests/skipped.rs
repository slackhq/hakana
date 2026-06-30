use crate::test_runners::integration_test::{IntegrationTest, TestArtifacts, TestContext};

pub struct SkippedTest;

impl IntegrationTest for SkippedTest {
    fn run(&self, ctx: TestContext) -> Result<TestArtifacts, String> {
        Ok(TestArtifacts::skipped(
            ctx.previous_scan_data,
            ctx.previous_analysis_result,
        ))
    }
}
